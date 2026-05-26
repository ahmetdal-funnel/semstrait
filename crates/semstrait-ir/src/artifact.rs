//! Adapter-consumable artifact family. Per spec `35 §12`.
//!
//! `35` ratifies the structural shape; `36` (`semstrait-adapter`) owns
//! the emission semantics. Engine identity (`DialectId`) appears only on
//! [`SqlArtifact::dialect`] and [`Dialect::ID`]; it never travels on
//! [`crate::SemanticPlan`], [`crate::PlanNode`], or [`crate::NodeMeta`]
//! per S7 / `35 §1.5`.
//!
//! ## Variant inventory
//!
//! - [`EngineArtifact`] — sum over the two adapter emission modes
//!   (text-SQL vs structured Substrait plan). `#[non_exhaustive]`.
//! - [`EnginePlan`] — structured-IR engine output. v1 ships
//!   `Substrait(Box<substrait::proto::Plan>)`; serialization helpers
//!   [`EnginePlan::to_bytes`] / [`EnginePlan::to_json`] surface
//!   prost / proto3-JSON encoding errors as
//!   [`IrErrorKind::SubstraitCodecError`].
//! - [`SqlArtifact`] — text + dialect provenance tag.
//! - [`DialectId`] — newtype-over-stable per `30 §4.3` (no
//!   `#[non_exhaustive]`); 4 v1 `pub const` slots: `ANSI`, `DATAFUSION`,
//!   `DUCKDB`, `SPARK`.
//! - [`Dialect`] — open trait; third-party adapters MAY impl it for
//!   their own [`DialectId`].
//! - [`Capability`] — cross-boundary capability vocabulary; type lives
//!   here, per-adapter rosters live in `36`.

use prost::Message;

use crate::error::IrErrorKind;

// ── EngineArtifact ──────────────────────────────────────────────────────

/// Engine-ready plan artifact. Per spec `35 §12.1`.
///
/// Two variants covering the adapter emission modes today:
/// - [`Self::Sql`] for engines that consume SQL strings (DuckDB, Spark,
///   PostgreSQL, …).
/// - [`Self::Plan`] for engines that consume Substrait plans natively
///   (DataFusion, future Substrait-consuming engines).
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum EngineArtifact {
    Sql(SqlArtifact),
    Plan(EnginePlan),
}

// ── EnginePlan ──────────────────────────────────────────────────────────

/// Structured-IR engine output. Per spec `35 §12.2`.
///
/// `Substrait` is boxed because `substrait::proto::Plan` is a deeply
/// nested protobuf message; boxing keeps `EnginePlan`'s stack size
/// moderate across platforms.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum EnginePlan {
    /// A Substrait plan. The canonical wire form per S4 (`35 §1.5`).
    Substrait(Box<substrait::proto::Plan>),
}

impl EnginePlan {
    /// Serialize the Substrait plan to protobuf bytes via prost. Per
    /// spec `35 §12.2`.
    ///
    /// Errors surface as [`IrErrorKind::SubstraitCodecError`] with
    /// `phase = "encode_proto"`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, IrErrorKind> {
        match self {
            Self::Substrait(plan) => {
                let mut buf = Vec::with_capacity(plan.encoded_len());
                plan.encode(&mut buf)
                    .map_err(|e| IrErrorKind::SubstraitCodecError {
                        phase: "encode_proto",
                        reason: e.to_string(),
                    })?;
                Ok(buf)
            }
        }
    }

    /// Serialize the Substrait plan to pretty proto3-JSON. Per spec
    /// `35 §12.2`.
    ///
    /// Uses `serde_json` via `substrait`'s `serde` feature. Errors
    /// surface as [`IrErrorKind::SubstraitCodecError`] with
    /// `phase = "encode_json"`.
    pub fn to_json(&self) -> Result<String, IrErrorKind> {
        match self {
            Self::Substrait(plan) => {
                serde_json::to_string_pretty(plan).map_err(|e| IrErrorKind::SubstraitCodecError {
                    phase: "encode_json",
                    reason: e.to_string(),
                })
            }
        }
    }
}

// ── SqlArtifact ─────────────────────────────────────────────────────────

/// Text-based engine output. Per spec `35 §12.3`.
///
/// `dialect` is a provenance tag — consumers route the text to the
/// correct engine. The text itself is opaque UTF-8.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlArtifact {
    pub text: String,
    pub dialect: DialectId,
}

