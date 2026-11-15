//! The training journal: every stored ride of every route in time order, grouped into weeks
//! (Monday to Sunday, UTC) with the week's totals and its load day by day.
//!
//! Built from the index entries only (no record is opened). Simulated rides are listed, marked,
//! and add nothing to the totals.

use ride_model::{RideMeta, RiderSettings, RouteRides};

use crate::fitness::{self, LoadSource};
use crate::power_curve::{day_of, period_start, valid_timestamp, Period};

/// One ride of the journal.
#[derive(Debug, Clone, PartialEq)]
pub struct JournalRide {
    pub route_id: String,
    pub route_name: String,
    pub meta: RideMeta,
    pub load: Option<(f64, LoadSource)>,
}

/// One week of the journal.
#[derive(Debug, Clone, PartialEq)]
pub struct JournalWeek {
    /// Monday 00:00 UTC.
    pub start_unix_s: f64,
    /// Newest first.
    pub rides: Vec<JournalRide>,
    pub time_s: f64,
    pub dist_m: f64,
    pub load: f64,
    /// The load of each day, Monday first.
    pub daily_load: [f64; 7],
}

/// The weeks that have rides, newest first.
pub fn journal(all: &[RouteRides], rider: &RiderSettings) -> Vec<JournalWeek> {
    let mut rides: Vec<JournalRide> = all
        .iter()
        .flat_map(|route| {
            route.rides.iter().map(move |meta| JournalRide {
                route_id: route.route_id.clone(),
                route_name: if meta.route_name.is_empty() {
                    route.route_name.clone()
                } else {
                    meta.route_name.clone()
                },
                meta: meta.clone(),
                load: fitness::ride_load(meta, rider),
            })
        })
        .filter(|r| valid_timestamp(r.meta.started_unix_s))
        .collect();
    rides.sort_by(|a, b| b.meta.started_unix_s.total_cmp(&a.meta.started_unix_s));

    let mut weeks: Vec<JournalWeek> = Vec::new();
    for ride in rides {
        let day = day_of(ride.meta.started_unix_s);
        let monday = period_start(day, Period::Week);
        let start = monday as f64 * 86_400.0;
        if weeks.last().is_none_or(|w| w.start_unix_s != start) {
            weeks.push(JournalWeek {
                start_unix_s: start,
                rides: Vec::new(),
                time_s: 0.0,
                dist_m: 0.0,
                load: 0.0,
                daily_load: [0.0; 7],
            });
        }
        let Some(week) = weeks.last_mut() else {
            continue;
        };
        if !ride.meta.simulated {
            let finite = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
            week.time_s += finite(ride.meta.elapsed_s);
            week.dist_m += finite(ride.meta.dist_m);
            if let Some((load, _)) = ride.load {
                week.load += load;
                if let Some(cell) = week.daily_load.get_mut((day - monday) as usize) {
                    *cell += load;
                }
            }
        }
        week.rides.push(ride);
    }
    weeks
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: f64 = 1_700_000_000.0; // Tuesday 2023-11-14
    const DAY: f64 = 86_400.0;

    fn meta(stem: &str, started: f64, tss: f64) -> RideMeta {
        RideMeta {
            stem: stem.into(),
            started_unix_s: started,
            elapsed_s: 3_600.0,
            dist_m: 30_000.0,
            tss: Some(tss),
            ..RideMeta::default()
        }
    }

    #[test]
    fn rides_of_every_route_are_grouped_into_weeks() {
        let mut simulated = meta("4", NOW - 0.1 * DAY, 500.0);
        simulated.simulated = true;
        let all = vec![
            RouteRides {
                route_id: "a".into(),
                route_name: "Alfa".into(),
                rides: vec![meta("1", NOW, 50.0), meta("2", NOW - 9.0 * DAY, 70.0)],
            },
            RouteRides {
                route_id: "b".into(),
                route_name: "Béta".into(),
                rides: vec![meta("3", NOW - DAY, 40.0), simulated],
            },
        ];
        let weeks = journal(&all, &RiderSettings::default());
        assert_eq!(weeks.len(), 2);
        let this = &weeks[0];
        assert_eq!(
            this.rides
                .iter()
                .map(|r| r.meta.stem.as_str())
                .collect::<Vec<_>>(),
            ["1", "4", "3"],
            "newest first, across routes"
        );
        assert_eq!(this.rides[2].route_name, "Béta");
        assert_eq!(this.load, 90.0, "the simulated ride adds nothing");
        assert_eq!(this.time_s, 7_200.0);
        // Monday 40, Tuesday 50
        assert_eq!(this.daily_load[0], 40.0);
        assert_eq!(this.daily_load[1], 50.0);
        assert_eq!(weeks[1].rides.len(), 1);
        assert!(weeks[0].start_unix_s > weeks[1].start_unix_s);
        assert!(journal(&[], &RiderSettings::default()).is_empty());
    }
}
