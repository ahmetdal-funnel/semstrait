---
prereqs: [13, 14, 14a, 16, 17]
authoritative-for:
  - the `semstrait-ir` public-API surface (types, traits, free functions)
  - `SemanticPlan` — the canonical, engine-agnostic query plan tree
  - `PlanNode` sum type — variant roster (`Scan`, `Filter`, `Project`, `Agg`, `Join`, `Union`, `Sort`, `Fetch`) and per-variant shape
  - `EngineArtifact` / `EnginePlan` / `SqlArtifact` adapter-consumable output types (structural shape owned here; emission semantics in `36`)
  - `DialectId` roster and `Dialect` trait surface
  - shared plan-level primitives: `SourceRef`, `ResolvedColumn`, `Name`, `KeyPair`, `SortDir`, `NullOrdering`, `AggregateExpr`, `NodeMeta`
  - well-formedness invariants across a `SemanticPlan` tree (predicates are `PhysicalExpr`; types resolved per `14b`; group-by / join / scan schema alignment)
  - `IrError` enum and `IR_E_35xx` error-code range
  - serde posture for `SemanticPlan` and the Substrait mapping table (conversion is `36`'s concern; mapping declared here)
  - on-wire shape of `Cardinality` / `JoinType` when they appear on a `PlanNode::Join` (vocabulary ratified in `16 §5`)
refined-by:
  - 34 (`semstrait-planner` produces `SemanticPlan` values)
  - 36 (`semstrait-adapter` consumes `SemanticPlan` and produces `EngineArtifact`; owns the Substrait / SQL emission logic referenced here as a mapping only)
  - 40 (`implementation/40_refactor_plan.md` — current `crates/semstrait-ir/src/plan` vs target layout delta is tracked here)
---

# 35. semstrait-ir

> **Status:** ratified. `35` nails down the public surface of `semstrait-ir` — the canonical IR crate that sits between planner output and adapter input per `00 §9` I7. Every type exposed here is ratified upstream in `13`, `14`, `14a`, or `16`; `35` adds no new expression or type vocabulary. It ratifies the **plan-tree shape**, the **adapter-artifact shape**, and the **well-formedness invariants** the planner guarantees and the adapter relies on.

## 1. Purpose and Scope

`semstrait-ir` is the **canonical IR crate**. It defines a single, engine-agnostic, in-memory plan tree (`SemanticPlan`) plus the adapter-consumable output types the adapter layer produces from it. The planner (`semstrait-planner`, `34`) is its producer; every adapter crate (`semstrait-adapter` family, `36`+) is its consumer. No other crate in the workspace contains a plan-tree vocabulary.

### 1.1 What `semstrait-ir` OWNS

- The `SemanticPlan` root type (§3) and the `PlanNode` sum type (§4) — every variant, its fields, and the well-formedness invariants its children must satisfy (§7).
- Plan-level primitive types (§5): `SourceRef`, `ResolvedColumn`, `Name`, `KeyPair`, `SortDir`, `NullOrdering`, `AggregateExpr`, `NodeMeta`.
- The adapter-consumable output family (§6): `EngineArtifact`, `EnginePlan`, `SqlArtifact`, `DialectId`, `Dialect`. `36` refines *emission* (how an adapter fills them in); `35` ratifies their *shape*.
- The visitor / traversal API over `PlanNode` trees (§8).
- Serde derivations for `SemanticPlan` and every public IR type (§9).
- The `IrError` enum and its stable `IR_E_35xx` codes (§10).
- The Substrait-mapping **table** (§9.2) — declarative correspondence between each `PlanNode` variant and the `substrait::proto::Rel` kind an adapter emits. The conversion *code* lives in `36`.

### 1.2 What `semstrait-ir` does NOT own

- **Planning strategy and per-DataKind plan assembly.** Every decision that "this `Request` against this `Manifest` expands into a tree of `PlanNode`s in this order" lives in `semstrait-planner` per `34`. `35` ratifies only the **output shape** that planning must produce.
- **Optimization passes.** Rule-based rewrites over `SemanticPlan` live in `semstrait-planner` (`34`, stage 5 per `10 §3.5`). `35`'s `walk` / `transform` helpers (§8) are the substrate those rewrites run on, not the rewrites themselves.
- **Adapter emission.** Translating a `SemanticPlan` into an `EngineArtifact` (SQL text or Substrait proto) is `36`'s contract. `35` ratifies the artifact's structural shape and the Substrait mapping table; the rendering code, dialect-specific SQL, and capability checks all live above.
- **Manifest shape.** `SemanticPlan` references the Manifest for bindings and resolved expressions (§5.2) via opaque identifiers (`SourceRef`, `BindingId`); the Manifest types themselves live in `semstrait-manifest` per `33`. `35` never embeds `ResolvedDataKind` / `ResolvedBinding` values inline.
- **Expression AST.** Every expression on every `PlanNode` is a `semstrait_core::PhysicalExpr` (or its aggregate analogue — see §5.7). The AST definition, wrapper invariants, and `Expr` variant roster all live in `semstrait-core` per `31 §3`.

### 1.3 Design posture — pure, sync, canonical

`semstrait-ir` is deliberately **pure** (no I/O, no async, no engine identity). It is the plan-tree equivalent of `semstrait-core`'s expression AST: the data-only substrate the planner fills in and the adapter drains. The crate has:

- **Zero I/O surface.** Concrete I11 guarantee (per `30 §9`).
- **Zero async.** Every method on every public type is synchronous; `SemanticPlan` is built, walked, rewritten, and serialized on the caller's thread. I6 guarantee.
- **Zero engine identity.** No `datafusion::*`, no `arrow::*`, no `spark::*`, no `duckdb::*` types are visible on any `semstrait-ir` public surface. `DialectId` is an opaque newtype; `substrait::proto::Plan` is the one exception and appears only inside `EnginePlan::Substrait(_)` (§6.2) as the adapter-consumable payload.
- **Single upstream dependency.** `semstrait-ir` depends on `semstrait-core` only. Every other workspace crate depends on `semstrait-ir` directly (planner, adapter) or transitively (façade). `Cargo.toml` audit per §12.2 enforces this.

### 1.4 Engine-IR concept inspiration

`PlanNode` borrows its **catalog of operators** and **tree composition** from engine IRs — DataFusion's `LogicalPlan`, Calcite's `RelNode`, Substrait's `Rel`. These are the shapes a planner naturally produces; re-inventing them would be a waste. Per `00 §3`, the inspiration is **structural only**:

- Borrowed: the set of operators (`Scan`, `Filter`, `Project`, `Agg`, `Join`, `Union`, `Sort`, `Fetch`), the box-per-child tree shape, the invariant that every non-leaf carries typed inputs.
- Rejected: cost / statistics fields on any `PlanNode` (cost lives in the engine, not the canonical plan); physical / distribution properties (`Partitioning`, `Exchange`, `Shuffle`, `Repartition`); dialect or adapter branching on node variants; vendor-specific rel kinds.

## 2. Module Layout

Top-level `pub mod` structure. One module per cohesive concept.

```
semstrait-ir
├── plan                 // SemanticPlan, PlanNode, per-variant structs, NodeMeta
│   ├── node             //   PlanNode enum + variant-struct shapes
│   └── traversal        //   PlanVisitor, walk_pre / walk_post / transform
├── primitives           // SourceRef, ResolvedColumn, Name, KeyPair, SortDir,
│                        //   NullOrdering, AggregateExpr
├── artifact             // EngineArtifact, EnginePlan, SqlArtifact, DialectId, Dialect
├── error                // IrError (35-owned error enum)
└── substrait_map        // Substrait mapping TABLE only (no conversion code)
```

**Split rationale:**

- `plan` vs `primitives` — `PlanNode` references every primitive type, but not vice-versa. Keeping primitives alphabetically separate lets future crates (e.g. a plan-diff tool) depend only on `primitives` without linking the full `PlanNode` surface.
- `plan::node` vs `plan::traversal` — the traversal API's method count scales with the `PlanNode` variant count; isolating it limits I10 blast radius when a new variant lands.
- `artifact` as a separate module — `EngineArtifact` is the *output* shape. It is naturally decoupled from the *input* shape (`SemanticPlan`) and is consumed by the engine layer above `semstrait-adapter` (executors, wrappers, CLI); isolating it keeps the import graph clean.
- `error` is its own module mirroring the `semstrait-core` split (`31 §2`).
- `substrait_map` exists as a table reference, not a conversion module. The actual conversion code lives in `36` (which owns the substrait-proto emission and consumption logic).

**Re-exports.** The crate root re-exports a curated surface (§14). Non-root re-exports of internal helpers are forbidden.

## 3. Public Types — `SemanticPlan` Root

### 3.1 Shape

```rust
/// The canonical, engine-agnostic query plan tree. Output of the planner
/// (`34`), input of every adapter (`36`). Per `00 §4.1`.
///
/// A `SemanticPlan` is a single rooted tree of `PlanNode`s plus
/// plan-wide metadata: the names in the final projection order, and any
/// `Diagnostic`s the planner wishes to surface alongside the plan
/// (warnings, informational notes — never errors, which abort planning
/// per `10 §3.4`).
#[non_exhaustive]
pub struct SemanticPlan {
    /// The root of the plan tree. Never empty; planning produces at
    /// least a `Scan` leaf.
    pub root: PlanNode,

    /// Output column names in the order they appear in `root`'s
    /// output schema. Length equals `root.meta().output_schema.len()`.
    /// Each entry is the user-visible column label the adapter emits
    /// (e.g. the SELECT-list alias in generated SQL).
    pub output_names: Vec<Name>,

    /// Non-error diagnostics surfaced by `plan` + `optimize`. Contains
    /// only `Severity::Warning` / `Severity::Note` entries; errors
    /// abort planning and never reach `SemanticPlan` per `10 §3.4`.
    /// Adapters MAY append their own non-error diagnostics during
    /// `adapt` but are not required to (see `36`).
    pub diagnostics: Vec<semstrait_core::Diagnostic>,
}
```

### 3.2 Invariants

A `SemanticPlan` is **well-formed** when:

1. `output_names.len() == root.meta().output_schema.len()` — every output column has a user-visible name.
2. Every `Name` in `output_names` is a valid identifier (see §5.4).
3. `root` and every descendant satisfy the tree invariants of §7.
4. No `Diagnostic` in `diagnostics` has `Severity::Error`.

Construction does **not** re-check invariants 1–3 at runtime (planning established them; re-checking is a planner-regression catch, not a caller contract). An optional `SemanticPlan::validate()` method (§8.3) walks the tree and reports violations as `IrError` for debugging.

### 3.3 Serde

`SemanticPlan` derives `Serialize` / `Deserialize` under the crate-level `serde` feature. The wire form is the direct struct shape; no intermediate envelope. `PhysicalExpr` inside child nodes serializes through `semstrait-core`'s expression serde (`31 §4.5`). `PlanNode` (`#[non_exhaustive]`) uses serde's `untagged` policy with a discriminator field (`kind: "scan" | "filter" | ...`) so the wire form survives the addition of new variants per I10.

A serialized `SemanticPlan` is a format-stable portable plan artifact: two processes with the same compiled Manifest can exchange a `SemanticPlan` and get identical adapter output. This is what makes the crate a faithful IR.

### 3.4 Construction patterns

Planners build a `SemanticPlan` from the bottom up:

```rust
let scan = PlanNode::Scan(ScanNode {
    meta: NodeMeta::new(binding.output_schema()),
    source: SourceRef::from_binding(&binding.id),
    columns: binding.columns_in_native_order(),
    filters_pushdown: Vec::new(),
});

let filter = PlanNode::Filter(FilterNode {
    meta: NodeMeta::new_shared(scan.meta().output_schema.clone()),
    input: Box::new(scan),
    predicate: physical_expr_from_request(req)?,
});

let plan = SemanticPlan {
    root: filter,
    output_names: vec![Name::new("id")?, Name::new("amount")?],
    diagnostics: Vec::new(),
};
```

Builders in `semstrait-planner` (`34`) wrap these constructions in a higher-level fluent API; `35` exposes only the struct-literal form so any consumer can construct / inspect a plan without the planner linked.

### 3.5 Cloning

`SemanticPlan: Clone`. Every child node is `Box`-owned; `Clone` is a deep clone. For cheap structural sharing inside optimizer passes, use `walk_post` with in-place `transform` (§8) rather than successive `clone`s.

## 4. Public Types — `PlanNode` Sum

### 4.1 The sum type

```rust
/// A single node within a `SemanticPlan`. The variant set forms the
/// canonical operator catalog borrowed (structurally only) from
/// DataFusion's `LogicalPlan`, Calcite's `RelNode`, and Substrait's
/// `Rel`. Per `00 §4.1` and `35 §1.4`.
///
/// `#[non_exhaustive]` per I10: adding a variant (e.g. a future
/// `Distinct`, `Window`, `Limit` that departs from `Fetch`'s shape) is
/// MINOR.
#[non_exhaustive]
pub enum PlanNode {
    Scan    (ScanNode),
    Filter  (FilterNode),
    Project (ProjectNode),
    Agg     (AggNode),
    Join    (JoinNode),
    Union   (UnionNode),
    Sort    (SortNode),
    Fetch   (FetchNode),
}

impl PlanNode {
    /// Shared accessor for the `NodeMeta` that every variant carries.
    pub fn meta(&self) -> &NodeMeta;
    pub fn meta_mut(&mut self) -> &mut NodeMeta;

    /// Shared accessor for the set of child `PlanNode`s (0, 1, or 2+).
    pub fn children(&self) -> Vec<&PlanNode>;
    pub fn children_mut(&mut self) -> Vec<&mut PlanNode>;
}
```

Eight variants in v1. Every variant wraps a struct (not tuple / record form) so field additions inside a variant are MINOR per `30 §4.2` (non-exhaustive struct growth).

### 4.2 `Scan` — leaf source

```rust
/// Reads a resolved source. The only `PlanNode` variant without child
/// nodes. Per `15 §10.6` the planner walks the `ResolvedBinding` and
/// emits one `ScanNode` per physical source unit.
#[non_exhaustive]
pub struct ScanNode {
    pub meta: NodeMeta,

    /// Opaque handle into the Manifest. Resolves to a
    /// `ResolvedPhysicalSource` per `15 §7.1`. Adapters consume the
    /// Manifest + this handle to learn the on-engine table / path /
    /// format. `35` never stores the expanded path.
    pub source: SourceRef,

    /// The projected columns in the order the `Scan` outputs them.
    /// Each `ResolvedColumn` carries `{ name, data_type, nullable,
    /// ordinal }` per `15 §4.2`. Non-empty.
    pub columns: Vec<ResolvedColumn>,

    /// Push-down predicates resolved at plan time. Each `PhysicalExpr`
    /// references only columns in `columns` (enforced by §7.4). Empty
    /// when no pushdown is applicable or the adapter does not support
    /// it. Optimizer-filled per `34`; adapters MAY further narrow
    /// (pushing deeper into the source) or MAY decline (pulling back
    /// up into a `Filter`) per their capabilities.
    pub filters_pushdown: Vec<semstrait_core::PhysicalExpr>,
}
```

`ScanNode` carries **no raw path, no URL, no dialect**. Resolution from `SourceRef` to on-engine identity happens in the adapter via the Manifest (I1).

### 4.3 `Filter` — predicate

```rust
#[non_exhaustive]
pub struct FilterNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    /// Boolean-valued predicate. Operand type must be `Boolean` per
    /// `14 §5.6`. Columns referenced must exist in `input.meta()
    /// .output_schema`.
    pub predicate: semstrait_core::PhysicalExpr,
}
```

Pass-through schema: `FilterNode.meta.output_schema` equals `input.meta().output_schema` (enforced at construction per §5.8; adapters rely on this).

### 4.4 `Project` — column list

```rust
#[non_exhaustive]
pub struct ProjectNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,

    /// Ordered list of `(output_name, expression)` pairs. The result
    /// schema's field at ordinal `i` has name `projections[i].0` and
    /// `data_type = projections[i].1.inferred_type`. Empty list is
    /// rejected at construction (trivial `Project` collapses to
    /// `input`).
    pub projections: Vec<(Name, semstrait_core::PhysicalExpr)>,
}
```

### 4.5 `Agg` — group-by + aggregates

```rust
#[non_exhaustive]
pub struct AggNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,

    /// Group-by keys by output-schema column name. Each `Name` must
    /// resolve to a column in `input.meta().output_schema`.
    /// An empty `group_by` is a grand-total aggregation (equivalent
    /// to SQL `SELECT agg(...) FROM ...` with no `GROUP BY`).
    pub group_by: Vec<Name>,

    /// Aggregate expressions paired with their output-column names.
    /// Each `AggregateExpr` is an `Expr::Aggregate` kernel (see §5.7)
    /// whose inner expression references only columns in
    /// `input.meta().output_schema`.
    pub aggregates: Vec<(Name, AggregateExpr)>,
}
```

### 4.6 `Join` — binary composition

```rust
#[non_exhaustive]
pub struct JoinNode {
    pub meta: NodeMeta,
    pub left: Box<PlanNode>,
    pub right: Box<PlanNode>,

