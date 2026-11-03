//! The workout chart: one coloured block per step, scaled to FTP.
//!
//! A workout is a sequence of power targets, so the honest picture is a bar chart over time:
//! the width of a block is its duration, its height is the target as a percentage of FTP, and
//! its colour is the power zone of that target. A ramp is the trapezoid between its two ends.
//! The same shape a workout builder shows, and the same zones the ride screen uses. The picker
//! draws the whole workout; the ride screen draws a time window of it around the rider.

use ride_model::{power_zone_of_pct, StepTarget, Workout};

/// Room above the hardest block: the axis tops out at this multiple of it (rounded up to ten),
/// so even a 120 % interval reaches only about two thirds of the height and the chart does not
/// look like a wall.
const HEADROOM: f64 = 1.25;

/// Intensity a free (unprescribed) step is drawn at, percent of FTP.
const FREE_PCT: f64 = 50.0;

/// FTP assumed when the rider's is unusable (only watt targets depend on it).
const FALLBACK_FTP_W: f64 = 200.0;

/// A block narrower than this (percent of the drawn window) gets no separating gap: on a dense
/// 30/15 workout the gaps would eat the blocks.
const GAP_MIN_PCT: f64 = 1.2;

/// One block of the chart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    /// Index of the step in the workout.
    pub step: usize,
    /// Start, seconds from the beginning of the workout.
    pub start_s: f64,
    /// Length, seconds.
    pub duration_s: f64,
    /// Left edge, percent of the workout's length.
    pub left: f64,
    /// Width, percent of the workout's length.
    pub width: f64,
    /// Target at the left edge, percent of FTP.
    pub from: f64,
    /// Target at the right edge, percent of FTP (equal to `from` for a steady block).
    pub to: f64,
    /// Power zone 1..=7 of the block's intensity; `0` for a free step.
    pub zone: u8,
}

impl Block {
    pub fn end_s(&self) -> f64 {
        self.start_s + self.duration_s
    }

    /// Target (percent of FTP) at `t` seconds from the workout start, clamped to the block.
    pub fn pct_at(&self, t: f64) -> f64 {
        if self.duration_s <= 0.0 {
            return self.from;
        }
        let f = ((t - self.start_s) / self.duration_s).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * f
    }
}

/// The blocks plus the vertical scale.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chart {
    pub blocks: Vec<Block>,
    /// Top of the y axis, percent of FTP: [`HEADROOM`] above the hardest block (at least 100 %),
    /// rounded up to ten.
    pub ceiling: f64,
    /// Length of the workout, seconds.
    pub total_s: f64,
}

impl Chart {
    /// The block running at `t` seconds (the last one after the end).
    pub fn block_at(&self, t: f64) -> Option<&Block> {
        self.blocks
            .iter()
            .find(|b| t < b.end_s())
            .or(self.blocks.last())
    }
}

/// Geometry of `workout` for a rider with `ftp_w` (watt targets are drawn relative to it); an
/// empty workout yields an empty chart with a 100 % ceiling.
pub fn chart(workout: &Workout, ftp_w: f64) -> Chart {
    let ftp = if ftp_w.is_finite() && ftp_w > 0.0 {
        ftp_w
    } else {
        FALLBACK_FTP_W
    };
    let total: f64 = workout
        .steps
        .iter()
        .map(|step| step.duration_s as f64)
        .sum();
    if total <= 0.0 {
        return Chart {
            blocks: Vec::new(),
            ceiling: 100.0,
            total_s: 0.0,
        };
    }
    let mut blocks = Vec::with_capacity(workout.steps.len());
    let mut left = 0.0;
    let mut top: f64 = 100.0;
    for (index, step) in workout.steps.iter().enumerate() {
        let duration = step.duration_s as f64;
        let (from, to) = match step.target {
            StepTarget::Free => (FREE_PCT, FREE_PCT),
            target => (
                target.pct_at(0.0, duration, ftp).unwrap_or(FREE_PCT),
                target.pct_at(duration, duration, ftp).unwrap_or(FREE_PCT),
            ),
        };
        top = top.max(from).max(to);
        let zone = if step.target.is_free() {
            0
        } else {
            power_zone_of_pct((from + to) * 0.5)
        };
        blocks.push(Block {
            step: index,
            start_s: left,
            duration_s: duration,
            left: left / total * 100.0,
            width: duration / total * 100.0,
            from,
            to,
            zone,
        });
        left += duration;
    }
    Chart {
        blocks,
        ceiling: ((top * HEADROOM + 9.9) / 10.0).floor() * 10.0,
        total_s: total,
    }
}

