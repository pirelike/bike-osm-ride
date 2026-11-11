//! Decides *when* a new panorama is needed and *what* the viewer should look at, and keeps
//! the visible state from flickering. Pure logic (time is passed in), no I/O.
//!
//! Policy:
//!
//! * a new panorama is requested when the rider moved >= 20 m since the last request, or
//!   at least 4 s passed *and* the rider moved at least 3 m (a stopped rider does not reload the
//!   iframe); never more often than every 1.5 s; a jump of at least 300 m (seek) is immediate;
//! * the heading is the circular mean of the route direction 3, 8, 13 and 18 m ahead, so the
//!   view looks where the road goes instead of jittering with every 10 m route point;
//!   pitch 2 degrees, field of view 90 degrees;
//! * hysteresis: while a panorama is showing, up to two "no panorama here" answers (or
//!   transient network errors) in a row keep it on screen; the panel switches to
//!   "not available" on the third. After an outage two successes in a row are needed before
//!   the picture comes back. Key / quota problems are shown at once and never retried fast.

use ride_model::Route;

use crate::cache::bucket_of;
use crate::error::SvError;
use crate::url::View;

/// Tunables (defaults are the product policy).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlannerConfig {
    pub min_move_m: f64,
    pub min_interval_s: f64,
    /// Movement needed for the time rule to fire.
    pub time_rule_floor_m: f64,
    /// Hard lower bound between two requests.
    pub min_gap_s: f64,
    /// Distance jump that counts as a seek and is answered at once.
    pub seek_jump_m: f64,
    /// Offsets ahead of the rider whose route headings are averaged.
    pub heading_offsets_m: [f64; 4],
    pub pitch: f64,
    pub fov: f64,
    /// A new answer for the same panorama only reloads when the heading moved this much.
    pub heading_eps_deg: f64,
    /// Consecutive misses before "not available" replaces a showing panorama.
    pub miss_limit: u32,
    /// Consecutive successes before the picture returns after an outage.
    pub recover_limit: u32,
    /// Retry interval after a transient failure / after a quota answer.
    pub retry_s: f64,
    pub quota_retry_s: f64,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        PlannerConfig {
            min_move_m: 20.0,
            min_interval_s: 4.0,
            time_rule_floor_m: 3.0,
            min_gap_s: 1.5,
            seek_jump_m: 300.0,
            heading_offsets_m: [3.0, 8.0, 13.0, 18.0],
            pitch: 2.0,
            fov: 90.0,
            heading_eps_deg: 6.0,
            miss_limit: 3,
            recover_limit: 2,
            retry_s: 4.0,
            quota_retry_s: 20.0,
        }
    }
}

/// What the panel shows right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// Nothing answered yet.
    Checking,
    Available,
    Unavailable(SvError),
}

/// A request the planner wants resolved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub seq: u64,
    /// Rider distance when asked.
    pub dist: f64,
    /// Cache bucket of that distance.
    pub bucket: u32,
    /// Smoothed look-ahead heading, degrees 0..360.
    pub heading: f64,
}

/// How a request ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Found { pano: String },
    NoPano,
    Failed(SvError),
}

/// What the viewer must show (a new double-buffer load).
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub pano: String,
    pub view: View,
}

/// What the panel has to do after an outcome was applied.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Nothing,
    Show(Shown),
    /// Switch the panel to its "not available" / error state.
    Unavailable(SvError),
}

/// Circular mean of angles in degrees, `0..360`. `None` for no input.
pub fn circular_mean_deg(angles: &[f64]) -> Option<f64> {
    if angles.is_empty() {
        return None;
    }
    let (mut s, mut c) = (0.0f64, 0.0f64);
    for a in angles.iter().filter(|a| a.is_finite()) {
        s += a.to_radians().sin();
        c += a.to_radians().cos();
    }
    if s.abs() < 1e-9 && c.abs() < 1e-9 {
        return angles
            .iter()
            .copied()
            .find(|a| a.is_finite())
            .map(|a| a.rem_euclid(360.0));
    }
    Some(s.atan2(c).to_degrees().rem_euclid(360.0))
}

/// Smoothed direction of travel at `dist`, looking a few metres ahead along the route.
pub fn smoothed_heading(route: &Route, dist: f64, offsets: &[f64]) -> f64 {
    let hs: Vec<f64> = offsets
        .iter()
        .map(|o| route.locate(dist + o).heading_deg)
        .collect();
    circular_mean_deg(&hs).unwrap_or_else(|| route.locate(dist).heading_deg)
}