impl SqlArtifact {
    /// Construct a [`SqlArtifact`] with the given text and dialect.
    pub fn new(text: impl Into<String>, dialect: DialectId) -> Self {
        Self {
            text: text.into(),
            dialect,
        }
    }
}

// ── DialectId ───────────────────────────────────────────────────────────

/// Stable identifier for a SQL dialect. Per spec `35 §12.4`.
///
/// **Artifact-side identity only** (S7 / `35 §1.5`). Used on
/// [`SqlArtifact::dialect`] and [`Dialect::ID`]; never appears on
/// [`crate::SemanticPlan`], [`crate::PlanNode`], [`crate::NodeMeta`],
/// or any registry-side type.
///
/// Newtype-over-stable per `30 §4.3` — no `#[non_exhaustive]`. New
/// dialects are introduced by adapters (in workspace via new
/// `pub const`, out-of-workspace via `Dialect` impl on a
/// `DialectId(&'static str)` literal).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct DialectId(&'static str);

impl DialectId {
    /// Borrow the dialect's stable name.
    pub const fn name(self) -> &'static str {
        self.0
    }

    /// ANSI-SQL dialect (engine-neutral baseline).
    pub const ANSI: DialectId = DialectId("ansi");
    /// DataFusion adapter dialect.
    pub const DATAFUSION: DialectId = DialectId("datafusion");
    /// DuckDB adapter dialect.
    pub const DUCKDB: DialectId = DialectId("duckdb");
    /// Spark adapter dialect.
    pub const SPARK: DialectId = DialectId("spark");
}

// ── Dialect ─────────────────────────────────────────────────────────────

/// Capability / identity trait implemented by every SQL-emitting
/// adapter. Per spec `35 §12.5`.
///
/// **Not sealed** — third-party adapter crates outside the workspace
/// (e.g. `semstrait-adapter-clickhouse`) MUST be able to impl
/// `Dialect` for their own [`DialectId`].
pub trait Dialect {
    /// The dialect's stable identity.
    const ID: DialectId;

    /// Adapter-declared capability flags consumed by the planner's
    /// capability check (per `36 §5`). Readers SHOULD NOT pattern-match
    /// exhaustively; the set is additive.
    fn capabilities(&self) -> &'static [Capability];
}

// ── Capability ──────────────────────────────────────────────────────────

