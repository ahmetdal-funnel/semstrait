# semstrait-ir

Intermediate Representation (IR) crate — defines the `LogicalPlan` and bidirectional Substrait serialization.

---

## Key Types

### PlanNode Variants

| Variant | Substrait Mapping |
|---------|-------------------|
| `ScanNode` | `ReadRel` with `NamedTable` |
| `FilterNode` | `FilterRel` |
| `ProjectNode` | `ProjectRel` with computed expressions |
| `AggNode` | `AggregateRel` with GROUP BY |
| `JoinNode` | `JoinRel` |
| `UnionNode` | `SetRel` (UNION ALL) |
| `SortNode` | `SortRel` (ORDER BY) |
| `FetchNode` | `FetchRel` (LIMIT/OFFSET) |

### NodeMeta

Every PlanNode carries metadata:
- `node_id` — unique UUID
- `output_schema` — `Schema` with stable field ordinals
- `annotations` — semantic annotations (aggregate role, filter source, etc.)

### Semantic Annotations

Annotations are serialized into Substrait `AdvancedExtension.detail`:
- `AggregateRole` — Final, SemiAdditiveInner, HorizontalSubResult, FanoutDedup
- `FilterSource` — DatasetFilter, MeasureFilter, MetricFilter, UserFilter
- `AdditivityAnnotation` — additivity metadata for measures
- `KindRef` — reference to the Kind being queried

### LogicalPlan

```rust
pub struct LogicalPlan {
    pub root: PlanNode,
    pub output_names: Vec<String>,  // semantic column names
}
```

---

## Substrait Serialization

```rust
// LogicalPlan → Substrait proto
let substrait_plan = SubstraitSerializer::to_substrait(&plan)?;

// Substrait proto → LogicalPlan
let back = SubstraitSerializer::from_substrait(&substrait_plan)?;
```

### ExprConverter

Converts `Expr` ↔ Substrait `Expression` for round-trip fidelity.

---

## Dependencies

- `semstrait-core` — `DataType`, `Schema`, `Expr`
- `substrait` v0.62 — Substrait protocol buffers
- `prost` v0.14 — protobuf encoding
- `uuid` — node identifiers
