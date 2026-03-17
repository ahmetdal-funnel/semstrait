# semstrait-ir

Semstrait Intermediate Representation (IR) crate.

## Overview

This crate defines the `LogicalPlan` IR with bidirectional Substrait serialization support. It depends only on `semstrait-core` and provides:

- **PlanNode enum**: Logical plan nodes with semantic annotations
- **LogicalPlan**: Wrapper for query plans with output names
- **ExprConverter**: Converts DslExpr to/from Substrait Expression
- **SubstraitSerializer**: Serializes LogicalPlan to/from Substrait Plan

## Architecture

### PlanNode Variants

- `ScanNode`: Table scan (maps to Substrait ReadRel with NamedTable)
- `FilterNode`: Filter operation (maps to Substrait FilterRel)
- `ProjectNode`: Projection with computed expressions (maps to Substrait ProjectRel)
- `AggNode`: Aggregation with GROUP BY (maps to Substrait AggregateRel)
- `JoinNode`: Join operations (maps to Substrait JoinRel)
- `UnionNode`: UNION ALL (maps to Substrait SetRel)
- `SortNode`: ORDER BY (maps to Substrait SortRel)
- `FetchNode`: LIMIT/OFFSET (maps to Substrait FetchRel)

### NodeMeta

Every PlanNode carries metadata:
- `node_id`: Unique UUID
- `output_schema`: Schema with stable field ordinals
- `annotations`: Semantic annotations (e.g., aggregate roles, filter sources)

### Semantic Annotations

Annotations are serialized into Substrait `AdvancedExtension.detail`:

- `AggregateRole`: Final, SemiAdditiveInner, HorizontalSubResult, FanoutDedup
- `FilterSource`: DatasetFilter, MeasureFilter, MetricFilter, UserFilter, etc.
- `AdditivityAnnotation`: Additivity metadata for measures
- `KindRef`: Reference to the Kind being queried
- `DomainHint`: Domain hint for dataset selection

## Usage

```rust
use semstrait_ir::*;
use semstrait_core::DataType;

// Build a simple plan
let schema = Schema::new(vec![
    Field::new("id", DataType::Int64),
    Field::new("amount", DataType::Float64),
]);

let scan = ScanNode {
    meta: NodeMeta::new(schema),
    table_name: "orders".to_string(),
    projection: vec!["id".to_string(), "amount".to_string()],
};

let plan = LogicalPlan::new(
    PlanNode::Scan(scan),
    vec!["id".to_string(), "amount".to_string()],
);

// Serialize to Substrait
let substrait_plan = SubstraitSerializer::to_substrait(&plan)?;

// Deserialize from Substrait
let back = SubstraitSerializer::from_substrait(&substrait_plan)?;
```

## Design Principles

1. **Schema Ordinals**: PlanNode ordinals match Substrait structField ordinals. Schema is always attached to each node. Parent nodes never guess field positions.

2. **Annotation Preservation**: Semantic annotations are preserved through Substrait round-trips using the extension mechanism.

3. **Separation of Concerns**: This crate handles only IR representation and Substrait serialization. Query planning logic lives in `semstrait-planner`.

## Dependencies

- `semstrait-core`: Foundation types (DataType, Schema)
- `substrait`: Substrait protocol buffers (0.62)
- `uuid`: Node identifiers
- `prost`: Protobuf support
- `thiserror`: Error types
- `serde`: Serialization support
