//! Temporal grain levels for dimensions and date truncation.

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// Temporal grain levels used in dimension types and DateTrunc expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(Display, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase", ascii_case_insensitive)]
pub enum Grain {
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Quarter,
    Year,
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
    fn test_from_str() {
        assert_eq!("day".parse::<Grain>().unwrap(), Grain::Day);
        assert_eq!("DAY".parse::<Grain>().unwrap(), Grain::Day);
        assert_eq!("Month".parse::<Grain>().unwrap(), Grain::Month);
        assert!("unknown".parse::<Grain>().is_err());
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