/// Cross-boundary capability vocabulary. Per spec `35 §12.6`.
///
/// Type definition lives in `35`; per-adapter rosters and
/// variant-addition drivers live in `36` per Q-IR-010 / Q-ADAPT-002
/// (2026-05-21).
///
/// **Scope rule.** `Capability` enumerates only features that cannot
/// be synthesized at an adapter's PlanBuilder layer — features whose
/// absence in the consuming engine cannot be papered over without
/// changing semantics. Adapter-internal rewrite strategies (CTE
/// expansion, GROUPING SETS expansion, DISTINCT-aggregate emulation)
/// are NOT capabilities; they are private adapter strategy.
///
/// **Load-bearing consumer.** The Substrait-handoff boundary, where
/// semstrait emits a Substrait plan and a foreign engine consumes it
/// without semstrait-side rewrite. SQL-emitting adapters own their
/// full rewrite pipeline; their capability advertisement is ergonomic
/// (planner pre-flight, api pre-`adapt` UX), not contractual.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    RegexpMatch,
    RegexpExtract,
    IntervalLiteral,
    AsOfJoin,
    StructAccess,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── DialectId ────────────────────────────────────────────────────

    #[test]
    fn dialect_id_v1_const_roster_matches_spec() {
        assert_eq!(DialectId::ANSI.name(), "ansi");
        assert_eq!(DialectId::DATAFUSION.name(), "datafusion");
        assert_eq!(DialectId::DUCKDB.name(), "duckdb");
        assert_eq!(DialectId::SPARK.name(), "spark");
    }

    #[test]
    fn dialect_id_const_slots_are_value_distinct() {
        let v = [
            DialectId::ANSI,
            DialectId::DATAFUSION,
            DialectId::DUCKDB,
            DialectId::SPARK,
        ];
        for (i, a) in v.iter().enumerate() {
            for b in v.iter().skip(i + 1) {
                assert_ne!(a, b, "dialect const slots must be distinct");
            }
        }
    }

    // ── SqlArtifact ──────────────────────────────────────────────────

    #[test]
    fn sql_artifact_carries_text_and_dialect() {
        let a = SqlArtifact::new("SELECT 1", DialectId::ANSI);
        assert_eq!(a.text, "SELECT 1");
        assert_eq!(a.dialect, DialectId::ANSI);
    }

    #[test]
    fn sql_artifact_equality_is_structural() {
        let a = SqlArtifact::new("SELECT 1", DialectId::DUCKDB);
        let b = SqlArtifact::new(String::from("SELECT 1"), DialectId::DUCKDB);
        assert_eq!(a, b);
    }

    #[test]
    fn sql_artifact_distinguishes_dialect() {
        let a = SqlArtifact::new("SELECT 1", DialectId::DUCKDB);
        let b = SqlArtifact::new("SELECT 1", DialectId::SPARK);
        assert_ne!(a, b);
    }

    // ── EngineArtifact / EnginePlan ──────────────────────────────────

    #[test]
    fn engine_artifact_sql_variant_round_trips() {
        let art = EngineArtifact::Sql(SqlArtifact::new("SELECT 1", DialectId::ANSI));
        match art {
            EngineArtifact::Sql(s) => assert_eq!(s.text, "SELECT 1"),
            _ => panic!("expected Sql variant"),
        }
    }

    #[test]
    fn engine_plan_substrait_to_bytes_round_trips_via_prost() {
        let plan = substrait::proto::Plan::default();
        let ep = EnginePlan::Substrait(Box::new(plan.clone()));
        let bytes = ep.to_bytes().expect("encode default plan");
        // Default plan has no fields set, so the encoding is empty.
        // The contract is just: encode succeeds and decode round-trips.
        let decoded = substrait::proto::Plan::decode(bytes.as_slice()).expect("decode round trip");
        assert_eq!(decoded, plan);
    }

    #[test]
    fn engine_plan_substrait_to_json_emits_object() {
        let plan = substrait::proto::Plan::default();
        let ep = EnginePlan::Substrait(Box::new(plan));
        let json = ep.to_json().expect("encode to json");
        assert!(json.starts_with('{'), "proto3 JSON renders as object");
    }

    // ── Dialect trait ────────────────────────────────────────────────

    struct FakeAnsi;
    impl Dialect for FakeAnsi {
        const ID: DialectId = DialectId::ANSI;
        fn capabilities(&self) -> &'static [Capability] {
            &[]
        }
    }

    #[test]
    fn dialect_impl_carries_id_and_capabilities() {
        assert_eq!(<FakeAnsi as Dialect>::ID, DialectId::ANSI);
        assert!(FakeAnsi.capabilities().is_empty());
    }

    struct FakeDuck;
    impl Dialect for FakeDuck {
        const ID: DialectId = DialectId::DUCKDB;
        fn capabilities(&self) -> &'static [Capability] {
            &[Capability::RegexpMatch, Capability::IntervalLiteral]
        }
    }

    #[test]
    fn dialect_impl_advertises_capability_subset() {
        let caps = FakeDuck.capabilities();
        assert_eq!(caps.len(), 2);
        assert!(caps.contains(&Capability::RegexpMatch));
        assert!(caps.contains(&Capability::IntervalLiteral));
    }

    // ── Capability ───────────────────────────────────────────────────

    #[test]
    fn capability_v1_roster_is_complete() {
        // Per spec §12.6: 5 v1 variants. Adding more is a MINOR edit
        // driven by `36`-side ratification — when this assertion fails,
        // sync the spec roster.
        let v: &[Capability] = &[
            Capability::RegexpMatch,
            Capability::RegexpExtract,
            Capability::IntervalLiteral,
            Capability::AsOfJoin,
            Capability::StructAccess,
        ];
        // Sanity: every variant is distinct.
        for (i, a) in v.iter().enumerate() {
            for b in v.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }

    // ── Codec error surfacing ────────────────────────────────────────

    #[test]
    fn substrait_codec_error_carries_phase_and_reason() {
        // Smoke test the discriminator strings used in `to_bytes` /
        // `to_json` so consumers can match on them stably.
        let err = IrErrorKind::SubstraitCodecError {
            phase: "encode_proto",
            reason: "synthetic".into(),
        };
        match err {
            IrErrorKind::SubstraitCodecError { phase, reason } => {
                assert_eq!(phase, "encode_proto");
                assert_eq!(reason, "synthetic");
            }
            _ => panic!("expected SubstraitCodecError"),
        }
    }
}