    /// Join kind. Per `16 §5.2` and `00 §4.1`. `35` is authoritative
    /// for the on-wire shape; `16` is authoritative for the vocabulary
    /// (Inner / Left / Right / Full).
    pub join_type: JoinType,

    /// Cardinality annotation carried from the Relationship graph
    /// (`16 §5.1`). Planners fill this from the `Relationship`
    /// ratified in `16`; optimizers / adapters MAY use it for
    /// optimization hints (e.g. `OneToOne` → redundant-join
    /// elimination). Never elided; always reflects the authored
    /// Relationship.
    pub cardinality: Cardinality,

    /// Join keys. Each `KeyPair` names one column from `left`'s
    /// schema and one from `right`'s. Non-empty for equi-joins
    /// (`Inner`, `Left`, `Right`, `Full`) in v1; cross-join is not a
    /// v1 variant (see §11.1 TD-IR-CROSS-JOIN).
    pub on: Vec<KeyPair>,
}
```

Non-equi-join predicates (range joins, inequality joins) are deferred — a future `JoinNode.residual: Option<PhysicalExpr>` field is MINOR per §11.1.

### 4.7 `Union` — n-ary stack

```rust
#[non_exhaustive]
pub struct UnionNode {
    pub meta: NodeMeta,

    /// Two or more inputs. Every input's `output_schema` must be
    /// structurally compatible (same arity, same element types, same
    /// nullability — per §7.6).
    pub inputs: Vec<PlanNode>,

