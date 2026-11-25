//! Explicit calendar days for Task metadata, without timezone or inferred dates.
//!
//! TaskSchedule accepts a nonempty partial schedule. Only two endpoints define
//! an inclusive duration; a due date remains independent of the interval.

use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{error::Error, fmt, str::FromStr};

/// A validated, canonical YYYY-MM-DD calendar day (years 0000 through 9999).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CalendarDate(NaiveDate);

#[derive(Debug)]
pub enum CalendarDateError {
    NoncanonicalFormat,
    InvalidCalendarDay(chrono::ParseError),
}
impl fmt::Display for CalendarDateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoncanonicalFormat => f.write_str("calendar day must use YYYY-MM-DD"),
            Self::InvalidCalendarDay(error) => write!(f, "invalid calendar day: {error}"),
        }
    }
}
impl Error for CalendarDateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidCalendarDay(error) => Some(error),
            Self::NoncanonicalFormat => None,
        }
    }
}
impl FromStr for CalendarDate {
    type Err = CalendarDateError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let bytes = value.as_bytes();
        if bytes.len() != 10
            || !bytes.iter().enumerate().all(|(index, byte)| {
                if index == 4 || index == 7 {
                    *byte == b'-'
                } else {
                    byte.is_ascii_digit()
                }
            })
        {
            return Err(CalendarDateError::NoncanonicalFormat);
        }
        value
            .parse::<NaiveDate>()
            .map(Self)
            .map_err(CalendarDateError::InvalidCalendarDay)
    }
}
impl fmt::Display for CalendarDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Serialize for CalendarDate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for CalendarDate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskScheduleError {
    Empty,
    InvertedInterval,
}
impl fmt::Display for TaskScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "schedule must contain at least one calendar day",
            Self::InvertedInterval => "schedule end_date precedes start_date",
        })
    }
}
impl Error for TaskScheduleError {}

/// Validated optional start/end/due days. None on TaskSpecification is unscheduled.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ScheduleFields")]
pub struct TaskSchedule {
    #[serde(skip_serializing_if = "Option::is_none")]
    start_date: Option<CalendarDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    end_date: Option<CalendarDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    due_date: Option<CalendarDate>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScheduleFields {
    start_date: Option<CalendarDate>,
    end_date: Option<CalendarDate>,
    due_date: Option<CalendarDate>,
}
impl TryFrom<ScheduleFields> for TaskSchedule {
    type Error = TaskScheduleError;
    fn try_from(fields: ScheduleFields) -> Result<Self, Self::Error> {
        Self::new(fields.start_date, fields.end_date, fields.due_date)
    }
}
impl TaskSchedule {
    pub fn new(
        start_date: Option<CalendarDate>,
        end_date: Option<CalendarDate>,
        due_date: Option<CalendarDate>,
    ) -> Result<Self, TaskScheduleError> {
        if start_date.is_none() && end_date.is_none() && due_date.is_none() {
            return Err(TaskScheduleError::Empty);
        }
        if let (Some(start), Some(end)) = (start_date, end_date)
            && end < start
        {
            return Err(TaskScheduleError::InvertedInterval);
        }
        Ok(Self {
            start_date,
            end_date,
            due_date,
        })
    }
    pub const fn start_date(self) -> Option<CalendarDate> {
        self.start_date
    }
    pub const fn end_date(self) -> Option<CalendarDate> {
        self.end_date
    }
    pub const fn due_date(self) -> Option<CalendarDate> {
        self.due_date
    }
    /// Civil days, including both endpoints; absent for a partial schedule.
    pub fn inclusive_duration_days(self) -> Option<i64> {
        Some(
            self.end_date?
                .0
                .signed_duration_since(self.start_date?.0)
                .num_days()
                + 1,
        )
    }
}

#[cfg(test)]
mod tests;