fn usable_ceiling(ceiling: f64) -> f64 {
    if ceiling.is_finite() && ceiling > 0.0 {
        ceiling
    } else {
        100.0
    }
}

/// Height of `pct` in the chart's `0 0 1000 1000` viewBox (y grows downwards).
pub fn y_of(pct: f64, ceiling: f64) -> f64 {
    (1.0 - (pct / usable_ceiling(ceiling)).clamp(0.0, 1.0)) * 1000.0
}

/// SVG polygon points of a block over the whole workout, in the `0 0 1000 1000` viewBox.
pub fn polygon(block: &Block, ceiling: f64) -> String {
    let x0 = block.left.clamp(0.0, 100.0) * 10.0;
    let x1 = (block.left + block.width).clamp(0.0, 100.0) * 10.0;
    let (y0, y1) = (y_of(block.from, ceiling), y_of(block.to, ceiling));
    format!("{x0:.1},1000 {x0:.1},{y0:.1} {x1:.1},{y1:.1} {x1:.1},1000")
}

/// SVG polygon points of the part of `block` inside the time window `t0..t1` (seconds), in the
/// `0 0 1000 1000` viewBox of that window; `None` when the block is outside it.
pub fn polygon_in(block: &Block, ceiling: f64, t0: f64, t1: f64) -> Option<String> {
    let span = t1 - t0;
    if !(span.is_finite() && span > 0.0) {
        return None;
    }
    let a = block.start_s.max(t0);
    let b = block.end_s().min(t1);
    if b <= a {
        return None;
    }
    let x = |t: f64| ((t - t0) / span).clamp(0.0, 1.0) * 1000.0;
    let (x0, x1) = (x(a), x(b));
    let (y0, y1) = (
        y_of(block.pct_at(a), ceiling),
        y_of(block.pct_at(b), ceiling),
    );
    Some(format!(
        "{x0:.1},1000 {x0:.1},{y0:.1} {x1:.1},{y1:.1} {x1:.1},1000"
    ))
}

/// Whether a block that covers `width_pct` of the drawn window is wide enough to be separated
/// from its neighbours by a hairline gap.
pub fn gets_gap(width_pct: f64) -> bool {
    width_pct >= GAP_MIN_PCT
}

#[cfg(test)]
mod tests {
    use super::*;
    use ride_model::{StepKind, WorkoutCategory, WorkoutStep};

    fn workout(steps: Vec<WorkoutStep>) -> Workout {
        Workout {
            id: "test".into(),
            name: "Teszt".into(),
            category: WorkoutCategory::Endurance,
            goal: String::new(),
            description: String::new(),
            ftp_pct: 70,
            duration_min: 10,
            tags: vec![],
            sources: vec![],
            steps,
        }
    }

    fn step(duration_s: u32, target: StepTarget) -> WorkoutStep {
        WorkoutStep {
            kind: StepKind::Steady,
            name: "blokk".into(),
            duration_s,
            target,
            cadence: None,
            note: None,
            messages: Vec::new(),
        }
    }