    /// Whether to apply `DISTINCT` deduplication after the union.
    /// `false` = `UNION ALL` (bag semantics); `true` = `UNION` (set
    /// semantics). Defaults to `false` — every engine natively
    /// supports `UNION ALL` without rewrite; `true` demands a
    /// post-hash-agg pass.
    pub distinct: bool,
}
```

### 4.8 `Sort` — ordering

```rust
#[non_exhaustive]
pub struct SortNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,

    /// Sort keys in priority order (first entry is the primary key).
    /// Each `Name` resolves to a column in `input`'s schema.
    /// Each `SortDir` carries ascending/descending + null-ordering.
    pub order: Vec<(Name, SortDir)>,
}
```

### 4.9 `Fetch` — limit / offset

```rust
#[non_exhaustive]
pub struct FetchNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,

    /// Row limit. `None` = unlimited (equivalent to no `LIMIT` clause
    /// in SQL). `Some(0)` is well-formed; adapters may short-circuit
    /// emission.
    pub limit: Option<u64>,

    /// Row offset. `None` = no offset. `Some(0)` is equivalent to
    /// `None` and kept as a distinct value for Substrait-roundtrip
    /// fidelity.
    pub offset: Option<u64>,
}
```

Unsigned integer (`u64`) deliberately rejects negative values at the type boundary; the `Option` shape keeps "no limit / no offset" distinct from "limit 0 / offset 0". This matches Substrait's `FetchRel.count` / `offset` (both `i64`) at the upper half of `u64`'s range; the rare `u64 > i64::MAX` case is rejected at adapter-emit time with `IR_E_3510 FetchValueOutOfRange`.

## 5. Public Types — Shared Primitives

### 5.1 `NodeMeta`

```rust
/// Metadata attached to every `PlanNode`. Per `15 §7` and `17 §3`.
#[non_exhaustive]
pub struct NodeMeta {
    /// Unique identifier for this node in the plan tree. Used by the
    /// optimizer (rule-engine source tracking) and the adapter
    /// (diagnostic correlation). Not stable across planner
    /// invocations — two runs against the same Manifest + Request
    /// MAY produce different `NodeId`s.
    pub node_id: NodeId,

    /// Output schema after this node. `Arc` allows pass-through
    /// nodes (Filter, Sort, Fetch) to share the parent schema without
    /// deep-cloning.
    pub output_schema: std::sync::Arc<Schema>,

    /// Semantic annotations attached by `semstrait-planner`
    /// (additivity role, filter source, kind-ref, etc.). Consumed by
    /// the adapter's Substrait-emission path to round-trip semstrait
    /// context across the Substrait boundary.
    pub annotations: Vec<SemAnnotation>,
}
```

`NodeId` is a newtype over `Uuid::new_v4()` in v1; external consumers should treat it as opaque. `Schema` is a plan-level structural schema (not a Manifest-level `ResolvedDataKind` schema) — `{ fields: Vec<Field> }` where `Field { name: Name, data_type: DataType, nullable: bool }` per `15 §4.2`.

`SemAnnotation` is an additive `#[non_exhaustive]` sum (AggregateRole, FilterSource, Additivity, KindRef, …) ratified in `34`'s planner notes; `35` re-exports the enum for the purpose of serde-roundtrip fidelity and adapter consumption.

### 5.2 `SourceRef`

```rust
/// Opaque reference to a `ResolvedPhysicalSource` in the Manifest.
/// Per `15 §7.1` / `00 §4.1`.
///
/// `SourceRef` is a deliberately opaque handle — adapters resolve it
/// against the Manifest they were handed alongside the `SemanticPlan`.
/// No path, URL, catalog name, or file format leaks into the plan
/// tree. I1 guarantee.
///
/// Newtype-over-stable exception per `30 §4.3`: no `#[non_exhaustive]`
/// on the outer newtype; the inner variant is crate-private.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct SourceRef(/* crate-private: (BindingId, u32) */);

impl SourceRef {
    /// Construct from a `(binding_id, source_index)` pair. Called by
    /// `semstrait-planner` at `ScanNode` creation time.
    pub fn new(binding_id: BindingId, source_index: u32) -> Self;

    pub fn binding_id(self) -> BindingId;
    pub fn source_index(self) -> u32;
}
```

### 5.3 `ResolvedColumn`

```rust
/// A column as projected by a `Scan`. Per `15 §4.2`.
#[non_exhaustive]
pub struct ResolvedColumn {
    pub name: Name,
    pub data_type: semstrait_core::DataType,
    pub nullable: bool,
    /// Ordinal in the underlying source's native schema order. Adapters
    /// consume this when emitting stable column references.
    pub ordinal: u32,
}
```

### 5.4 `Name`

```rust
/// Identifier used for output-column names, group-by keys, sort keys,
/// and projection aliases. A plan-level newtype over `String` with a
/// construction boundary that enforces identifier well-formedness:
///
/// - Non-empty.
/// - UTF-8 (guaranteed by `String`).
/// - Not a reserved plan-tree tag (see §5.4.1).
///
/// `Name` is **not** normalized (case-folding, whitespace-collapsing)
/// — two distinct `Name`s may differ only in case, matching
/// case-preserving-but-case-sensitive semantics from `11 §5`.
///
/// Newtype-over-stable exception per `30 §4.3`: no
/// `#[non_exhaustive]`.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct Name(String);

impl Name {
    /// Validates the identifier; returns `IrError::InvalidName` on
    /// violation.
    pub fn new(s: impl Into<String>) -> Result<Self, IrError>;

    pub fn as_str(&self) -> &str;
    pub fn into_string(self) -> String;
}
```

#### 5.4.1 Reserved plan-tree tags

Substrait-roundtrip fidelity reserves a small set of identifier prefixes for semstrait's own use: `__semstrait_`, `__plan_`, `__agg_`. Constructing a `Name` with one of these prefixes raises `IrError::ReservedName`. The reserved-prefix set is additive; adding new prefixes is MINOR.

### 5.5 `KeyPair`

```rust
/// One join-key pair on a `JoinNode.on`. Per `16 §5.1`.
///
/// Both `left` and `right` are column names resolving against the
/// join's corresponding child's `output_schema`. Column types must
/// match per §7.5 — planner-side reconciliation lives in `14b`;
/// Cast-wrapping lives in `15 §10.5`.
#[non_exhaustive]
pub struct KeyPair {
    pub left: Name,
    pub right: Name,
}
```

### 5.6 `SortDir` / `NullOrdering`

```rust
/// Sort direction + null-ordering bundle. Per Substrait's
/// `SortField.direction`.
#[non_exhaustive]
pub enum SortDir {
    /// Ascending order.
    Asc  { nulls: NullOrdering },
    /// Descending order.
    Desc { nulls: NullOrdering },
}

