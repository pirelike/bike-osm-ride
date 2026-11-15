//! Scheduled workouts, stored as local calendar dates and optional local times.

use serde::{Deserialize, Serialize};

use crate::Workout;

/// A scheduled workout keeps its own snapshot so later library edits cannot change it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarEntry {
    pub id: String,
    /// ISO calendar date, independent of timezone.
    pub date: String,
    /// Optional local wall-clock time, HH:MM.
    #[serde(default)]
    pub time: Option<String>,
    pub workout: Workout,
}

impl CalendarEntry {
    pub fn is_valid(&self) -> bool {
        let date = self.date.as_bytes();
        if date.len() != 10
            || !self.date.is_ascii()
            || date[4] != b'-'
            || date[7] != b'-'
            || date
                .iter()
                .enumerate()
                .any(|(i, b)| i != 4 && i != 7 && !b.is_ascii_digit())
        {
            return false;
        }
        let (Ok(year), Ok(month), Ok(day)) = (
            self.date[0..4].parse::<u32>(),
            self.date[5..7].parse::<u32>(),
            self.date[8..10].parse::<u32>(),
        ) else {
            return false;
        };
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => 0,
        };
        let valid_time = self.time.as_ref().is_none_or(|time| {
            let bytes = time.as_bytes();
            bytes.len() == 5
                && bytes[2] == b':'
                && bytes
                    .iter()
                    .enumerate()
                    .all(|(i, b)| i == 2 || b.is_ascii_digit())
                && time
                    .get(..2)
                    .and_then(|s| s.parse::<u8>().ok())
                    .is_some_and(|h| h < 24)
                && time
                    .get(3..)
                    .and_then(|s| s.parse::<u8>().ok())
                    .is_some_and(|m| m < 60)
        });
        year > 0
            && day > 0
            && day <= days
            && valid_time
            && !self.id.is_empty()
            && !self.workout.id.is_empty()
            && !self.workout.steps.is_empty()
    }
}
