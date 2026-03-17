//! Temporal grain levels for dimensions and date truncation.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Temporal grain levels used in dimension types and DateTrunc expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grain {
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

impl fmt::Display for Grain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Grain::Minute => write!(f, "minute"),
            Grain::Hour => write!(f, "hour"),
            Grain::Day => write!(f, "day"),
            Grain::Week => write!(f, "week"),
            Grain::Month => write!(f, "month"),
            Grain::Quarter => write!(f, "quarter"),
            Grain::Year => write!(f, "year"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display() {
        assert_eq!(Grain::Minute.to_string(), "minute");
        assert_eq!(Grain::Hour.to_string(), "hour");
        assert_eq!(Grain::Day.to_string(), "day");
        assert_eq!(Grain::Week.to_string(), "week");
        assert_eq!(Grain::Month.to_string(), "month");
        assert_eq!(Grain::Quarter.to_string(), "quarter");
        assert_eq!(Grain::Year.to_string(), "year");
    }

    #[test]
    fn test_serde_roundtrip() {
        let grains = vec![
            Grain::Minute,
            Grain::Hour,
            Grain::Day,
            Grain::Week,
            Grain::Month,
            Grain::Quarter,
            Grain::Year,
        ];

        for grain in grains {
            let json = serde_json::to_string(&grain).unwrap();
            let parsed: Grain = serde_json::from_str(&json).unwrap();
            assert_eq!(grain, parsed);
        }
    }
}