/// Where to place `NULL` values in a sort.
#[non_exhaustive]
pub enum NullOrdering {
    /// Place `NULL` values first.
    First,
    /// Place `NULL` values last.
    Last,
    /// Let the adapter choose (SQL default: ASC NULLS LAST, DESC NULLS
    /// FIRST — most engines; adapters MAY narrow). Carries zero
    /// semstrait-side constraint.
    Unspecified,
}

impl SortDir {
    pub const ASC_NULLS_FIRST:  SortDir = SortDir::Asc { nulls: NullOrdering::First };
    pub const ASC_NULLS_LAST:   SortDir = SortDir::Asc { nulls: NullOrdering::Last  };
    pub const DESC_NULLS_FIRST: SortDir = SortDir::Desc { nulls: NullOrdering::First };
    pub const DESC_NULLS_LAST:  SortDir = SortDir::Desc { nulls: NullOrdering::Last  };
}
```

Keeping null-ordering on `SortDir` (rather than a separate `Sort::null_order: NullOrdering` field next to each key) keeps the per-key shape tight and matches Substrait's wire form.

### 5.7 `AggregateExpr`

```rust
/// A single aggregate kernel on `AggNode.aggregates`. Wraps an
/// `Expr::Aggregate` with plan-level shape:
///
/// - `aggregation` and `distinct` match `core::Expr::Aggregate`'s
///   `aggregation` and `distinct` fields per `14 §3.2`.
/// - `input_expr` is the inner expression (e.g. `Column("amount")`
///   for `sum(amount)`; `Literal(1)` for `count(1)`).
/// - `filter` is the optional `FILTER (WHERE ...)` clause Substrait
///   supports natively; `None` in v1 since `14 §3.2` does not ratify
///   an aggregate-filter on the inner `Expr::Aggregate`. The field is
///   reserved (`[TD-IR-AGG-FILTER]`) and adapters MUST accept `None`.
///
/// `AggregateExpr` is NOT `PhysicalExpr` — the wrapper invariants
/// for `PhysicalExpr` forbid `Expr::Aggregate` (`14 §2.3` / `31 §3.3`).
/// Aggregates are plan-level primitives: they carry through `AggNode`
/// and never appear inside `FilterNode.predicate` or
/// `ProjectNode.projections[*].1`.
#[non_exhaustive]
pub struct AggregateExpr {
    pub aggregation: semstrait_core::Aggregation,
    pub input_expr:  semstrait_core::PhysicalExpr,
    pub distinct:    bool,
    pub filter:      Option<semstrait_core::PhysicalExpr>,
    pub inferred_type: semstrait_core::DataType,
}
```

The `inferred_type` field is populated by the planner's type-inference pass (`34`, `14 §5.4` aggregate-typing table). Adapters MAY read it directly without re-deriving from the `aggregation` + `input_expr.inferred_type`.

### 5.8 Invariants enforced at construction

Each `PlanNode` variant's struct is constructed directly (no hidden builder); the variant's field combination is the contract. Schema invariants (§7) are *not* checked at construction — consumers rely on the planner to produce well-formed trees and rely on `SemanticPlan::validate()` (§8.3) for a debug-only full re-check.

## 6. Public Types — Adapter Artifact Family

The output types produced by `semstrait-adapter::adapt()` (`36`) from a `SemanticPlan`. `35` ratifies the structural shape; `36` owns the emission semantics.

### 6.1 `EngineArtifact`

```rust
/// Engine-ready plan artifact. Produced by `EngineAdapter::adapt()`
/// in `36`.
///
/// Two variants covering the two adapter emission modes:
/// - `Sql` for engines that consume SQL strings (DuckDB, Spark,
///   PostgreSQL, …).
/// - `Plan` for engines that consume Substrait plans natively
///   (DataFusion, some Spark paths, future Substrait-consuming
///   engines).
#[non_exhaustive]
pub enum EngineArtifact {
    Sql  (SqlArtifact),
    Plan (EnginePlan),
}
```

### 6.2 `EnginePlan`

```rust
/// Structured-IR engine output. Variants today: `Substrait(...)`.
/// Non-exhaustive per I10 — future engine IRs (e.g. direct
/// DataFusion-LogicalPlan emission, future vendor IRs) can be added
/// as MINOR variants.
#[non_exhaustive]
pub enum EnginePlan {
    /// A Substrait plan. Boxed to keep `EnginePlan`'s size moderate
    /// across platforms.
    Substrait(Box<substrait::proto::Plan>),
}

impl EnginePlan {
    /// Serialize the Substrait plan to protobuf bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, IrError>;
    /// Serialize the Substrait plan to pretty JSON.
    pub fn to_json(&self) -> Result<String, IrError>;
}
```

### 6.3 `SqlArtifact`

```rust
/// Text-based engine output. Per `00 §4.1`.
#[non_exhaustive]
pub struct SqlArtifact {
    /// The emitted SQL text. UTF-8.
    pub text: String,
    /// The dialect this text targets. Consumers use this to route the
    /// text to the correct engine.
    pub dialect: DialectId,
}
```

### 6.4 `DialectId` + `Dialect`

```rust
/// Stable identifier for a SQL dialect. Per `00 §4.1` and `36`.
///
/// Implemented as a newtype over a `&'static str` with `pub const`
/// identities per built-in adapter; adapters outside the workspace
/// register new dialects via the `Dialect` trait (§6.5).
///
/// Newtype-over-stable exception per `30 §4.3`: no
/// `#[non_exhaustive]`.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct DialectId(&'static str);

impl DialectId {
    pub const fn name(self) -> &'static str { self.0 }

    // Canonical built-in dialect identities — one `pub const` per
    // in-workspace adapter per `00 §4.1`.
    pub const ANSI:       DialectId = DialectId("ansi");
    pub const DATAFUSION: DialectId = DialectId("datafusion");
    pub const DUCKDB:     DialectId = DialectId("duckdb");
    pub const SPARK:      DialectId = DialectId("spark");
}
```

### 6.5 `Dialect` trait

```rust
/// Capability / identity trait implemented by every SQL-emitting
/// adapter. Per `00 §4.1` and `36 §3`.
///
/// Not sealed — third-party adapter crates outside the workspace
/// (e.g. `semstrait-adapter-clickhouse`) MUST be able to impl
/// `Dialect` for their own `DialectId`.
pub trait Dialect {
    /// The dialect's stable identity.
    const ID: DialectId;

    /// Adapter-declared capability flags consumed by the planner's
    /// capability check (`36 §5`). Readers SHOULD NOT pattern-match
    /// exhaustively; the set is additive.
    fn capabilities(&self) -> &'static [Capability];
}
```

`Capability` is a `#[non_exhaustive]` enum ratified in `36` (e.g. `DistinctAggregate`, `RegexpMatch`, `AsOfJoin`, `StructAccess`); `35` re-exports it only for the purposes of adapter compile-time integration. Adapter-side capability roster ownership is `36`'s.

### 6.6 `Capability`

```rust
/// Dialect / adapter capability vocabulary. Ratified by `36`; re-exported
/// here so `SemanticPlan` consumers can interrogate an adapter before
/// calling `adapt`.
#[non_exhaustive]
pub enum Capability {
    DistinctAggregate,
    RegexpMatch,
    RegexpExtract,
    IntervalLiteral,
    AsOfJoin,
    StructAccess,
    // ... full enumeration lives in `36`
}
```

## 7. Tree Invariants

A **well-formed** `SemanticPlan` satisfies every invariant below. The planner (`34`) is the canonical producer; every invariant below is the planner's contract. `SemanticPlan::validate()` (§8.3) is an optional post-hoc walker that reports violations as `IrError`.

### 7.1 Expression-wrapper invariants

- Every predicate-valued expression on a `PlanNode` is a `PhysicalExpr` — never a `SemanticExpr`. This applies to `FilterNode.predicate`, `ScanNode.filters_pushdown[*]`, `ProjectNode.projections[*].1`, `AggregateExpr.input_expr`, `AggregateExpr.filter`. Invariant rationale: `EntityRef` resolution completed at `compile` per `14b`; every reachable expression from the plan root is binding-side, not semantics-side. `ValidateError::EntityRefInPhysicalExpr` per `31 §8.2` would have fired at compile.
- No `PhysicalExpr` carries an `Expr::Aggregate` — aggregation is carried through `AggNode.aggregates` as `AggregateExpr` (§5.7). Invariant rationale: `PhysicalExpr` wrapper rejects `Aggregate` per `14 §2.3` / `31 §3.3`.

