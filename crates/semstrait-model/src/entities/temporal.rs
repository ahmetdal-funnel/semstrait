//! `TemporalShape` and friends — `18 §3`.
//!
//! Per `32 §4.1`, the variant tag (shape kind) cascades from a complex
//! ancestor to leaf descendants. The grain field is leaf-only (SR-E-7)
//! and grainset children must author their own grain (SR-E-8) — those
//! invariants are enforced at validate, not by these types.

use crate::types::SemanticsName;
use semstrait_core::Grain;
use serde::{Deserialize, Serialize};

/// Variant tag + body, plus optional leaf-effective grain. The variant
/// is flattened at YAML so authors write `temporal: { events: { ... },
/// grain: minute }` per `18 §3.2`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct TemporalShape {
    #[serde(flatten)]
    pub kind: TemporalShapeKind,

    /// Effective at a `Dataset` leaf. Required on leaves with `temporal:`
    /// authored (SR-E-6); forbidden on `ComplexDataKind` (SR-E-7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grain: Option<Grain>,
}

impl TemporalShape {
    pub fn new(kind: TemporalShapeKind) -> Self {
        Self { kind, grain: None }
    }

    pub fn with_grain(mut self, grain: Grain) -> Self {
        self.grain = Some(grain);
        self
    }

    /// Convenience constructor for the `Timeseries` variant.
    pub fn timeseries(occurred_at: impl Into<SemanticsName>, grain: Option<Grain>) -> Self {
        Self {
            kind: TemporalShapeKind::Timeseries(TimeseriesBody {
                occurred_at: occurred_at.into(),
            }),
            grain,
        }
    }

    /// Convenience constructor for the `Events` variant.
    pub fn events(event_time: impl Into<SemanticsName>, grain: Option<Grain>) -> Self {
        Self {
            kind: TemporalShapeKind::Events(EventsBody {
                event_time: event_time.into(),
            }),
            grain,
        }
    }

    /// Convenience constructor for the `Snapshot` variant.
    pub fn snapshot(snapshotted_at: impl Into<SemanticsName>, grain: Option<Grain>) -> Self {
        Self {
            kind: TemporalShapeKind::Snapshot(SnapshotBody {
                snapshotted_at: snapshotted_at.into(),
            }),
            grain,
        }
    }

    /// Convenience constructor for the `Scd` variant.
    pub fn scd(
        scd_type: ScdType,
        valid_from: impl Into<SemanticsName>,
        valid_to: impl Into<SemanticsName>,
        grain: Option<Grain>,
    ) -> Self {
        Self {
            kind: TemporalShapeKind::Scd(ScdBody {
                scd_type,
                valid_from: valid_from.into(),
                valid_to: valid_to.into(),
            }),
            grain,
        }
    }
}

/// Tagged temporal-shape variant. The YAML tag is flattened onto the
/// containing [`TemporalShape`] alongside `grain:` per `18 §3.2`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TemporalShapeKind {
    Timeseries(TimeseriesBody),
    Events(EventsBody),
    Snapshot(SnapshotBody),
    Scd(ScdBody),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct TimeseriesBody {
    pub occurred_at: SemanticsName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct EventsBody {
    pub event_time: SemanticsName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct SnapshotBody {
    pub snapshotted_at: SemanticsName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct ScdBody {
    pub scd_type: ScdType,
    pub valid_from: SemanticsName,
    pub valid_to: SemanticsName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ScdType {
    Type1,
    Type2,
}
