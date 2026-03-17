# Decision Log

All architectural and implementation decisions are recorded here.
Module-level decisions live in `crates/<module>/DECISION_LOG.md`.

---

## DL-001: Restructure from monolithic to 10-crate workspace

**Date:** 2026-03-17
**Status:** Accepted
**Context:** Current `semstrait-core` contains all logic (schema, parser, planner, DSL, compiler, output, diagnostics). CONTEXT.md specifies 10 crates with strict dependency DAG.
**Decision:** Restructure to match CONTEXT.md's 10-crate architecture. Move existing working code into appropriate new crates. Preserve all 203 existing tests.
**Rationale:** The 10-crate structure enforces dependency rules at the Cargo level, enables parallel compilation, and matches the design document that is the authoritative reference.

## DL-002: V1 focuses on plan generation, not execution

**Date:** 2026-03-17
**Status:** Accepted
**Context:** Full engine execution requires DuckDB, Trino, Spark connectors with complex wire protocols.
**Decision:** V1 generates LogicalPlan + ANSI SQL. Connector traits exist but implementations are stubs. Execution is v2.
**Rationale:** Validating the semantic model → plan → SQL pipeline end-to-end is the critical path. Execution adds integration complexity that can be layered on once the plan generation is proven correct.

## DL-003: Use sqlparser-rs AST for SQL generation

**Date:** 2026-03-17
**Status:** Accepted
**Context:** CONTEXT.md specifies using `sqlparser-rs` as intermediate form for syntactically correct SQL output.
**Decision:** PlanNode tree → sqlparser AST → String. No Jinja templates. Programmatic SQL construction only.
**Rationale:** sqlparser-rs guarantees syntactic correctness. Template-based approaches are fragile and hard to test.

## DL-004: Substrait is always produced, SQL is on-demand

**Date:** 2026-03-17
**Status:** Accepted (from CONTEXT.md design principle)
**Context:** Substrait bytes are the canonical IR.
**Decision:** Every plan compilation produces Substrait internally. SQL emission is an additional step, dialect-specific, derived from the same PlanNode tree.
**Rationale:** Substrait is the stable semantic contract. It's engine-agnostic, inspectable, and versionable.

## DL-005: Existing semstrait-core code forms the seed implementation

**Date:** 2026-03-17
**Status:** Accepted
**Context:** Current core has 203 passing tests covering schema types, DSL lexer/parser/lowering, planner IR, SQL emission, Substrait conversion, and constraint checking.
**Decision:** Extract and refactor existing code into new crates rather than rewriting from scratch. Preserve test coverage.
**Rationale:** The existing code is tested and correct for its scope. Rewriting introduces unnecessary risk and discards validated logic.

## DL-006: Optimizer is empty in v1 (identity function)

**Date:** 2026-03-17
**Status:** Accepted (from CONTEXT.md D4)
**Context:** Optimizer passes (predicate pushdown, projection pruning) add complexity.
**Decision:** `Optimizer` struct and `OptimizerPass` trait exist on day one. Zero passes registered by default. Passes are opt-in.
**Rationale:** The infrastructure cost is minimal. Adding passes later requires no API changes.

## DL-007: IcebergRestCatalog is stub-only in v1

**Date:** 2026-03-17
**Status:** Accepted
**Context:** Full Iceberg REST catalog requires Polaris/Gravitino integration.
**Decision:** `IcebergRestCatalog` struct exists behind `iceberg` feature flag. V1 implementation is minimal — enough to demonstrate the CatalogProvider trait contract. NullCatalogProvider is the default for testing.
**Rationale:** Catalog integration is secondary to plan generation correctness. The trait surface is what matters for v1.