### 7.2 Type-resolution invariants

- Every `PhysicalExpr` on every `PlanNode` has `inferred_type.is_some()` — type inference completed at `compile` per `14b §4.1`. An unresolved type at plan time is `IR_E_3502 UnresolvedType`.
- Every `AggregateExpr.inferred_type` is populated per `14 §5.4`'s aggregate-typing table.

### 7.3 Scan-schema invariants

- `ScanNode.columns[*]` references actual columns of the resolved source. The planner populates `columns` from the Manifest's `ResolvedBinding.sources[source_index].columns` — if the Manifest is consistent with the plan, this invariant holds.
- `ScanNode.meta.output_schema.len() == ScanNode.columns.len()`.
- `ScanNode.meta.output_schema.fields[i].name == ScanNode.columns[i].name` for all `i`.

### 7.4 Push-down invariants

- Every `PhysicalExpr` in `ScanNode.filters_pushdown` references only columns in `ScanNode.columns` (enforced by adapter at `36`, optimizer at `34`).
- `filters_pushdown` does not change `meta.output_schema` — it narrows row count, not column shape.

### 7.5 Join invariants

- `JoinNode.on` is non-empty. (Cross-joins deferred per §11.1.)
- For each `KeyPair`, `left` resolves to a column in `left.meta().output_schema` and `right` resolves to a column in `right.meta().output_schema`.
- For each `KeyPair`, `left`'s column `data_type` matches `right`'s (modulo nullability). Type reconciliation is a planner responsibility (`15 §10.5` Cast-wrapping at Manifest compile time); a mismatch reaching `35` is `IR_E_3503 JoinKeyTypeMismatch`.
- `JoinNode.meta.output_schema` = structural concatenation of `left.meta().output_schema` and `right.meta().output_schema`, with nullability widened on the outer side per `join_type` (per SQL semantics).

### 7.6 Union invariants

- `UnionNode.inputs.len() >= 2`.
- All inputs have structurally compatible output schemas: same arity; same `DataType` at each ordinal; same nullability at each ordinal (after upward widening of nullable-to-non-nullable mismatches by the planner).
- `UnionNode.meta.output_schema` = first input's schema with nullability widened across inputs (per SQL semantics).

### 7.7 Agg invariants

- Every `Name` in `AggNode.group_by` resolves to a column in `input.meta().output_schema`.
- Every `(Name, AggregateExpr)` in `AggNode.aggregates` has a unique output `Name`. Duplicate output-name is `IR_E_3504 DuplicateAggName`.
- The inner `input_expr` of each `AggregateExpr` references only columns in `input.meta().output_schema`.
- `AggNode.meta.output_schema` = one column per `group_by` entry (in that order) followed by one column per `aggregates` entry (in that order).

### 7.8 Sort invariants

- Every `Name` in `SortNode.order[*].0` resolves to a column in `input.meta().output_schema`.
- Pass-through schema: `SortNode.meta.output_schema == input.meta().output_schema` (cheap via `Arc` share).

### 7.9 Fetch invariants

- If `FetchNode.limit == Some(0)`, the adapter MAY short-circuit to an empty-relation emission (e.g. `SELECT ... FROM ... WHERE false`); this is an adapter choice, not a plan-tree invariant.
- Pass-through schema: `FetchNode.meta.output_schema == input.meta().output_schema`.

### 7.10 Filter invariants

- `FilterNode.predicate.inferred_type == Some(DataType::Boolean)`. A non-Boolean predicate reaching `35` is `IR_E_3505 FilterPredicateNotBoolean`.
- Pass-through schema: `FilterNode.meta.output_schema == input.meta().output_schema`.

## 8. Visitor / Traversal API

### 8.1 `PlanVisitor`

```rust
/// Tree walker over a `SemanticPlan`. Implementations provide node
/// handlers; `PlanNode::walk_pre` / `walk_post` dispatch.
pub trait PlanVisitor {
    type Output;

    /// Called for each `PlanNode` encountered. The default
    /// implementation `()` for `Output = ()` descends children — a
    /// `walk_children` helper makes this one-liner-safe.
    fn visit(&mut self, node: &PlanNode) -> Self::Output;
}

/// Mutating variant for in-place analysis (e.g. schema enrichment).
pub trait PlanVisitorMut {
    type Output;
    fn visit_mut(&mut self, node: &mut PlanNode) -> Self::Output;
}
```

### 8.2 Walk / transform free functions

```rust
impl PlanNode {
    /// Pre-order traversal: visitor sees each node before its children.
    pub fn walk_pre<V: PlanVisitor>(&self, v: &mut V) -> V::Output;

    /// Post-order traversal: visitor sees each node after all children.
    pub fn walk_post<V: PlanVisitor>(&self, v: &mut V) -> V::Output;

    /// Bottom-up rewrite: each node is rewritten after its children.
    /// Propagates `Err` if the rewrite function fails on any node.
    pub fn transform<F>(self, f: F) -> Result<PlanNode, IrError>
    where F: FnMut(PlanNode) -> Result<PlanNode, IrError>;

    /// Iterator-style child access; used by generic tree algorithms.
    pub fn children(&self) -> impl Iterator<Item = &PlanNode>;
    pub fn children_mut(&mut self) -> impl Iterator<Item = &mut PlanNode>;
}
```

Wrapper-level delegation: `SemanticPlan::walk_pre` / `::walk_post` / `::transform` call the corresponding `PlanNode` method on `root`, returning any diagnostic the rewrite surfaces alongside the transformed tree.

### 8.3 `SemanticPlan::validate`

```rust
impl SemanticPlan {
    /// Full tree walk; re-checks every invariant in §7. Returns the
    /// first violation as an `IrError`; `Ok(())` on well-formedness.
    ///
    /// Intended use: debug / test harnesses, planner-regression
    /// catches, audit tools. Production callers rely on the planner's
    /// well-formedness guarantee and SHOULD NOT validate on every plan.
    pub fn validate(&self) -> Result<(), IrError>;
}
```

### 8.4 Typical usage patterns

**Count nodes of a variant.** Implement `PlanVisitor` with `Output = ()` and a counter field; let the default descend-children implementation do the work.

**Extract all `Scan`'s sources.** Implement `PlanVisitor` collecting `&SourceRef` from every `PlanNode::Scan(ScanNode { source, .. })`.

**Push-down rewrite.** Implement `transform` with a closure that matches `Filter { input: box Scan { .. }, predicate }` and rebuilds the subtree with the predicate in `filters_pushdown`.

**Schema re-check.** Implement `PlanVisitor<Output = Result<(), IrError>>` that recomputes `output_schema` for each variant and compares to `meta().output_schema`; return the first mismatch.

## 9. Serde / Substrait Mapping

### 9.1 Serde

Every public IR type derives `Serialize` / `Deserialize` under the crate-level `serde` feature flag (§11). `SemanticPlan` is the intended portable form: a serialized plan can be round-tripped across processes sharing the same Manifest. Wire-form stability rules:

- Every `#[non_exhaustive]` enum serializes with a `kind` discriminator field (serde-tagged). Adding a variant preserves round-trip of existing variants.
- Every `#[non_exhaustive]` struct serializes with absent-field-tolerant deserialization. Adding a field preserves round-trip of existing values (new field defaults to its `Default::default` or `None`).
- `PhysicalExpr` serializes through `semstrait-core`'s expression serde (`31 §4.5`). `PlanNode` is the only `semstrait-ir`-owned enum that requires non-trivial serde; everything else is a direct `#[derive(Serialize, Deserialize)]`.

### 9.2 Substrait mapping table

The adapter crate (`36`) owns the bidirectional conversion between `SemanticPlan` and `substrait::proto::Plan`. `35` ratifies the **mapping** so both the crate's tests (round-trip), `36`'s emitter, and `36`'s deserializer agree on which `substrait::proto::Rel` corresponds to which `PlanNode`.

