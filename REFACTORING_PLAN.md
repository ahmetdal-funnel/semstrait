# Semstrait Refactoring Plan

**Version:** 2.0 | **Date:** 2026-03-19 | **Status:** Post-refactoring cleanup complete

---

## Completed

| Item | Category | Summary |
|------|----------|---------|
| R-001 | Bug | Union distinct flag preserved on Substrait roundtrip |
| R-002 | Safety | DuckDB `execute_batch` documented (multi-statement by design) |
| R-003 | Safety | `PlanNode` removed from facade re-exports |
| R-004 | Bug | Facade `explain()` uses connector's preferred dialect |
| R-005 | Safety | Unity catalog URL parameters encoded via `reqwest::query()` |
| R-006 | Safety | `Schema::project()` unwrap replaced with safe access |
| R-007 | Safety | `Expr::and_many`/`or_many` return `Option<Expr>` |
| R-010 | Dedup | Shared `build_dataset_plan()` in `kind/shared.rs` (~300 lines removed) |
| R-011/012 | Dedup | Manifest `compile_dimensions/measures/metrics` + `collect_interface_names` helpers |
| R-014 | Dedup | Default `adapt()` on `ComputeAdapter` trait |
| R-016 | Dedup | Default methods on `SqlDialect` trait |
| R-018 | Dedup | `CompileError`/`RepositoryError` migrated to `thiserror` |
| R-019 | Bug | gRPC transport maps all proto fields to `RawQueryRequest` |
| R-020 | API | Planner internals restricted to `pub(crate)` |
| R-041 | Deps | Removed unused `prost` from semstrait-ir |
| R-044 | Deps | Removed unused `arrow-schema`, `datafusion`, `sqlparser` from workspace deps |
| R-046 | Deps | Feature-gated `serde_json` in catalog behind `iceberg`/`unity` |
| R-047 | Deps | Removed unused `serde_json` from semstrait-model |
| R-050 | CLI | Extracted `compile_from_file`, `build_raw_request`, `run_query` helpers |

## Evaluated — No Change Needed

| Item | Reason |
|------|--------|
| R-015 | Arrow-to-JSON dedup blocked by DL-009 (DataFusion re-exports) |
| R-021 | `PlanNode` pub visibility — used by `semstrait-sql`, needs larger redesign |
| R-022 | Wildcard imports — high churn, low value |
| R-023 | `ComputeEmitter` — not dead; convenience for connector tests (DL-023) |
| R-024 | `PolyglotEmitter::dialect()` — never called in production |

## Remaining (Low Priority)

| Item | Category | Summary |
|------|----------|---------|
| R-013 | Dedup | Deserializer macro for 6 identical single-variant-map patterns in model |
| R-017 | Dedup | `AuthenticatedClient` extraction for catalog HTTP helpers |
| R-030–036 | Types | Type unification (GlobPattern, Relationship, JoinType, Grain, etc.) |
| R-051 | API | `validate()` / `to_resolved()` overlapping validation |
| R-052 | API | `explain()` is async but fully synchronous |
