# DataFusion Substrait Execution Plan

**Status:** Complete | **Date:** 2026-03-31

---

## 1. Problem Statement

The DataFusion adapter produces `PlanArtifact::Substrait(proto::Plan)`, but the DataFusion connector only accepts `PlanArtifact::Sql`. This means the Substrait-first pipeline — the **primary path** per CONTEXT.md (E6) — is broken end-to-end.

```
Planner → LogicalPlan → DataFusionAdapter.adapt()
                            ↓
                    PlanArtifact::Substrait(proto::Plan)   ← adapter produces Substrait
                            ↓
                    DataFusionConnector.execute()
                            ↓
                    artifact.as_sql() → None → ERROR       ← connector expects SQL
```

The adapter and connector are misaligned: the adapter correctly targets Substrait (DataFusion's native interchange format), but the connector ignores it and falls back to SQL-only execution.

---

## 2. Version Alignment

All dependencies are already compatible — no upgrades needed.

| Dependency | Workspace | datafusion-substrait 52.x |
|---|---|---|
| `substrait` | 0.62 | 0.62 |
| `arrow` | 55 | 55 |
| `datafusion` | 52 | 52 |

**Key insight:** Both this project and `datafusion-substrait` depend on `substrait = "0.62"`, so the `substrait::proto::Plan` struct built by `SubstraitSerializer::to_substrait()` can be passed **directly** to `datafusion_substrait::from_substrait_plan()` — no serialization/deserialization boundary. Same Rust type, same crate version.

---

## 3. What Already Works

| Component | Location | Status |
|---|---|---|
| `SubstraitSerializer::to_substrait()` | `semstrait-ir/src/substrait/serializer.rs` | Complete — all 8 PlanNode types |
| `ExprConverter` (Expr ↔ Substrait Expression) | `semstrait-ir/src/substrait/expr_converter.rs` | Complete — 20+ expression types |
| `DataFusionAdapter.adapt()` | `semstrait-adapter/src/datafusion.rs` | Complete — produces `PlanArtifact::Substrait` |
| `EngineProfile::supports_substrait()` | `semstrait-core/src/engine_profile.rs` | Complete — returns `true` for DataFusion |
| `PlanArtifact::Substrait` variant | `semstrait-ir/src/artifact.rs` | Complete — with `to_json()`, `to_bytes()` |
| Table registration from manifest | `semstrait-connectors/src/datafusion.rs` | Complete — `register_manifest_sources()` |
| Debug SQL fallback | `semstrait-adapter/src/traits.rs` | Complete — `EngineAdapter::debug_sql()` |

---

## 4. What Needs to Change

### 4.1 Add `datafusion-substrait` dependency

**File:** `crates/semstrait-connectors/Cargo.toml`

```toml
[dependencies]
datafusion-substrait = { version = "52", default-features = false, optional = true }

[features]
datafusion = ["datafusion-engine", "datafusion-substrait", "tokio", "semstrait-adapter/datafusion", "semstrait-manifest"]
```

Feature-gated behind the existing `datafusion` feature flag. No new feature needed.

### 4.2 Update `DataFusionConnector.execute()` — dual-path execution

**File:** `crates/semstrait-connectors/src/datafusion.rs`

The `execute()` method currently rejects Substrait artifacts:

```rust
// CURRENT (broken for Substrait)
async fn execute(&self, artifact: &PlanArtifact) -> Result<ComputeResult, ConnectorError> {
    let sql = artifact.as_sql().ok_or_else(|| {
        ConnectorError::Execution("requires SQL artifact".to_string())
    })?;
    let df = self.ctx.sql(sql).await?;
    let batches = df.collect().await?;
    // ... convert to JSON
}
```

Replace with dual-path:

```rust
// PROPOSED (handles both SQL and Substrait)
async fn execute(&self, artifact: &PlanArtifact) -> Result<ComputeResult, ConnectorError> {
    let start = Instant::now();

    let batches = match artifact {
        PlanArtifact::Sql(sql) => {
            self.ctx.sql(sql).await
                .map_err(|e| ConnectorError::Execution(e.to_string()))?
                .collect().await
                .map_err(|e| ConnectorError::Execution(e.to_string()))?
        }
        PlanArtifact::Substrait(plan) => {
            let df_logical = datafusion_substrait::logical_plan::consumer::from_substrait_plan(
                &self.ctx.state(), plan
            ).await
                .map_err(|e| ConnectorError::Execution(
                    format!("Substrait plan consumption failed: {}", e)
                ))?;
            self.ctx.execute_logical_plan(df_logical)
                .map_err(|e| ConnectorError::Execution(e.to_string()))?
                .collect().await
                .map_err(|e| ConnectorError::Execution(e.to_string()))?
        }
    };

    // ... rest unchanged (Arrow → JSON, stats)
}
```

~15 lines changed. Both artifact types accepted — SQL for debugging, Substrait for production.

### 4.3 Audit extension URIs for DataFusion compatibility

**File:** `crates/semstrait-ir/src/substrait/anchors.rs`

Our `SubstraitSerializer` uses custom extension URIs and function anchors. DataFusion's `from_substrait_plan()` resolves functions by **URI + function name**. We must verify our URIs match the standard Substrait YAML extensions that DataFusion expects:

| Our URI pattern | Expected by DataFusion |
|---|---|
| Aggregate functions | `/functions_aggregate_generic.yaml` |
| Comparison functions | `/functions_comparison.yaml` |
| Boolean functions | `/functions_boolean.yaml` |
| Arithmetic functions | `/functions_arithmetic.yaml` |

**Action:** Read `anchors.rs`, compare URIs against the Substrait specification and DataFusion's consumer, fix any mismatches.

---

## 5. Risks and Edge Cases

### 5.1 Function anchor alignment (HIGH RISK)

Our `SubstraitSerializer` assigns function anchors (SUM=1, AVG=2, etc.) and extension URIs. DataFusion resolves functions by URI + name string lookup, not by anchor ID. If our URIs or function names don't match DataFusion's expectations, function resolution fails at runtime.

**Mitigation:** Audit `anchors.rs` against Substrait YAML specs before implementation. Fix any mismatches upfront.

### 5.2 NamedTable resolution (MEDIUM RISK)

Our `ScanNode` serializes as `ReadRel::NamedTable`. DataFusion's consumer resolves table names via `SessionState::table()`. The table names in `ScanNode.table` must match the names used in `register_manifest_sources()`.

Currently both use `binding.dataset_name` — should match. But multi-source datasets (where one dataset has multiple resolved sources) may register under the same name, causing conflicts.

**Mitigation:** Integration test with real manifest + registered tables.

### 5.3 Type mapping edge cases (LOW RISK)

Our `datatype_to_substrait()` maps:
- UInt types → Int64 (Substrait doesn't have unsigned)
- List/Struct → String (fallback)

DataFusion's consumer may not handle these gracefully. For v1, the core types (Int64, Float64, Utf8, Date) cover 95%+ of cases.

**Mitigation:** Document as known limitation. Fix when specific type failures appear.

### 5.4 SafeDivide semantics (NO RISK)

Per DL-024, `SafeDivide` maps to `Divide` in Substrait. The null-guard is SQL-level only. In DataFusion, division by zero on Float64 produces `inf`/`NaN`, not NULL. This is acceptable behavior — the Substrait path is not expected to have SQL-level null guards.

---

## 6. Table registration ordering constraint

`from_substrait_plan()` resolves table references via `SessionState`'s `TableProvider` registry. Tables **must** be registered before Substrait plan consumption.

Current flow already satisfies this:
```
1. register_manifest_sources()     ← tables registered
2. adapter.adapt(plan)             ← Substrait plan created
3. connector.execute(artifact)     ← plan consumed, tables resolved
```

No change needed, but this ordering is a hard requirement.

---

## 7. Testing Strategy

### 7.1 Unit test: Substrait roundtrip through DataFusion

Build `LogicalPlan` → serialize to Substrait → consume via `from_substrait_plan()` → verify DataFusion LogicalPlan schema matches expectations.

### 7.2 Integration test: Full pipeline with Substrait

```
YAML model → compile → plan → adapt(Substrait) → register tables → execute → verify rows
```

Use in-memory tables (Arrow RecordBatches) to avoid filesystem dependencies.

### 7.3 E2E comparison test: SQL vs Substrait

Execute the same query via both paths, compare results:
```rust
let sql_artifact = PlanArtifact::Sql(adapter.debug_sql(&plan)?);
let substrait_artifact = adapter.adapt(&plan)?;

let sql_result = connector.execute(&sql_artifact).await?;
let substrait_result = connector.execute(&substrait_artifact).await?;

assert_eq!(sql_result.data, substrait_result.data);
```

### 7.4 Reuse existing e2e model

Extend `tests/e2e_pipeline_test.rs` with Substrait-specific tests using `comprehensive_ecommerce.yaml`.

---

## 8. Implementation Order

| Step | Scope | Files | Risk |
|---|---|---|---|
| 1. Audit extension URIs | Research | `ir/substrait/anchors.rs` vs Substrait spec | Blocks everything |
| 2. Fix URI mismatches (if any) | IR crate | `ir/substrait/anchors.rs`, `ir/substrait/serializer.rs` | May break existing Substrait roundtrip tests |
| 3. Add `datafusion-substrait` dep | Connectors crate | `connectors/Cargo.toml` | Low — additive |
| 4. Dual-path `execute()` | Connectors crate | `connectors/src/datafusion.rs` | ~15 lines |
| 5. Integration tests | Connectors crate | `connectors/src/datafusion.rs` (tests) | Validates steps 1-4 |
| 6. E2E Substrait test | Root | `tests/e2e_pipeline_test.rs` | End-to-end validation |

---

## 9. Non-Goals (v1)

- **Physical plan execution** — we convert to DataFusion LogicalPlan, not PhysicalPlan. DataFusion handles optimization and physical planning internally.
- **Custom SubstraitConsumer** — use `from_substrait_plan()` (simple API) with `DefaultSubstraitConsumer`. Custom consumer only needed for UDF support (future).
- **Native Arrow result path** — results are still converted to JSON via `ArrowBatches::to_json_rows()`. Native Arrow output is a future optimization.
- **Bidirectional Substrait** — we only need plan consumption (Substrait → DataFusion). Plan production (DataFusion → Substrait) is not needed.