| `PlanNode` variant | Substrait `Rel` kind           | Notes                                                                                                                                            |
|--------------------|--------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------|
| `Scan`             | `ReadRel`                      | `source` resolves to `ReadRel.read_type` via the adapter's Manifest lookup. `filters_pushdown` → `ReadRel.filter` (one conjunction).             |
| `Filter`           | `FilterRel`                    | `predicate` → `FilterRel.condition`.                                                                                                             |
| `Project`          | `ProjectRel`                   | `projections` → `ProjectRel.expressions` (order-preserving). Output names carried in `RelRoot.names` at the plan root.                           |
| `Agg`              | `AggregateRel`                 | `group_by` → one `Grouping` with the referenced columns as `grouping_expressions`. `aggregates` → `AggregateRel.measures`.                       |
| `Join`             | `JoinRel`                      | `join_type` → `JoinRel.type`. `on` → `JoinRel.expression` (equijoin with conjunction of `KeyPair` equalities). `cardinality` → `AdvancedExtension.enhancement` with URN `urn:semstrait:join-cardinality:v1`. |
| `Union`            | `SetRel` with `op = UNION` / `UNION_DISTINCT` | `distinct = false` → `SET_OP_UNION_ALL`; `distinct = true` → `SET_OP_UNION_DISTINCT`.                                               |
| `Sort`             | `SortRel`                      | `order[*].0` → `SortField.expr` (resolved to the referenced column). `order[*].1` → `SortField.direction`.                                       |
| `Fetch`            | `FetchRel`                     | `limit` / `offset` → `FetchRel.count` / `FetchRel.offset` (`-1` when `None`).                                                                    |

`SemAnnotation` on `NodeMeta.annotations` round-trips through Substrait's `AdvancedExtension.optimization` (URN `urn:semstrait:annotations:v1`) per `36 §4`.

The adapter is free to emit Substrait proto plans with extra hints (capacity, parallelism) in `AdvancedExtension.enhancement` slots; those hints are adapter-owned and not round-tripped through `35`.

### 9.3 YAML / JSON alternatives

`SemanticPlan` serialized via `serde_json` is the reference portable form for debugging / testing. There is no semstrait-specific YAML schema for `SemanticPlan` (YAML in semstrait is an authoring-layer concept per `14 §4`, not an IR-layer concept). Adapters producing plan text for logs / dumps SHOULD emit Substrait protobuf or `serde_json`.

## 10. Error Types

### 10.1 `IrError`

```rust
/// Typed error surface for `semstrait-ir`'s own operations: plan-tree
/// construction (`Name` validation), plan walking (`transform`
/// failures), plan validation (`validate`), and adapter-artifact
/// serialization (`EnginePlan::to_bytes` / `to_json`).
///
/// All `IrError` variants carry the subsystem prefix `IR_E` per
/// `30 §6.1` format. Range: `IR_E_3500`–`3599`. `30 §6.2` reserves
/// the `IR` subsystem prefix for this doc.
#[non_exhaustive]
pub enum IrError {
    /// IR_E_3500 — `Name::new` was called with an empty or invalid
    /// identifier.
    InvalidName        { supplied: String, reason: String },

    /// IR_E_3501 — `Name::new` was called with a reserved plan-tree
    /// prefix (§5.4.1).
    ReservedName       { supplied: String, prefix: String },

    /// IR_E_3502 — a `PhysicalExpr` reaching the plan tree lacks
    /// `inferred_type`. Only reported by `SemanticPlan::validate`.
    UnresolvedType     { location: String, expr_sketch: String },

    /// IR_E_3503 — two `KeyPair` columns have incompatible types on a
    /// `JoinNode`. Only reported by `SemanticPlan::validate`.
    JoinKeyTypeMismatch{ pair: KeyPair, left_ty:  semstrait_core::DataType,
                                        right_ty: semstrait_core::DataType },

    /// IR_E_3504 — `AggNode.aggregates` contains a duplicate output
    /// name. Only reported by `SemanticPlan::validate`.
    DuplicateAggName   { name: Name },

    /// IR_E_3505 — `FilterNode.predicate` has non-Boolean type. Only
    /// reported by `SemanticPlan::validate`.
    FilterPredicateNotBoolean { actual: semstrait_core::DataType },

    /// IR_E_3506 — a `PlanNode`'s `meta.output_schema` disagrees with
    /// the schema computed from its children. Only reported by
    /// `SemanticPlan::validate`.
    SchemaMismatch     { node_kind: &'static str, expected: String, got: String },

    /// IR_E_3507 — `UnionNode.inputs` schemas are not structurally
    /// compatible.
    UnionSchemaMismatch{ input_ix: usize, expected: String, got: String },

    /// IR_E_3508 — `UnionNode.inputs.len() < 2`.
    UnionArityTooLow   { arity: usize },

    /// IR_E_3509 — a `Name` referenced by `group_by` / `order` / `on`
    /// does not resolve to a column in the input schema.
    UnresolvedColumnRef{ name: Name, available: Vec<Name> },

    /// IR_E_3510 — a `FetchNode.limit` / `FetchNode.offset` value is
    /// out of the adapter's representable range (typically `i64::MAX`
    /// for Substrait).
    FetchValueOutOfRange { field: &'static str, value: u64 },

    /// IR_E_3511 — a `transform` / `walk` callback returned an error.
    /// Wraps the user-supplied error as a boxed `dyn Error`.
    TransformFailure   { reason: String },

    /// IR_E_3512 — `EnginePlan::to_bytes` / `to_json` failed; wraps
    /// the underlying `prost::EncodeError` / `serde_json::Error`
    /// context as a string.
    ArtifactSerializationFailed { reason: String },

    /// IR_E_3513 — a visitor-side invariant was violated (e.g. a
    /// transform produced a structurally invalid subtree that
    /// immediate post-check caught).
    TransformInvariantViolated  { reason: String },
}

impl IrError {
    pub fn code(&self) -> &'static str;           // IR_E_35xx per variant
    pub fn severity(&self) -> semstrait_core::Severity; // Always Error for v1
}

impl semstrait_core::IntoDiagnostic for IrError {
    fn into_diagnostic(self) -> semstrait_core::Diagnostic;
}

impl std::fmt::Display for IrError { /* per-variant messages */ }
impl std::error::Error for IrError {}
```

`IrError` is owned by `semstrait-ir`. It is not a re-export of `CompileError` or `PlannerError` — those typed enums have different production sites and different lifecycles. A planner-side failure producing a malformed `SemanticPlan` is a `PlannerError` (`34`); that same malformed plan caught by `SemanticPlan::validate()` on the consumer side becomes an `IrError`. Both codes are preserved on the diagnostic for auditability.

### 10.2 Code range registration

`30 §6.2` reserves `IR_E` for `semstrait-ir`. The v1 range is `IR_E_3500`–`IR_E_3599` (100 codes reserved; 14 in use at v1); `IR_W_35xx` is reserved but unused in v1 (no warnings exist today at `semstrait-ir` level). Registering `IR_W` entries in a future release is MINOR per `30 §6.3`.

The offset (`3500` rather than `0001`) deliberately aligns with this doc's number (`35`) — matching the convention used informally in engineering hallways, and keeping `IR_*` codes lexically distinct from `PLAN_*` / `ADAPT_*`. An amendment against `30 §6.2`'s table will land next to this doc's ratification (`[TD-IR-CODE-TABLE-AMEND]`).

### 10.3 Warning / Info posture

v1 has no `IR_W_*` / `IR_I_*` codes. Warnings surfaced by planner or optimizer are `PLAN_W_*` / `OPT_W_*` per `30 §6.2`; adapter warnings are `ADAPT_W_*`. `semstrait-ir` itself, being pure data + validation, has no warning-emitting operation.

## 11. Stability

### 11.1 Stable parts

- **`PlanNode` variant set growth is non-breaking (I10).** Adding a variant (e.g. a future `Distinct`, `Window`, `Unnest`, `TopN`) is MINOR. Consumers that pattern-match exhaustively on `PlanNode` will compile-error by design — the `#[non_exhaustive]` attribute forces them to add a fallback arm.
- **Struct field addition inside a `PlanNode` variant is non-breaking.** Every variant's struct is `#[non_exhaustive]` per §4; adding a new field with a sensible default (`None`, `Vec::new()`, `0`, `false`) is MINOR. Examples: `JoinNode.residual: Option<PhysicalExpr>` for non-equi joins; `ScanNode.order_hint: Option<Vec<(Name, SortDir)>>` for order-preserving scans; `AggregateExpr.filter` (already reserved, §5.7).
- **`DialectId` const additions are non-breaking.** Adding a new `pub const` on `DialectId` is MINOR.
- **`SemAnnotation` variant additions are non-breaking** (annotation roster growth is expected as `34` matures).
- **Error-code additions in the `IR_E_35xx` reserved range are non-breaking** per `30 §6.3`.
- **Substrait mapping table entries are non-breaking** — adding a new `PlanNode` variant with a corresponding Substrait `Rel` kind is MINOR; changing an existing mapping is MAJOR.