    #[test]
    fn blocks_share_the_width_by_time_and_the_height_by_intensity() {
        let c = chart(
            &workout(vec![
                step(300, StepTarget::PctFtp { value: 50.0 }),
                step(600, StepTarget::PctFtp { value: 100.0 }),
                step(300, StepTarget::PctFtp { value: 75.0 }),
            ]),
            250.0,
        );
        assert_eq!(c.blocks.len(), 3);
        assert_eq!(c.ceiling, 130.0, "headroom over the 100 % block");
        assert_eq!(c.total_s, 1200.0);
        assert!((c.blocks[0].width - 25.0).abs() < 1e-9);
        assert!((c.blocks[1].width - 50.0).abs() < 1e-9);
        assert!((c.blocks[1].left - 25.0).abs() < 1e-9);
        assert!((c.blocks[2].left - 75.0).abs() < 1e-9);
        assert_eq!(c.blocks[2].start_s, 900.0);
        // zones: 50 % is Z1, 100 % is Z4, 75 % is Z2
        assert_eq!(c.blocks[0].zone, 1);
        assert_eq!(c.blocks[1].zone, 4);
        assert_eq!(c.blocks[2].zone, 2);
        // the polygon is a rectangle from the baseline up to the target
        assert_eq!(
            polygon(&c.blocks[1], c.ceiling),
            "250.0,1000 250.0,230.8 750.0,230.8 750.0,1000"
        );
        assert_eq!(c.block_at(301.0).unwrap().step, 1);
        assert_eq!(
            c.block_at(5000.0).unwrap().step,
            2,
            "after the end: the last block"
        );
    }

    #[test]
    fn watt_targets_are_drawn_relative_to_the_riders_ftp() {
        let c = chart(
            &workout(vec![step(60, StepTarget::Watts { value: 250.0 })]),
            250.0,
        );
        assert!((c.blocks[0].from - 100.0).abs() < 1e-9);
        let c = chart(
            &workout(vec![step(60, StepTarget::Watts { value: 250.0 })]),
            f64::NAN,
        );
        assert!(c.blocks[0].from.is_finite());
    }

    #[test]
    fn a_ramp_is_a_trapezoid_and_raises_the_ceiling() {
        let c = chart(
            &workout(vec![step(
                600,
                StepTarget::RampPct {
                    from: 60.0,
                    to: 130.0,
                },
            )]),
            200.0,
        );
        assert_eq!(c.ceiling, 170.0);
        let points = polygon(&c.blocks[0], c.ceiling);
        let points: Vec<&str> = points.split(' ').collect();
        assert_eq!(points.len(), 4);
        // the left edge starts at 60 % of 170 %, the right one ends at 130 %, below the top
        assert!(points[1].ends_with(",647.1"), "{}", points[1]);
        assert!(points[2].ends_with(",235.3"), "{}", points[2]);
    }

    #[test]
    fn a_window_clips_blocks_and_interpolates_ramps() {
        let c = chart(
            &workout(vec![
                step(600, StepTarget::PctFtp { value: 50.0 }),
                step(
                    600,
                    StepTarget::RampPct {
                        from: 50.0,
                        to: 100.0,
                    },
                ),
            ]),
            200.0,
        );
        // window 900..1200 s: only the second half of the ramp
        assert_eq!(polygon_in(&c.blocks[0], c.ceiling, 900.0, 1200.0), None);
        let p = polygon_in(&c.blocks[1], c.ceiling, 900.0, 1200.0).unwrap();
        assert_eq!(p, "0.0,1000 0.0,423.1 1000.0,230.8 1000.0,1000");
        assert_eq!(polygon_in(&c.blocks[1], c.ceiling, 5.0, 5.0), None);
        assert!(gets_gap(5.0) && !gets_gap(0.8));
    }

    #[test]
    fn a_free_step_is_neutral_and_an_empty_workout_has_no_blocks() {
        let c = chart(&workout(vec![step(300, StepTarget::Free)]), 200.0);
        assert_eq!(c.blocks[0].zone, 0, "free steps have no zone");
        assert_eq!(c.ceiling, 130.0);
        let empty = chart(&workout(vec![]), 200.0);
        assert!(empty.blocks.is_empty());
        assert_eq!(empty.ceiling, 100.0);
        assert!(empty.block_at(0.0).is_none());
    }

    #[test]
    fn the_chart_never_produces_nan_or_out_of_range_points() {
        let c = chart(
            &workout(vec![
                step(0, StepTarget::PctFtp { value: 250.0 }),
                step(60, StepTarget::Free),
            ]),
            200.0,
        );
        for block in &c.blocks {
            let points = polygon(block, c.ceiling);
            assert!(!points.contains("NaN"), "{points}");
            for point in points.split(' ') {
                for value in point.split(',') {
                    let value: f64 = value.parse().unwrap();
                    assert!((0.0..=1000.0).contains(&value), "{value} in {points}");
                }
            }
        }
    }
}
