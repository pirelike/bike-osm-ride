//! The virtual gearbox: a 2 x 12 drivetrain (50/34 chainrings, 11-34 cassette) whose gears are
//! shifted with the keys or the gear panel.
//!
//! A smart trainer has no gears of its own, so the gear is turned into what the trainer can do:
//! its resistance. Each gear shifts the grade that is sent to the trainer by a few percent
//! (the easiest gear feels like a gentle descent compared to the reference gear, the hardest like
//! a steep ramp), exactly what the rider would feel as "a gear too hard / too easy for this road".
//! The ride itself (distance, speed on the map) still comes from power and the real grade, like
//! in every trainer app; the gear only changes how that power has to be produced.

use std::sync::OnceLock;

/// Chainring teeth, large ring first.
const FRONT: [u8; 2] = [50, 34];
/// Cassette teeth, smallest cog first.
const REAR: [u8; 12] = [11, 12, 13, 14, 15, 17, 19, 21, 24, 27, 30, 34];
/// The ratio at which the gearbox adds nothing to the grade (the geometric middle of the range).
const REFERENCE_RATIO: f64 = 2.13;
/// Percent of grade per unit of ln(ratio / reference).
const GRADE_PER_LN: f64 = 7.0;
/// Wheel circumference of a 700 x 25c tyre, metres.
const WHEEL_M: f64 = 2.096;
/// Gears closer than this (relative) are the same gear for the rider.
const MERGE_BELOW: f64 = 0.015;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gear {
    pub front: u8,
    pub rear: u8,
}

impl Gear {
    pub fn ratio(self) -> f64 {
        f64::from(self.front) / f64::from(self.rear)
    }

    /// "50 × 17"
    pub fn label(self) -> String {
        format!("{} × {}", self.front, self.rear)
    }
}

/// All usable gears, easiest first, without near duplicates (the 50/34 chainrings overlap).
pub fn gears() -> &'static [Gear] {
    static GEARS: OnceLock<Vec<Gear>> = OnceLock::new();
    GEARS.get_or_init(|| {
        let mut all: Vec<Gear> = FRONT
            .iter()
            .flat_map(|&front| REAR.iter().map(move |&rear| Gear { front, rear }))
            .collect();
        all.sort_by(|a, b| a.ratio().total_cmp(&b.ratio()));
        let mut kept: Vec<Gear> = Vec::with_capacity(all.len());
        for gear in all {
            match kept.last() {
                Some(last) if gear.ratio() / last.ratio() - 1.0 < MERGE_BELOW => {
                    // of two near duplicates the one on the bigger ring pair wins: straighter chain line
                    if gear.front > last.front {
                        *kept.last_mut().expect("non-empty") = gear;
                    }
                }
                _ => kept.push(gear),
            }
        }
        kept
    })
}

/// The gear at which the gearbox changes nothing.
pub fn default_index() -> usize {
    gears()
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (a.ratio() - REFERENCE_RATIO)
                .abs()
                .total_cmp(&(b.ratio() - REFERENCE_RATIO).abs())
        })
        .map_or(0, |(i, _)| i)
}

/// The saved gear number as an index: anything out of range means "not chosen yet".
pub fn index_from_saved(saved: u8) -> usize {
    let i = usize::from(saved);
    if i < gears().len() {
        i
    } else {
        default_index()
    }
}

/// One step up (`+1`, harder) or down (`-1`, easier), stopping at the ends.
pub fn shifted(index: usize, delta: i32) -> usize {
    let last = gears().len() - 1;
    (index as i64 + i64::from(delta)).clamp(0, last as i64) as usize
}

/// Percent of grade the gear adds to what the trainer is asked for.
pub fn grade_offset(index: usize) -> f64 {
    let gear = gears()[index.min(gears().len() - 1)];
    GRADE_PER_LN * (gear.ratio() / REFERENCE_RATIO).ln()
}

/// Speed in km/h that this gear gives at this cadence.
pub fn speed_kmh(index: usize, cadence_rpm: f64) -> f64 {
    let gear = gears()[index.min(gears().len() - 1)];
    cadence_rpm.max(0.0) * gear.ratio() * WHEEL_M * 60.0 / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gearbox_is_sorted_and_has_no_duplicates() {
        let g = gears();
        assert!(g.len() >= 16 && g.len() <= 24, "{} gears", g.len());
        for pair in g.windows(2) {
            assert!(
                pair[1].ratio() / pair[0].ratio() - 1.0 >= MERGE_BELOW - 1e-9,
                "{pair:?}"
            );
        }
        assert_eq!(
            g[0],
            Gear {
                front: 34,
                rear: 34
            }
        );
        let top = g[g.len() - 1];
        assert_eq!(
            top,
            Gear {
                front: 50,
                rear: 11
            }
        );
        assert_eq!(top.label(), "50 × 11");
    }

    #[test]
    fn the_default_gear_adds_nothing_and_the_ends_are_symmetric_ish() {
        let d = default_index();
        assert!(grade_offset(d).abs() < 0.5, "{}", grade_offset(d));
        let easiest = grade_offset(0);
        let hardest = grade_offset(gears().len() - 1);
        assert!(easiest < -4.0 && easiest > -7.0, "{easiest}");
        assert!(hardest > 4.0 && hardest < 7.0, "{hardest}");
        // monotonic: every shift up adds resistance
        for i in 1..gears().len() {
            assert!(grade_offset(i) > grade_offset(i - 1));
        }
    }

    #[test]
    fn shifting_stops_at_the_ends() {
        let last = gears().len() - 1;
        assert_eq!(shifted(0, -1), 0);
        assert_eq!(shifted(last, 1), last);
        assert_eq!(shifted(5, 1), 6);
        assert_eq!(shifted(5, -1), 4);
        assert_eq!(shifted(5, 100), last);
    }

    #[test]
    fn a_saved_gear_out_of_range_falls_back_to_the_default() {
        assert_eq!(index_from_saved(255), default_index());
        assert_eq!(index_from_saved(3), 3);
        assert_eq!(index_from_saved(gears().len() as u8), default_index());
    }

    #[test]
    fn speed_follows_cadence_and_ratio() {
        let i = gears()
            .iter()
            .position(|g| {
                *g == Gear {
                    front: 50,
                    rear: 17,
                }
            })
            .expect("50x17");
        // 90 rpm in 50 x 17: 90 * 2.94 * 2.096 m/min = 33.3 km/h
        assert!(
            (speed_kmh(i, 90.0) - 33.3).abs() < 0.2,
            "{}",
            speed_kmh(i, 90.0)
        );
        assert_eq!(speed_kmh(i, 0.0), 0.0);
        assert_eq!(speed_kmh(i, -5.0), 0.0);
        assert!(speed_kmh(i + 1, 90.0) > speed_kmh(i, 90.0));
    }
}