### 11.2 Internal parts

- **`NodeMeta.node_id` values** are not stable across planner invocations. Consumers relying on stable identity across runs should derive identity from the plan-tree shape (e.g. a tree-hash visitor), not from `node_id`.
- **`SemanticPlan::validate()`'s error-ordering** is not stable. The first violation reported may shift between releases as `validate` reorders its checks for performance; consumers SHOULD treat any `IrError` as a single bad-plan signal, not a "first problem is X" guarantee.
- **Serde's on-wire shape under `#[non_exhaustive]` enums** follows the serde-tagged convention (§9.1). The exact JSON spelling of a `kind` discriminator is stable across MINOR releases; deserializers MUST be tolerant to unknown variant tags (typically mapping unknowns to a skipped-node error rather than panicking).

### 11.3 Delta with current code

The `crates/semstrait-ir/src/plan/node.rs` definitions exist today as `LogicalPlan` + `PlanNode` per `[TD-IR-RENAME]`. Target state:
- `LogicalPlan` → rename to `SemanticPlan` (matches `00 §4.1` vocabulary).
- Local `JoinType` / `SortDirection` enums → drop in favor of canonical `16 §5.2` `JoinType` re-exported through `semstrait-core` and the `SortDir` + `NullOrdering` types from §5.6.
- Every `pub enum` in `plan/` → add `#[non_exhaustive]`.
- `ScanNode.location` / `ScanNode.format` → drop in favor of the opaque `SourceRef` (§5.2); path + format resolution moves to `36`.
- Add `filters_pushdown: Vec<PhysicalExpr>` to `ScanNode`.

Migration items are tracked in `implementation/40_refactor_plan.md` under `[TD-IR-RENAME]` and `[TD-IR-NONEXHAUSTIVE]`.

## 12. Crate Boundaries

### 12.1 What `semstrait-ir` does NOT do

- **No planning.** `semstrait-ir` contains no `fn plan(manifest, request) -> SemanticPlan`. Planning logic (strategy dispatch, per-DataKind expansion, Relationship-graph traversal, constraint checking) all live in `semstrait-planner` per `34`.
- **No optimization.** `semstrait-ir` contains no `fn optimize(plan) -> SemanticPlan`. Canonical optimizer passes (constant folding, predicate pushdown, metadata-dimension substitution) live in `semstrait-planner` per `34 §5`.
- **No emission.** `semstrait-ir` contains no `fn adapt(plan) -> EngineArtifact`. Adapter emission (SQL rendering, Substrait proto building, capability checking) lives in `semstrait-adapter` per `36`.
- **No I/O.** No `std::fs`, no `reqwest`, no `tokio`. Every method on every public type is synchronous and pure. I11 guarantee.
- **No engine identity.** No adapter-specific logic inside `PlanNode` variants. `Scan` carries `SourceRef` (opaque); `Join` carries `JoinType` (canonical, not engine-specific); `Filter.predicate` is `PhysicalExpr` (canonical, not SQL text). I1 / I3 guarantees.
- **No Manifest construction.** `semstrait-ir` consumes `SourceRef`s that reference an external Manifest but never constructs one. Manifest construction is `semstrait-manifest`'s responsibility per `33`.

### 12.2 Dependency posture

```toml
[dependencies]
semstrait-core = { path = "../semstrait-core" }
thiserror      = "^"
prost          = "^"
substrait      = "^"   # substrait::proto::Plan for EnginePlan::Substrait
uuid           = { version = "^", features = ["v4"] }

[dependencies.serde]
version  = "^"
optional = true
features = ["derive"]

[features]
default = []
serde   = ["dep:serde", "semstrait-core/serde"]
```

**No runtime-only dependencies.** No `tokio`, `async-trait`, `futures`, `reqwest`, `hyper`, `sqlx`.
**No engine dependencies.** `substrait` (a proto codegen crate) is permitted because `EnginePlan::Substrait(Box<substrait::proto::Plan>)` is structural, not engine-identity. No `datafusion`, no `arrow`, no `duckdb`, no `spark-*`.
**No in-workspace dependencies beyond `semstrait-core`.** CI-enforced manifest audit per `30 §9`.

## 13. Invariants Upheld by the Crate

| Invariant | `semstrait-ir` guarantee |
|---|---|
| **I1** — no raw SQL in canonical layer | `PlanNode` variants carry `PhysicalExpr` for every predicate; `Name` for every column / key identifier; `SourceRef` (opaque) for every source. No `String`-as-SQL field exists on any `PlanNode`. `SqlArtifact.text` exists, but it is an *adapter output*, not a *plan content*. |
| **I2** — physical types belong to adapters | `SemanticPlan` references only `semstrait_core::DataType` (logical). `EnginePlan::Substrait(Box<substrait::proto::Plan>)` carries engine-specific types, but it is an *adapter output*, not an input to or content of a `SemanticPlan`. |
| **I3** — no engine-identity branching in canonical types | `PlanNode` has zero variants keyed by adapter / dialect. The only engine-identity value anywhere in `semstrait-ir` is `DialectId`, and it appears only on `SqlArtifact` (adapter output) / `Dialect::ID` (adapter-trait associated constant). |
| **I6** — plan hot path is synchronous | **No `pub async fn` exists on `semstrait-ir`.** Every method on every public type is synchronous. CI lint + `forbid_async_fn!` macro audit guard the crate. |
| **I7** — strict DAG | `Cargo.toml` lists `semstrait-core` as the only internal workspace dependency. CI check greps for any other `semstrait-*` entry. |
| **I10** — extensibility | Every `pub enum` and `pub struct` carries `#[non_exhaustive]` except the newtype-over-stable set: `Name`, `SourceRef`, `DialectId`, `NodeId`. An `integration-test` over `cargo public-api` enforces the rule. |
| **I11** — no downward I/O surprises | No `std::fs`, no `std::net`, no `tokio`, no `reqwest` anywhere in the crate. `substrait`'s `prost` dependency is bytes-encoding only, not I/O. |
| **I12** — first-class diagnostics | `IntoDiagnostic` implemented on `IrError`; every variant maps to a stable `IR_E_35xx` code. No `Display` output reaches API consumers without passing through `IntoDiagnostic`. |

## 14. Public API Surface Sketch

### 14.1 `plan`

```
pub struct SemanticPlan                                  // root; { root, output_names, diagnostics }
pub enum   PlanNode                                      // 8 variants per §4
pub struct ScanNode                                      // §4.2
pub struct FilterNode                                    // §4.3
pub struct ProjectNode                                   // §4.4
pub struct AggNode                                       // §4.5
pub struct JoinNode                                      // §4.6
pub struct UnionNode                                     // §4.7
pub struct SortNode                                      // §4.8
pub struct FetchNode                                     // §4.9
pub struct NodeMeta                                      // §5.1
pub struct NodeId                                        // newtype over Uuid
pub struct Schema                                        // plan-level schema; { fields }
pub struct Field                                         // { name, data_type, nullable }
pub enum   SemAnnotation                                 // #[non_exhaustive]; AggregateRole, FilterSource, ...
```

### 14.2 `plan::traversal`

```
pub trait  PlanVisitor                                   // visit(&PlanNode) -> Self::Output
pub trait  PlanVisitorMut                                // visit_mut(&mut PlanNode) -> Self::Output
```

### 14.3 `primitives`

```
pub struct SourceRef                                     // opaque handle; §5.2
pub struct ResolvedColumn                                // §5.3
pub struct Name                                          // newtype over String; §5.4
pub struct KeyPair                                       // §5.5
pub enum   SortDir                                       // Asc | Desc with NullOrdering; §5.6
pub enum   NullOrdering                                  // First | Last | Unspecified
pub struct AggregateExpr                                 // §5.7
pub use    semstrait_core::{Cardinality, JoinType}       // re-exported from 16 §5 per `authoritative-for`
```

### 14.4 `artifact`

```
pub enum   EngineArtifact                                // Sql | Plan
pub enum   EnginePlan                                    // Substrait
pub struct SqlArtifact                                   // { text, dialect }
pub struct DialectId                                     // newtype; ANSI | DATAFUSION | DUCKDB | SPARK
pub trait  Dialect                                       // ID const + capabilities()
pub enum   Capability                                    // #[non_exhaustive]; roster owned by 36
```

