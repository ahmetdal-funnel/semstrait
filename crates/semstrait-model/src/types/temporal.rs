//! Temporal configuration types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Temporal Grain
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalGrain {
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

impl TemporalGrain {
    /// All variants in finest-to-coarsest order. Single source of truth —
    /// any new variant must be added here and in `coarseness()`.
    pub const ALL: [TemporalGrain; 7] = [
        Self::Minute, Self::Hour, Self::Day, Self::Week,
        Self::Month, Self::Quarter, Self::Year,
    ];

    /// Returns the grain's relative coarseness (higher = coarser).
    pub fn coarseness(self) -> u8 {
        match self {
            Self::Minute => 0,
            Self::Hour => 1,
            Self::Day => 2,
            Self::Week => 3,
            Self::Month => 4,
            Self::Quarter => 5,
            Self::Year => 6,
        }
    }
}

impl From<TemporalGrain> for semstrait_core::Grain {
    fn from(tg: TemporalGrain) -> Self {
        match tg {
            TemporalGrain::Minute => Self::Minute,
            TemporalGrain::Hour => Self::Hour,
            TemporalGrain::Day => Self::Day,
            TemporalGrain::Week => Self::Week,
            TemporalGrain::Month => Self::Month,
            TemporalGrain::Quarter => Self::Quarter,
            TemporalGrain::Year => Self::Year,
        }
    }
}

// =============================================================================
// Temporal Config
// =============================================================================

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemporalConfig {
    /// Data-level temporal cadence (e.g., day, hour). When set, enables grain
    /// auto-propagation to column_mapping entries that share the same physical column.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grain: Option<TemporalGrain>,
    /// Links this temporal config to a semantic dimension name. Used to set
    /// `KindInterface.temporal_dim` without scanning the interface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<String>,
    #[serde(rename = "type")]
    pub temporal_type: TemporalHistorization,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalHistorization {
    Timeseries(TimeseriesConfig),
    Events(EventsConfig),
    Snapshot(SnapshotConfig),
    Scd(ScdConfig),
}

impl<'de> Deserialize<'de> for TemporalHistorization {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map: HashMap<String, serde_yaml::Value> = Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "TemporalHistorization must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next()
            .ok_or_else(|| serde::de::Error::custom("expected one variant key"))?;
        match key.as_str() {
            "timeseries" => Ok(TemporalHistorization::Timeseries(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "snapshot" => Ok(TemporalHistorization::Snapshot(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "events" => Ok(TemporalHistorization::Events(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "scd" => Ok(TemporalHistorization::Scd(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["timeseries", "events", "snapshot", "scd"],
            )),
        }
    }
}

impl TemporalHistorization {
    /// Returns the variant name as a string for error messages.
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::Timeseries(_) => "timeseries",
            Self::Events(_) => "events",
            Self::Snapshot(_) => "snapshot",
            Self::Scd(_) => "scd",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimeseriesConfig {
    pub occurred_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventsConfig {
    pub occurred_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SnapshotConfig {
    pub snapshotted_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScdConfig {
    #[serde(flatten)]
    pub scd_type: ScdType,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScdType {
    Type1,
    Type2(ScdVersionedColumns),
    Type3,
    Type4,
    Type5(ScdVersionedColumns),
    Type6(ScdVersionedColumns),
}

impl<'de> Deserialize<'de> for ScdType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map: HashMap<String, serde_yaml::Value> = Deserialize::deserialize(deserializer)?;
        if map.len() != 1 {
            return Err(serde::de::Error::custom(
                "ScdType must have exactly one variant key",
            ));
        }
        let (key, value) = map.into_iter().next()
            .ok_or_else(|| serde::de::Error::custom("expected one variant key"))?;
        match key.as_str() {
            "type_1" => Ok(ScdType::Type1),
            "type_2" => Ok(ScdType::Type2(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "type_3" => Ok(ScdType::Type3),
            "type_4" => Ok(ScdType::Type4),
            "type_5" => Ok(ScdType::Type5(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            "type_6" => Ok(ScdType::Type6(
                serde_yaml::from_value(value).map_err(serde::de::Error::custom)?,
            )),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["type_1", "type_2", "type_3", "type_4", "type_5", "type_6"],
            )),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScdVersionedColumns {
    pub valid_from: String,
    pub valid_to: String,
}