/// Smallest absolute difference of two angles, degrees `0..=180`.
pub fn angle_diff_deg(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

#[derive(Debug, Clone)]
pub struct Planner {
    cfg: PlannerConfig,
    seq: u64,
    applied_seq: u64,
    last_request: Option<(f64, f64)>,
    shown: Option<Shown>,
    availability: Availability,
    misses: u32,
    successes: u32,
    retry_after: Option<f64>,
    halted: bool,
}

impl Default for Planner {
    fn default() -> Self {
        Planner::new(PlannerConfig::default())
    }
}

impl Planner {
    pub fn new(cfg: PlannerConfig) -> Planner {
        Planner {
            cfg,
            seq: 0,
            applied_seq: 0,
            last_request: None,
            shown: None,
            availability: Availability::Checking,
            misses: 0,
            successes: 0,
            retry_after: None,
            halted: false,
        }
    }

    pub fn availability(&self) -> Availability {
        self.availability
    }

    pub fn shown(&self) -> Option<&Shown> {
        self.shown.as_ref()
    }

    /// After a key / request error no further request is planned (until a new planner is
    /// made for the changed settings).
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// Called whenever the ride position updates; returns the request to resolve, if any.
    pub fn tick(&mut self, now_s: f64, route: &Route, dist: f64) -> Option<Plan> {
        if self.halted || !dist.is_finite() || !now_s.is_finite() {
            return None;
        }
        let dist = dist.clamp(0.0, route.total_dist.max(0.0));
        let due = match self.last_request {
            None => true,
            Some((d0, t0)) => {
                let moved = (dist - d0).abs();
                let dt = now_s - t0;
                if let Some(wait) = self.retry_after {
                    dt >= wait
                } else if moved >= self.cfg.seek_jump_m {
                    true
                } else if dt < self.cfg.min_gap_s {
                    false
                } else {
                    moved >= self.cfg.min_move_m
                        || (dt >= self.cfg.min_interval_s && moved >= self.cfg.time_rule_floor_m)
                }
            }
        };
        if !due {
            return None;
        }
        self.last_request = Some((dist, now_s));
        self.retry_after = None;
        self.seq += 1;
        Some(Plan {
            seq: self.seq,
            dist,
            bucket: bucket_of(dist),
            heading: smoothed_heading(route, dist, &self.cfg.heading_offsets_m),
        })
    }

    /// Feeds the result of a plan back in and returns what the panel must do.
    pub fn apply(&mut self, plan: &Plan, outcome: Outcome) -> Effect {
        if plan.seq < self.applied_seq {
            return Effect::Nothing; // an older answer arrived late
        }
        self.applied_seq = plan.seq;
        match outcome {
            Outcome::Found { pano } => {
                self.misses = 0;
                self.successes = self.successes.saturating_add(1);
                self.retry_after = None;
                let recovering = matches!(self.availability, Availability::Unavailable(_));
                if recovering && self.successes < self.cfg.recover_limit {
                    return Effect::Nothing;
                }
                self.availability = Availability::Available;
                let view = View {
                    heading: plan.heading,
                    pitch: self.cfg.pitch,
                    fov: self.cfg.fov,
                }
                .normalized();
                if let Some(s) = &self.shown {
                    if s.pano == pano
                        && angle_diff_deg(s.view.heading, view.heading) < self.cfg.heading_eps_deg
                    {
                        return Effect::Nothing;
                    }
                }
                let shown = Shown { pano, view };
                self.shown = Some(shown.clone());
                Effect::Show(shown)
            }
            Outcome::NoPano => self.miss(SvError::NoCoverage, false),
            Outcome::Failed(e) if e.is_no_coverage() => self.miss(SvError::NoCoverage, false),
            Outcome::Failed(e) if e.is_permanent() => {
                self.halted = true;
                self.set_unavailable(e)
            }
            Outcome::Failed(SvError::QuotaExceeded) => {
                self.successes = 0;
                self.retry_after = Some(self.cfg.quota_retry_s);
                self.set_unavailable(SvError::QuotaExceeded)
            }
            Outcome::Failed(e) => self.miss(e, true),
        }
    }

    /// A "nothing to show" answer: only switches the panel after `miss_limit` in a row.
    fn miss(&mut self, reason: SvError, retry: bool) -> Effect {
        self.successes = 0;
        self.misses = self.misses.saturating_add(1);
        if retry {
            self.retry_after = Some(self.cfg.retry_s);
        }
        let keep_showing = matches!(self.availability, Availability::Available)
            && self.misses < self.cfg.miss_limit;
        if keep_showing {
            return Effect::Nothing;
        }
        if self.availability == Availability::Unavailable(reason) {
            return Effect::Nothing;
        }
        self.set_unavailable(reason)
    }

    fn set_unavailable(&mut self, e: SvError) -> Effect {
        self.successes = 0;
        if self.availability == Availability::Unavailable(e) {
            return Effect::Nothing;
        }
        self.availability = Availability::Unavailable(e);
        Effect::Unavailable(e)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn circular_mean_ignores_nonfinite_values_even_in_degenerate_cases() {
        assert_eq!(super::circular_mean_deg(&[f64::NAN]), None);
        assert_eq!(super::circular_mean_deg(&[f64::NAN, 0.0, 180.0]), Some(0.0));
    }

    use super::*;
    use ride_model::testing::straight_route;

    fn flat() -> Route {
        straight_route(&[(5000.0, 0.0)], 100.0)
    }

    fn found(id: &str) -> Outcome {
        Outcome::Found { pano: id.into() }
    }

    #[test]
    fn first_tick_requests_immediately() {
        let r = flat();
        let mut p = Planner::default();
        let plan = p.tick(0.0, &r, 100.0).expect("plan");
        assert_eq!(plan.seq, 1);
        assert_eq!(plan.bucket, 4);
        assert_eq!(plan.dist, 100.0);
        // the test route runs east
        assert!(angle_diff_deg(plan.heading, 90.0) < 1.0, "{}", plan.heading);
    }

    #[test]
    fn distance_rule_time_rule_and_floor() {
        let r = flat();
        let mut p = Planner::default();
        p.tick(0.0, &r, 100.0).expect("first");
        // too early / too short
        assert!(p.tick(1.0, &r, 110.0).is_none(), "min gap");
        assert!(p.tick(2.0, &r, 115.0).is_none(), "15 m < 20 m, 2 s < 4 s");
        // 20 m moved, gap ok
        assert!(p.tick(2.0, &r, 120.0).is_some(), "20 m rule");
        // time rule: 4 s and at least 3 m
        assert!(p.tick(5.0, &r, 122.0).is_none(), "only 2 m, 3 s");
        assert!(p.tick(6.5, &r, 122.0).is_none(), "2 m is below the floor");
        assert!(p.tick(6.5, &r, 124.0).is_some(), "4.5 s and 4 m");
        // standing still never reloads
        assert!(p.tick(60.0, &r, 124.0).is_none());
        assert!(p.tick(120.0, &r, 124.5).is_none());
    }

    #[test]
    fn fast_riding_is_rate_limited_and_seek_is_immediate() {
        let r = flat();
        let mut p = Planner::default();
        p.tick(0.0, &r, 0.0).expect("first");
        // 30 m in 1 s (descent): the 1.5 s gap holds it back
        assert!(p.tick(1.0, &r, 30.0).is_none());
        assert!(p.tick(1.5, &r, 45.0).is_some());
        // a seek forward ignores the gap
        assert!(p.tick(1.6, &r, 2000.0).is_some());
        assert!(p.tick(1.7, &r, 100.0).is_some(), "seek back as well");
    }

    #[test]
    fn nan_inputs_and_clamping() {
        let r = flat();
        let mut p = Planner::default();
        assert!(p.tick(0.0, &r, f64::NAN).is_none());
        assert!(p.tick(f64::NAN, &r, 5.0).is_none());
        let plan = p.tick(0.0, &r, 99_999.0).expect("plan");
        assert_eq!(plan.dist, r.total_dist);
        let plan = p.tick(10.0, &r, -50.0).expect("plan");
        assert_eq!((plan.dist, plan.bucket), (0.0, 0));
    }

    #[test]
    fn heading_is_smoothed_and_wraps_around_north() {
        assert_eq!(circular_mean_deg(&[]), None);
        let m = circular_mean_deg(&[350.0, 10.0]).expect("mean");
        assert!(angle_diff_deg(m, 0.0) < 1e-6, "{m}");
        let m = circular_mean_deg(&[80.0, 100.0]).expect("mean");
        assert!((m - 90.0).abs() < 1e-6);
        assert_eq!(
            circular_mean_deg(&[90.0, 270.0]),
            Some(90.0),
            "opposite: first wins"
        );
        assert_eq!(angle_diff_deg(359.0, 1.0), 2.0);
        assert_eq!(angle_diff_deg(0.0, 180.0), 180.0);

        // an L-shaped route: east for 100 m, then north. Just before the corner the
        // smoothed heading already turns, but less than the raw next-segment heading.
        let mut r = flat();
        r.points.clear();
        let m_lon = 111_320.0 * 47.5f64.to_radians().cos();
        let m_lat = 111_320.0;
        let mut pts = Vec::new();
        for i in 0..=10 {
            pts.push((
                47.5,
                19.0 + f64::from(i) * 10.0 / m_lon,
                f64::from(i) * 10.0,
            ));
        }
        for j in 1..=10 {
            pts.push((
                47.5 + f64::from(j) * 10.0 / m_lat,
                19.0 + 100.0 / m_lon,
                100.0 + f64::from(j) * 10.0,
            ));
        }
        r.points = pts
            .into_iter()
            .map(|(lat, lon, dist)| ride_model::RoutePoint {
                lat,
                lon,
                ele: 100.0,
                dist,
                grade: 0.0,
            })
            .collect();
        r.total_dist = 200.0;
        let offs = PlannerConfig::default().heading_offsets_m;
        let far = smoothed_heading(&r, 40.0, &offs);
        assert!(angle_diff_deg(far, 90.0) < 1.0, "{far}");
        let near = smoothed_heading(&r, 90.0, &offs);
        assert!(near < 90.0 && near > 10.0, "partly turned: {near}");
        let after = smoothed_heading(&r, 150.0, &offs);
        assert!(angle_diff_deg(after, 0.0) < 1.0, "{after}");
    }

    fn plan_at(p: &mut Planner, t: f64, d: f64) -> Plan {
        p.tick(t, &flat(), d).expect("plan")
    }

    #[test]
    fn found_shows_then_same_pano_does_not_reload() {
        let mut p = Planner::default();
        assert_eq!(p.availability(), Availability::Checking);
        let a = plan_at(&mut p, 0.0, 0.0);
        let eff = p.apply(&a, found("P1"));
        let Effect::Show(s) = eff else {
            panic!("expected Show, got {eff:?}")
        };
        assert_eq!(s.pano, "P1");
        assert_eq!((s.view.pitch, s.view.fov), (2.0, 90.0));
        assert_eq!(p.availability(), Availability::Available);
        // same pano, same heading: nothing to do
        let b = plan_at(&mut p, 5.0, 8.0);
        assert_eq!(p.apply(&b, found("P1")), Effect::Nothing);
        // other pano: reload
        let c = plan_at(&mut p, 10.0, 40.0);
        assert!(matches!(p.apply(&c, found("P2")), Effect::Show(_)));
    }

    #[test]
    fn same_pano_new_heading_reloads_only_for_a_real_turn() {
        let mut p = Planner::default();
        let mut a = plan_at(&mut p, 0.0, 0.0);
        a.heading = 90.0;
        assert!(matches!(p.apply(&a, found("P")), Effect::Show(_)));
        let mut b = plan_at(&mut p, 5.0, 8.0);
        b.heading = 94.0;
        assert_eq!(p.apply(&b, found("P")), Effect::Nothing);
        let mut c = plan_at(&mut p, 10.0, 16.0);
        c.heading = 100.0;
        assert!(matches!(p.apply(&c, found("P")), Effect::Show(_)));
    }

    #[test]
    fn hysteresis_keeps_the_picture_through_short_gaps() {
        let mut p = Planner::default();
        let a = plan_at(&mut p, 0.0, 0.0);
        p.apply(&a, found("P1"));
        let mut t = 5.0;
        let mut d = 30.0;
        let mut next = |p: &mut Planner| {
            t += 5.0;
            d += 30.0;
            plan_at(p, t, d)
        };
        // two misses: still showing, no flicker
        let m1 = next(&mut p);
        assert_eq!(p.apply(&m1, Outcome::NoPano), Effect::Nothing);
        assert_eq!(p.availability(), Availability::Available);
        let m2 = next(&mut p);
        assert_eq!(p.apply(&m2, Outcome::NoPano), Effect::Nothing);
        // a hit resets the streak
        let h = next(&mut p);
        assert_eq!(p.apply(&h, found("P1")), Effect::Nothing);
        let m3 = next(&mut p);
        assert_eq!(p.apply(&m3, Outcome::NoPano), Effect::Nothing);
        let m4 = next(&mut p);
        assert_eq!(p.apply(&m4, Outcome::NoPano), Effect::Nothing);
        // third miss in a row flips
        let m5 = next(&mut p);
        assert_eq!(
            p.apply(&m5, Outcome::NoPano),
            Effect::Unavailable(SvError::NoCoverage)
        );
        // more misses: no repeated effect
        let m6 = next(&mut p);
        assert_eq!(p.apply(&m6, Outcome::NoPano), Effect::Nothing);
        // first success after the outage is held back, the second brings the picture
        let r1 = next(&mut p);
        assert_eq!(p.apply(&r1, found("P9")), Effect::Nothing);
        assert_eq!(
            p.availability(),
            Availability::Unavailable(SvError::NoCoverage)
        );
        let r2 = next(&mut p);
        assert!(matches!(p.apply(&r2, found("P9")), Effect::Show(_)));
        assert_eq!(p.availability(), Availability::Available);
    }

    #[test]
    fn alternating_hits_and_misses_never_flip_the_panel() {
        let mut p = Planner::default();
        let a = plan_at(&mut p, 0.0, 0.0);
        p.apply(&a, found("P1"));
        let mut effects = 0;
        for i in 0..40 {
            let plan = plan_at(&mut p, 5.0 * f64::from(i + 1), 30.0 * f64::from(i + 1));
            let o = if i % 2 == 0 {
                Outcome::NoPano
            } else {
                found("P1")
            };
            if matches!(p.apply(&plan, o), Effect::Unavailable(_)) {
                effects += 1;
            }
        }
        assert_eq!(effects, 0);
    }

    #[test]
    fn first_answer_no_pano_is_shown_immediately() {
        let mut p = Planner::default();
        let a = plan_at(&mut p, 0.0, 0.0);
        assert_eq!(
            p.apply(&a, Outcome::NoPano),
            Effect::Unavailable(SvError::NoCoverage)
        );
    }

    #[test]
    fn permanent_errors_halt_and_show_at_once() {
        let mut p = Planner::default();
        let a = plan_at(&mut p, 0.0, 0.0);
        p.apply(&a, found("P1"));
        let b = plan_at(&mut p, 5.0, 30.0);
        assert_eq!(
            p.apply(&b, Outcome::Failed(SvError::KeyRejected)),
            Effect::Unavailable(SvError::KeyRejected)
        );
        assert!(p.is_halted());
        assert!(p.tick(100.0, &flat(), 500.0).is_none());
    }

    #[test]
    fn quota_is_shown_at_once_and_retried_slowly() {
        let mut p = Planner::default();
        let a = plan_at(&mut p, 0.0, 0.0);
        p.apply(&a, found("P1"));
        let b = plan_at(&mut p, 5.0, 30.0);
        assert_eq!(
            p.apply(&b, Outcome::Failed(SvError::QuotaExceeded)),
            Effect::Unavailable(SvError::QuotaExceeded)
        );
        assert!(!p.is_halted());
        // 25 m further after 6 s would normally be due; the quota back-off holds it
        assert!(p.tick(11.0, &flat(), 80.0).is_none());
        assert!(p.tick(25.0, &flat(), 80.0).is_some());
    }

    #[test]
    fn transient_errors_retry_even_when_standing_still() {
        let mut p = Planner::default();
        let a = plan_at(&mut p, 0.0, 0.0);
        p.apply(&a, found("P1"));
        let b = plan_at(&mut p, 5.0, 30.0);
        assert_eq!(
            p.apply(&b, Outcome::Failed(SvError::Network)),
            Effect::Nothing
        );
        assert!(
            p.tick(6.0, &flat(), 30.0).is_none(),
            "before the retry time"
        );
        assert!(p.tick(9.1, &flat(), 30.0).is_some(), "retry without moving");
    }

    #[test]
    fn late_answers_are_ignored() {
        let mut p = Planner::default();
        let first = plan_at(&mut p, 0.0, 0.0);
        let second = plan_at(&mut p, 5.0, 30.0);
        assert!(matches!(p.apply(&second, found("NEW")), Effect::Show(_)));
        assert_eq!(p.apply(&first, found("OLD")), Effect::Nothing);
        assert_eq!(p.shown().map(|s| s.pano.as_str()), Some("NEW"));
    }
}