### 14.5 `error`

```
pub enum   IrError                                       // 14 variants in v1; IR_E_3500–3513
impl       IntoDiagnostic for IrError
impl       std::error::Error for IrError
```

### 14.6 Free functions / inherent impl methods at crate root

```
impl PlanNode {
    pub fn meta(&self) -> &NodeMeta;
    pub fn meta_mut(&mut self) -> &mut NodeMeta;
    pub fn children(&self) -> impl Iterator<Item = &PlanNode>;
    pub fn children_mut(&mut self) -> impl Iterator<Item = &mut PlanNode>;
    pub fn walk_pre<V: PlanVisitor>(&self, v: &mut V) -> V::Output;
    pub fn walk_post<V: PlanVisitor>(&self, v: &mut V) -> V::Output;
    pub fn transform<F>(self, f: F) -> Result<PlanNode, IrError>
    where F: FnMut(PlanNode) -> Result<PlanNode, IrError>;
}

impl SemanticPlan {
    pub fn validate(&self) -> Result<(), IrError>;
    pub fn walk_pre<V: PlanVisitor>(&self, v: &mut V) -> V::Output;
    pub fn walk_post<V: PlanVisitor>(&self, v: &mut V) -> V::Output;
    pub fn transform<F>(self, f: F) -> Result<SemanticPlan, IrError>
    where F: FnMut(PlanNode) -> Result<PlanNode, IrError>;
}

impl EnginePlan {
    pub fn to_bytes(&self) -> Result<Vec<u8>, IrError>;
    pub fn to_json(&self) -> Result<String, IrError>;
}
```

### 14.7 Crate-root re-exports

```rust
// lib.rs
pub use crate::plan::{
    SemanticPlan, PlanNode, ScanNode, FilterNode, ProjectNode, AggNode,
    JoinNode, UnionNode, SortNode, FetchNode,
    NodeMeta, NodeId, Schema, Field, SemAnnotation,
    traversal::{PlanVisitor, PlanVisitorMut},
};
pub use crate::primitives::{
    SourceRef, ResolvedColumn, Name, KeyPair, SortDir, NullOrdering,
    AggregateExpr,
};
pub use crate::artifact::{
    EngineArtifact, EnginePlan, SqlArtifact, DialectId, Dialect, Capability,
};
pub use crate::error::IrError;

// Re-exports from semstrait-core that `35`-authoritative surfaces rely on:
pub use semstrait_core::{Cardinality, JoinType};
```

## 15. Ratified Decisions Index

`35` introduces no new expression vocabulary and no new type vocabulary — every type above is ratified upstream in `13`, `14`, `14a`, or `16`. The ratifications below concern **plan-tree shape**, **visibility**, and **boundary** decisions unique to `semstrait-ir` as a crate:

| # | Decision | Rationale | § |
|---|---|---|---|
| R1 | `SemanticPlan` is the crate-owned top-level type; `LogicalPlan` is the current code name and is renamed as part of `[TD-IR-RENAME]` | Matches `00 §4.1` canonical vocabulary. Rename is a MINOR via type-alias transition (`pub type LogicalPlan = SemanticPlan;` retained one MINOR cycle). | §3.1 |
| R2 | `PlanNode` has exactly 8 variants in v1: `Scan`, `Filter`, `Project`, `Agg`, `Join`, `Union`, `Sort`, `Fetch` | Matches the engine-IR structural inspiration (`00 §3` / `35 §1.4`). Distinct, Window, Unnest, TopN are deferred as non-breaking additions. | §4.1 |
| R3 | Every `PlanNode` variant's inner struct is `#[non_exhaustive]` | I10 — field additions inside a variant are MINOR. | §4 |
| R4 | `ScanNode` carries `SourceRef` (opaque handle) + `Vec<ResolvedColumn>` + `Vec<PhysicalExpr>` for pushdown. No path, URL, or format string | I1 / I3 — engine identity and path resolution live in the adapter. Adapters consult the Manifest via `SourceRef`. | §4.2 |
| R5 | `Expr::Aggregate` is NOT carried inside `PhysicalExpr` on any plan-level surface; aggregates live on `AggregateExpr` on `AggNode.aggregates` | Matches `14 §2.3` / `31 §3.3` `PhysicalExpr` wrapper invariants. `AggregateExpr` is a plan-level wrapper carrying the same shape. | §5.7 |
| R6 | `JoinNode.on: Vec<KeyPair>` with type-reconciled columns; non-equi predicates deferred as `[TD-IR-NON-EQUI-JOIN]` | v1 covers the common case of equijoin over reconciled types. Non-equi is a MINOR addition via `JoinNode.residual: Option<PhysicalExpr>`. | §4.6 |
| R7 | `JoinNode.cardinality` is required (not `Option<Cardinality>`) and reflects the Relationship graph | Cardinality is always known at plan time; absent cardinality on a Join is a planner bug, not a legitimate state. | §4.6 |
| R8 | `UnionNode.inputs: Vec<PlanNode>` (n-ary), with `distinct: bool` for `UNION ALL` vs `UNION DISTINCT` | N-ary matches SQL / Substrait `SetRel` shape; avoids right-leaning binary trees for multi-union common case. | §4.7 |
| R9 | `SortDir` bundles direction + null-ordering; `NullOrdering::Unspecified` defers to the adapter | Keeps `SortNode.order[*]` a single `(Name, SortDir)` tuple; matches Substrait's `SortField.direction`. | §5.6 |
| R10 | `FetchNode.limit: Option<u64>` and `FetchNode.offset: Option<u64>`; `None` means "no limit / no offset" | `Option` cleanly distinguishes unset from zero; `u64` prevents negative. Values exceeding `i64::MAX` raise `IR_E_3510` at adapter-emit time. | §4.9 |
| R11 | `DialectId` is a newtype over `&'static str` with `pub const` identities; third-party adapters may extend via the `Dialect` trait | Matches `CanonicalFn`'s newtype-over-stable posture in `semstrait-core` (`31 §5.1`). Extensible without sealing. | §6.4 |
| R12 | `EngineArtifact = Sql(SqlArtifact) \| Plan(EnginePlan)`; `EnginePlan::Substrait(Box<substrait::proto::Plan>)` is the sole plan form in v1 | Matches `00 §4.1` vocabulary. Future structured-IR emissions (e.g. DataFusion-Logical direct) are MINOR `EnginePlan` variants. | §6 |
| R13 | `NodeMeta.output_schema: Arc<Schema>` for cheap pass-through schema sharing | Filter, Sort, Fetch share their input's schema without deep clone; measured memory + allocation win over owned schemas. | §5.1 |
| R14 | `IrError` is a distinct enum from `CompileError` / `PlannerError`; its production sites are `semstrait-ir`-internal (plan construction + validation + artifact serialization) | Keeps the crate boundary clean; `semstrait-ir` does not re-export upstream error variants. | §10.1 |
| R15 | `IR_E_35xx` reserved range in `30 §6.2`; 14 codes in use at v1 (`3500`–`3513`) | `30 §6.2`'s table amendment tracked as `[TD-IR-CODE-TABLE-AMEND]`. | §10.2 |
| R16 | `Name` is a validating newtype over `String`; rejects empty and reserved-prefix identifiers | Identifier well-formedness is checked at the plan boundary, not re-checked on every adapter call. | §5.4 |
| R17 | Serde is a feature-gated opt-in (`serde` feature); `PlanNode` uses serde-tagged `kind` discriminator to survive variant additions | Matches `semstrait-core`'s posture (`31 §11`); enables portable `SemanticPlan` exchange across processes. | §9.1 |
| R18 | Substrait mapping lives as a **table** in `35 §9.2`; conversion code lives in `36` | Keeps `semstrait-ir` free of `substrait::proto` conversion logic (complexity belongs to the adapter); keeps the mapping in one ratified place. | §9.2 |

---

## 16. Round-1 Open Items

See `/docs/design/open_questions/35_open_questions.md` for the parked questions surfaced during Round-1 drafting. Round-1 defaults above ratify the plan-tree shape enough for `34` / `36` to draft against; the open-questions file records the items that will revisit once downstream drafts push back.

---

*Cross-references in this document are by section (e.g. `14 §3.2`, `16 §5.1`, `30 §6.2`). No code-path references are used, per `00 §8`.*
