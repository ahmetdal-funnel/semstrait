---
prereqs: [10, 11, 13, 14, 14b, 15, 16, 17, 20, 21, 22, 23, 24, 25, 30, 31, 33, 35]
authoritative-for:
  - the `semstrait-planner` public-API surface (types, traits, free functions)
  - the `plan` free-function signature (`plan(&Manifest, Request) -> Result<SemanticPlan, PlanErrors>`) and its seven pipeline sub-steps
  - the `optimize` free-function signature (`optimize(SemanticPlan) -> Result<SemanticPlan, OptimizeErrors>`) and the canonical-pass roster
  - the `Request` type and its fields (dimensions, measures, metrics, filters, order, limit, offset, from, temporal, session)
  - the `SessionContext` type and its fields (now, timezone, feature_toggles, correlation_id)
  - the `ResolvedQueryRequest` type — Manifest-contextualized Request with looked-up Semantics and resolved target DataKind
  - the `Strategy` trait surface (`id`, `supports`, `plan`) and its companion types (`StrategyId`, `StrategyContext`)
  - the `OptimizerPass` trait surface and its canonical v1 passes (constant folding, metadata-Dimension substitution, predicate simplification, identity-Project elimination)
  - the `PlanError` / `PlanErrors` / `OptimizeError` / `OptimizeErrors` typed carriers and their `PLAN_E_*` / `OPT_E_*` code ranges
  - fail-fast diagnostics accumulation for `plan` and `optimize` (non-error diagnostics carried through both arms per `30 §7`)
  - crate boundary posture — no I/O, no SQL emission, no YAML parsing, no catalog access on the `plan` / `optimize` hot path
refined-by:
  - 21 (`data-kinds/21_dataset.md` — `SimpleStrategy` per-variant algorithm)
  - 22 (`data-kinds/22_grainset.md` — `GrainsetStrategy` per-variant algorithm)
  - 23 (`data-kinds/23_unionset.md` — `UnionsetStrategy` per-variant algorithm)
  - 24 (`data-kinds/24_joinset.md` — `JoinsetStrategy` per-variant algorithm)
  - 25 (`data-kinds/25_applicability_matrix.md` — per-variant cross-cut consumed at dispatch)
  - 35 (`apis/35_semstrait_ir.md` — `SemanticPlan` and `PlanNode` tree the planner emits)
  - 36 (`apis/36_semstrait_adapter.md` — consumes the `SemanticPlan` produced by this crate)
  - 40 (`implementation/40_refactor_plan.md` — current code-vs-target delta for the planner crate)
---

# 34. semstrait-planner

> **Status:** ratified (Round 1). `34` nails down the public surface of
> `semstrait-planner` — the crate that owns the `plan` stage (`10 §3.4`)
> and the `optimize` stage (`10 §3.5`) — against `20`'s Strategy-per-
> variant taxonomy, `21`–`24`'s per-variant strategy bodies, `33`'s
> Manifest consumer contract, `35`'s `SemanticPlan` shape, and `30`'s
> stability / diagnostics policy. All types the surface touches are
> ratified upstream; `34` adds the crate-level wiring — the `plan` /
> `optimize` entry points, the `Request` / `SessionContext` /
> `ResolvedQueryRequest` value objects, the `Strategy` and
> `OptimizerPass` trait shapes, and the `PLAN_E_*` / `OPT_E_*` error
> enums that flow across the stage boundary. Round-1 open items parked
> in `questions/open/34_questions.md`.

## 1. Purpose, scope, layering

### 1.1 What `semstrait-planner` OWNS

- The `plan` free function (§6) that turns a `&Manifest` + a `Request` into a `SemanticPlan`.
- The `optimize` free function (§11) that takes a `SemanticPlan` and returns an equivalent, canonicalized `SemanticPlan`.
- The `Request` (§3), `SessionContext` (§4), and `ResolvedQueryRequest` (§5) value types.
- The `Strategy` trait (§8) — the dispatch surface ratified structurally in `20 §5.2` and concretized here.
- The four built-in strategies: `SimpleStrategy`, `GrainsetStrategy`, `UnionsetStrategy`, `JoinsetStrategy` (§9) — crate-public wrappers whose algorithms are ratified in `21`–`24`.
- Field-first resolution (§10) — the planner-side realization of the algorithm ratified in `16 §11`.
- The `OptimizerPass` trait (§12) — the pluggable v1 optimizer interface.
- The `PlanError` / `PlanErrors` / `OptimizeError` / `OptimizeErrors` typed carriers (§13) and their stable `PLAN_E_*` / `OPT_E_*` codes.
- The `StrategyRegistry` and `StrategyContext` (§8.3 / §8.4) — internal wiring types exposed to adapter-level extensions and test doubles only.

### 1.2 What `semstrait-planner` does NOT own

- **Expression / type vocabulary.** `Expr`, `PhysicalExpr`, `Aggregation`, `DataType`, `Grain`, `Diagnostic`, `Severity` all live in `semstrait-core` (`31`). `34` consumes them.
- **Plan-tree shape.** `SemanticPlan`, `PlanNode`, `NodeMeta`, `SourceRef`, `Name` all live in `semstrait-ir` (`35`). `34` emits and consumes them.
- **Manifest shape.** `Manifest`, `ResolvedDataKind`, `ResolvedBinding`, `ResolvedExprTable`, `CoverageIndex`, `CompositionIndex` all live in `semstrait-manifest` (`33`). `34` reads them — never mutates, never re-resolves (I5 / I8).
- **YAML parsing / structural validation.** Lives in `semstrait-model` (`32`). The `Request` the planner accepts is already a Rust value — the API layer (`semstrait-api`) is responsible for converting user-facing JSON / gRPC / protobuf into `Request`.
- **Catalog / filesystem access.** `CatalogProvider` / `FileSystem` (`37`) are consumed at `compile` time only; no planner surface accepts them (I11).
- **SQL / engine emission.** `semstrait-adapter` (`36`) consumes the `SemanticPlan` and produces engine-specific artifacts; no planner API touches a dialect.

### 1.3 Design posture — sync-only dispatch crate

The planner is the workspace's widest dispatch site — the `plan` entry point fans out across every `ResolvedDataKind` variant — but it is **not** the crate with the most runtime weight. The hot path is a single tree walk over pre-resolved Manifest indices (§7). Three properties shape the design:

- **Synchronous end-to-end.** Per I6, there is no `async fn` anywhere on the public surface. `plan` and `optimize` are ordinary fallible functions; every strategy method is sync. The `async` wrapper on `compile` (`10 §3.3`) is the last async boundary in the pipeline; everything the planner reads has already been resolved.
- **Pure transformations over pre-built indices.** Per I5 / I8, the planner does no name resolution, no catalog fetch, no expression recompilation. It performs O(1) / O(log n) index lookups on the `Manifest` (`14b §2.3`, `33 §3.4`) and emits `PlanNode`s carrying pre-resolved `PhysicalExpr`s.
- **Strategy dispatch is the one variant-match site.** Per `20 §5.3`, the only place the planner branches on a `ResolvedDataKind` variant tag is the dispatch function (§8.5). Every other planner site consumes `&dyn Strategy`. Adding a new `Complex` variant per I10 is a MINOR change that forces a new match arm in one place, not a scatter of edits.

### 1.4 Invariants upheld by the crate

| Invariant | `semstrait-planner` guarantee |
|---|---|
| **I5** — name resolution is compile-time | The planner performs **lookup only** (`14b §2.3`, `33 §3.4`). No `resolve_*` method walks names to Semantics or columns; every such walk was performed at `compile`. A CI lint forbids `EntityRef` leaves in any `PhysicalExpr` a plan node carries. |
| **I6** — plan hot path is synchronous | **No `pub async fn` exists on `semstrait-planner`.** The `Strategy` trait's `plan` method is sync; so is every `OptimizerPass::apply`. A CI audit (`cargo clippy -- -D clippy::async_fn_in_trait`) enforces. |
| **I8** — planner-complete Manifest | Every `(SemanticsName, BindingId)` the planner might ask for is already in `manifest.expr_table` (`14b §2.3`). Every `Relationship` the planner walks is in `manifest.resolved_relationships`. A Manifest whose indices fail this completeness guarantee triggers `PLAN_E_2052 ManifestIndexInconsistent` at dispatch (`20 §8.2`). |
| **I10** — non-exhaustive public sum types | `Request`, `SessionContext`, `ResolvedQueryRequest`, `PlanError`, `PlanErrors`, `OptimizeError`, `OptimizeErrors`, `StrategyId` are all `#[non_exhaustive]`. An integration test over `cargo public-api` enforces. |
| **I11** — no I/O in hot path | No `std::fs`, no `std::net`, no `tokio`, no `reqwest` in the crate's dependency graph. The `Cargo.toml` audit (§16.2) is CI-enforced. |
| **I12** — first-class diagnostics | Every `PlanError` / `OptimizeError` variant has a stable `PLAN_E_*` / `OPT_E_*` code (§13); each converts to `Diagnostic` via `IntoDiagnostic` (`31 §7.4`). Non-error diagnostics accumulate through both success and failure arms per `30 §7`. |

### 1.5 Constraint validation precedes Strategy dispatch

Per `11 §8.6`, realized-carrier Constraint validation runs as the **planner's first action — step 0, pre-resolution — before any other sub-step of the `plan` pipeline**. The rationale is twofold:

1. Constraints express author-declared admissibility rules on Request shape (`11 §8.4`). A Request that violates a Measure's `dimensions: { one_of: [...] }` rule cannot be planned without lying about author intent; it is cheaper and clearer to reject before any Manifest index work begins.
2. The v1 `ConstraintValidator::check()` (`11 §8.6`) reads only the Request and the Manifest's name indices — work the planner must do anyway in step 1. Reordering would duplicate lookups.

§7.1 ratifies step 0 as the planner's entry action. The seven-step pipeline is fixed: violating the order is a crate-internal bug, not a surface the caller can influence.

### 1.6 The `optimize` stage — when and why

`optimize` is **stage 5** of the canonical pipeline (`10 §3.5`), immediately after `plan`. Like `plan` it is sync / pure / fail-fast; unlike `plan` it is **optional** per `10 §3.5`'s contract — a caller that does not need canonical-form guarantees MAY skip it and pass the raw `SemanticPlan` straight to an adapter. Concrete consumers (e.g. the `semstrait-api` request pipeline) invoke it unconditionally.

The v1 optimizer is deliberately minimal (§11.2): four canonical rewrite passes with simple, well-specified semantics. The pass roster grows under I10 as downstream performance evidence accumulates; new passes register via the `OptimizerPass` trait (§12).

## 2. Public crate surface

Every `pub` symbol below carries a doc comment, is listed in this document, and has documented invariants. Crate-internal helpers (`DataKindPlannerRegistry`, `PlanFragment`, `PrunedView`, etc. from the legacy code) are `pub(crate)` and are not part of `34`'s surface. The target module layout is:

```
semstrait-planner
├── request              // Request, SessionContext, ResolvedQueryRequest,
│                        //   Filter, TemporalRequest (forward-ref to 17)
├── plan                 // pub fn plan, PlanError, PlanErrors, plan-pipeline internals
├── optimize             // pub fn optimize, OptimizerPass, OptimizeError,
│                        //   OptimizeErrors, canonical v1 passes
├── strategy             // Strategy trait, StrategyId, StrategyContext,
│                        //   StrategyRegistry, dispatch_strategy
├── strategies           // SimpleStrategy, GrainsetStrategy, UnionsetStrategy,
│                        //   JoinsetStrategy
└── resolution           // field-first resolution entry point; Request-lookup
                         //   helpers; relationship-traversal wrappers
```

Crate-root re-exports (§17.1) expose the stable convenience surface. Non-root re-exports are forbidden — consumers either import `semstrait_planner::plan` or `semstrait_planner::plan::plan`, never both.

**Surface roster (one line per item; full shapes in later sections):**

| Module | Item | Kind | Section |
|---|---|---|---|
| (crate root) | `pub fn plan(&Manifest, Request) -> Result<SemanticPlan, PlanErrors>` | free fn | §6 |
| (crate root) | `pub fn optimize(SemanticPlan) -> Result<SemanticPlan, OptimizeErrors>` | free fn | §11 |
| `request` | `pub struct Request` | value type | §3 |
| `request` | `pub struct SessionContext` | value type | §4 |
| `request` | `pub struct ResolvedQueryRequest` | value type | §5 |
| `request` | `pub struct Filter` | value type | §3.5 |
| `request` | `pub enum FilterOperator` | sum type | §3.5 |
| `request` | `pub enum FilterValue` | sum type | §3.5 |
| `request` | `pub enum SortDir` | sum type | §3.6 |
| `request` | `pub struct DataKindRef` | newtype | §3.8 |
| `request` | `pub struct TemporalRequest` | value type (reserved) | §3.9 |
| `plan` | `pub enum PlanError` | typed error | §13 |
| `plan` | `pub struct PlanErrors` | error carrier | §13.2 |
| `optimize` | `pub enum OptimizeError` | typed error | §13.4 |
| `optimize` | `pub struct OptimizeErrors` | error carrier | §13.4 |
| `optimize` | `pub trait OptimizerPass` | pluggable pass | §12 |
| `optimize` | `pub struct Optimizer` | pass chain | §12.4 |
| `optimize` | `pub struct OptimizerBuilder` | builder | §12.5 |
| `strategy` | `pub trait Strategy` | dispatch surface | §8 |
| `strategy` | `pub struct StrategyId` | newtype | §8.2 |
| `strategy` | `pub struct StrategyContext<'a>` | per-invocation context | §8.4 |
| `strategy` | `pub struct StrategyRegistry` | dispatch table | §8.3 |
| `strategy` | `pub fn dispatch_strategy(...) -> &dyn Strategy` | free fn | §8.5 |
| `strategies` | `pub struct SimpleStrategy` | v1 strategy | §9.1 |
| `strategies` | `pub struct GrainsetStrategy` | v1 strategy | §9.2 |
| `strategies` | `pub struct UnionsetStrategy` | v1 strategy | §9.3 |
| `strategies` | `pub struct JoinsetStrategy` | v1 strategy | §9.4 |

## 3. The `Request` type

### 3.1 Shape

```rust
/// A caller-authored query intent — the planner's input at the
/// stage-4 boundary (`10 §3.4`). Per `00 §4.1`'s Request row. Carries
/// no references to the Manifest; the API layer constructs it from
/// the user-facing query form.
#[non_exhaustive]
pub struct Request {
    pub dimensions: Vec<SemanticsName>,
    pub measures:   Vec<SemanticsName>,
    pub metrics:    Vec<SemanticsName>,
    pub filters:    Vec<Filter>,
    pub order:      Vec<(Name, SortDir)>,
    pub limit:      Option<u64>,
    pub offset:     Option<u64>,
    pub from:       Option<DataKindRef>,
    pub temporal:   Option<TemporalRequest>,
    pub session:    SessionContext,
}
```

Each field is detailed in §3.2–§3.7. `#[non_exhaustive]` per I10: additions (e.g. a reserved `explain:` block for plan-tree introspection) are MINOR per `30 §2.2`.

### 3.2 `dimensions`, `measures`, `metrics`

Each field is a `Vec<SemanticsName>`. Order is user-visible: it pins the left-to-right column order in `SemanticPlan.output_names` (`35 §3.2`). Duplicates within any single list are rejected at step 1 with `PLAN_E_0510 DuplicateRequestedName`; empty lists on all three simultaneously are rejected with `PLAN_E_0511 EmptyRequest`.

The split into three vectors mirrors `00 §4.1`'s terminology. At the planner layer the distinction is structural, not semantic: every entry becomes a leaf lookup against the Manifest's name index. The split matters downstream — a Measure wraps inside a `PlanNode::Agg`; a Dimension lands in `group_by`; a Metric routes through its `CompiledMetric` entry per `14b §2.4`.

**Type admissibility.** If a name's resolved Semantics type contradicts the field it was placed in (e.g. a Dimension name in `measures`), the planner raises `PLAN_E_0512 RequestFieldTypeMismatch` at step 1.

### 3.3 `filters`

`Vec<Filter>` — a flat list of row-level predicates conjoined by logical AND. Each `Filter` names a Semantics by `SemanticsName`, an operator, and a value vector (§3.5). Filter decomposition and placement across scan / rename / expression / aggregation layers is the planner's job (§7.6, `21 §4.6`); the caller sees only the conjunctive list.

Filters on a Measure or two-stage Metric with `agg:` land above `PlanNode::Agg` (HAVING-equivalent). Filters on a Dimension sink below aggregation. Filters on a declared Filter Semantics (`11 §6`) resolve through the Semantics' body. `filters: Vec::new()` is legal; default filters carried on the `ResolvedDataKind` (e.g. Filter-injection under reserved carrier `11 §8.5.2`'s `requires:`) are applied separately.

### 3.4 `order`, `limit`, `offset`

- `order: Vec<(Name, SortDir)>` — ORDER BY list. Names MUST appear in `dimensions` ∪ `measures` ∪ `metrics`; violation raises `PLAN_E_0513 OrderByUnknownName`.
- `limit: Option<u64>` — row cap on the post-sort output. `Some(0)` emits an empty-result `PlanNode::Fetch`.
- `offset: Option<u64>` — row offset applied before `limit`.

Placement: `order` → `PlanNode::Sort`; `offset` / `limit` → `PlanNode::Fetch`. Empty `order` with non-empty `limit` is legal; result order is non-deterministic (`35 §4.8`).

### 3.5 `Filter`

```rust
/// A single user-supplied predicate. Conjoined with siblings via AND.
#[non_exhaustive]
pub struct Filter {
    /// The Semantics name being filtered. MUST resolve on the target
    /// `ResolvedDataKind`'s interface (Simple `SemanticInterface` or
    /// Complex `ComposedSemanticInterface`).
    pub field: SemanticsName,

    /// The operator applied between `field` and `values`.
    pub operator: FilterOperator,

    /// The operand(s) the operator consumes. Each operator has a
    /// per-variant arity rule enforced at step 1 (§7.2).
    pub values: Vec<FilterValue>,
}

#[non_exhaustive]
pub enum FilterOperator {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    In,
    NotIn,
    Between,
    IsNull,
    IsNotNull,
}

/// Typed filter operand. The planner lowers every variant to an
/// `Expr::Literal` leaf with the appropriate `LiteralValue` variant
/// during step 5 (§7.5).
#[non_exhaustive]
pub enum FilterValue {
    String(String),
    Number(f64),
    Integer(i64),
    Decimal(String),   // string form; parsed against the target's DataType
    Bool(bool),
    Timestamp(Timestamp),
    Date(Date),
    Null,
}
```

**Arity rules** (enforced at step 1):

| Operator | Arity | Error |
|---|---|---|
| `Eq` / `NotEq` / `Lt` / `LtEq` / `Gt` / `GtEq` | 1 | `PLAN_E_0520 FilterArityMismatch` |
| `In` / `NotIn` | ≥ 1 | same |
| `Between` | 2 (lower, upper) | same |
| `IsNull` / `IsNotNull` | 0 | same |

**Type admissibility.** A `FilterValue` whose native Rust type cannot be cast to `field`'s resolved `DataType` raises `PLAN_E_0521 FilterValueTypeMismatch`. Conversions follow `13 §4`'s compatibility relation; no engine-specific coercion is performed (I2).

### 3.6 `SortDir`

```rust
#[non_exhaustive]
pub enum SortDir {
    Asc,
    Desc,
    AscNullsFirst,
    AscNullsLast,
    DescNullsFirst,
    DescNullsLast,
}
```

The null-aware variants are explicit vocabulary per `35 §4.7`. `Asc` and `Desc` are null-placement-agnostic; the planner forwards the agnostic variant to the `PlanNode::Sort` node and the adapter's null-placement default applies.

### 3.7 Delta with legacy code

The legacy `crates/semstrait-planner/src/request.rs::ResolvedQueryRequest` folds several concerns into one struct:

- `entity_name: String` — the legacy name for `Request.from` (scalar, not optional). `34` ratifies `from: Option<DataKindRef>`, with `None` triggering field-first resolution.
- `dimensions: Vec<String>` — split in this spec into `dimensions` / `measures` / `metrics` to match `00 §4.1`.
- `grain: Option<Grain>` — DEFERRED to `17 §6`'s `Request.temporal` block; current `ResolvedQueryRequest.grain` is a legacy convenience that the API layer derived from `dimensions`.
- `session_variables: HashMap<String, String>` — refined into `SessionContext` (§4).

The rename is tracked in `implementation/40_refactor_plan.md` as `[TD-REQUEST-SHAPE]`. The legacy name `ResolvedQueryRequest` survives in the crate-internal `resolution::` pipeline as a stage-internal alias for the post-lookup form (§5) until the refactor lands.

### 3.8 `DataKindRef`

```rust
/// Reference to a top-level `ResolvedDataKind` by its canonical name.
/// Newtype over `DataKindName` (from `semstrait-core` / `11 §4`).
pub struct DataKindRef(pub DataKindName);

impl DataKindRef {
    pub fn new(name: impl Into<DataKindName>) -> Self;
    pub fn as_name(&self) -> &DataKindName;
}
```

`DataKindRef` is intentionally non-`#[non_exhaustive]` — it is a newtype over a stable inner type per `30`'s "newtype-over-stable" exception (`31 §3.2`).

### 3.9 `TemporalRequest` — reserved shape

```rust
/// Reserved per `17 §6.1`. Planner consumption is DEFERRED.
#[non_exhaustive]
pub struct TemporalRequest {
    pub as_of: Option<Timestamp>,
    pub range: Option<TimeRange>,
}

#[non_exhaustive]
pub struct TimeRange {
    pub start: Timestamp,
    pub end: Timestamp,
}
```

Exposed now, consumed later. Ratifying the shape in `34` lets the API layer (`semstrait-api`) populate the field in a forward-compatible way; per `17 §10`, the Round-1 planner emits `PLAN_W_1702` when a populated `TemporalRequest` targets a `TemporalShape::None` DataKind, and `PLAN_W_1700 ShapeAwareRequestDeferred` on any non-trivial consumption attempt.

## 4. `SessionContext`

### 4.1 Shape

```rust
/// Per-invocation runtime parameters the planner consumes. Never
/// `Option` on `Request`; every Request carries a `SessionContext`.
#[non_exhaustive]
pub struct SessionContext {
    pub now:             Timestamp,
    pub timezone:        Timezone,
    pub feature_toggles: FeatureToggles,
    pub correlation_id:  Option<CorrelationId>,
}
```

The planner reads `SessionContext` at exactly two sites: `Expr::Literal` folding for Request filters referencing session-resolved literals (e.g. `as_of = session.now`), and expressions whose resolved `PhysicalExpr` carries a `FunctionCall` whose `FunctionSpec` is session-dependent (`14a §3.5` Q3, currently empty). `#[non_exhaustive]` per I10: additional parameters (e.g. a `tenant_id` for per-tenant rewrites) are MINOR per `30 §2.2`.

### 4.2 Field-by-field

- **`now: Timestamp`** — the single fact every session-aware expression resolves against. The planner never calls `SystemTime::now()` itself (I11); a caller replaying a historical query sets `now` to that instant and every strategy observes it uniformly.
- **`timezone: Timezone`** — `#[non_exhaustive]` newtype over an IANA identifier; `Timezone::utc()` is `Default`. Feeds into `PhysicalExpr::DateTrunc` nodes declaring `zone: ZoneScope::Session` per `13 §3`; v1's `Grain` variants are session-zone-oblivious so the field is carried but not consumed.
- **`feature_toggles: FeatureToggles`** — a `BTreeMap<String, FeatureToggleValue>` shim, `#[non_exhaustive]`. Per I4, the planner's output is a pure function of `(manifest, request, feature_toggles)`. The planner reads toggles only at ratified sites (§11.4, `OptimizerPass::apply`).
- **`correlation_id: Option<CorrelationId>`** — opaque caller-supplied identifier embedded into every emitted `Diagnostic.location.source` so downstream log-aggregation can correlate a request across stage boundaries.

### 4.3 Constructors

```rust
impl SessionContext {
    /// Build a `SessionContext` with `now = SystemTime::now()`,
    /// `timezone = Timezone::utc()`, empty toggles, no correlation ID.
    /// Intended for tests and single-shot CLI tools.
    pub fn new() -> Self;

    /// Build a `SessionContext` pinned to a specific instant in UTC.
    pub fn at(now: Timestamp) -> Self;

    pub fn with_timezone(self, tz: Timezone) -> Self;
    pub fn with_feature_toggle(self, key: &str, value: FeatureToggleValue) -> Self;
    pub fn with_correlation_id(self, id: CorrelationId) -> Self;
}
```

The only constructor that reads the host clock is `new()`. Every other code path must call `at(...)` with an explicit timestamp, preserving I11's "no implicit I/O / host-env dependency on the hot path" rule.

### 4.4 Delta with legacy code

Legacy `crates/semstrait-planner/src/request.rs::SessionVariables` is a `HashMap<String, String>`. `34` ratifies a structured shape with four named fields; free-form session bag moves to `feature_toggles`. Migration tracked as `[TD-SESSION-CONTEXT]` in the refactor plan; the legacy `SessionVariables` alias survives for one MINOR cycle.

## 5. `ResolvedQueryRequest`

### 5.1 Shape

The Manifest-contextualized Request: every `SemanticsName` looked up, every target DataKind resolved (either via explicit `from` or via field-first resolution per §10), every `Filter.field` bound to its `ResolvedSemanticRef`. Produced by step 1 (§7.2) and consumed by step 4 (§7.5). NEVER exposed on the `plan` entry signature — a caller who needs the resolved form destructures the returned `SemanticPlan` instead. The type is `pub` so `Strategy` impls can consume it directly; construction is `pub(crate)` (only the step-1 builder produces well-formed instances).

```rust
#[non_exhaustive]
pub struct ResolvedQueryRequest {
    pub target:     ResolvedTarget,
    pub dimensions: Vec<(SemanticsName, ResolvedSemanticRef)>,
    pub measures:   Vec<(SemanticsName, ResolvedSemanticRef)>,
    pub metrics:    Vec<(SemanticsName, ResolvedSemanticRef)>,
    pub filters:    Vec<ResolvedFilter>,
    pub order:      Vec<(Name, SortDir)>,
    pub limit:      Option<u64>,
    pub offset:     Option<u64>,
    pub session:    SessionContext,
    pub temporal:   Option<TemporalRequest>,
}

#[non_exhaustive]
pub enum ResolvedTarget {
    Explicit(DataKindRef),                            // Request.from = Some(d)
    Implicit(DataKindRef),                            // `16 §11.3` fast path
    SynthesizedComposition {                          // `16 §11.4`–§11.5
        constituents:    Vec<DataKindRef>,
        traversed_paths: Vec<RelationshipPath>,
    },
}

#[non_exhaustive]
pub struct ResolvedSemanticRef {
    pub owner:      DataKindRef,
    pub element:    SemanticElement,
    pub binding_id: BindingId,                        // entry key into manifest.expr_table
}

#[non_exhaustive]
pub enum SemanticElement { Dimension, Measure, Metric, Filter, Key }
```

For `SynthesizedComposition`, the planner does not reify a `ComposedSemanticInterface` back onto the resolved request; composition-aware strategies consume `traversed_paths` and the constituent DataKinds directly via `ctx.plan_datakind`. For Complex-owned fields, `binding_id` points at the constituent Binding the field resolves through per `16 §7`'s `FieldOwnership`.

### 5.2 Invariants

A well-formed `ResolvedQueryRequest` satisfies: every `owner` names a `ResolvedDataKind` in `manifest.resolved_datakinds`; every `binding_id` is present in `manifest.expr_table` (I8 / `14b §2.3`); `target` is consistent with all `owner` fields; `dimensions` / `measures` / `metrics` entries have `element` matching the field they appear in; `filters` preserve caller order. Violation is a planner-internal bug; step 1 constructs only well-formed instances and consumers MAY `debug_assert!` in test builds.

### 5.3 Why this type is explicit

Exposing the resolved form lets strategies consume a single pre-resolved value: (a) re-running name resolution per dispatch would violate I5, (b) composition targets need `constituents` / `traversed_paths` that do not live on a raw `Request`, (c) filter placement (`21 §4.6`) needs element typing resolved once, not per strategy.

## 6. The `plan` function signature

### 6.1 Signature

```rust
/// Planner entry point. Per `10 §3.4`.
///
/// Consumes a read-only `Manifest` reference and an owned `Request`;
/// returns a well-formed `SemanticPlan` on success or a fail-fast
/// `PlanErrors` carrier on failure.
///
/// Sync (I6). Never mutates the Manifest (I8 / I5). Performs no I/O
/// (I11). Accumulates non-error diagnostics across both arms per
/// `30 §7` — warnings surfaced during planning flow out through
/// `PlanErrors.warnings` (error arm) or `SemanticPlan.diagnostics`
/// (success arm).
pub fn plan(
    manifest: &Manifest,
    request: Request,
) -> Result<SemanticPlan, PlanErrors>;
```

**Input ownership.** `Manifest` is borrowed (never consumed) — a Manifest is typically cached across many `plan` calls; the planner takes a read-only view. `Request` is owned (consumed) — its fields are destructured into the internal `ResolvedQueryRequest` representation, and retaining the caller's copy has no utility.

**Return shape.** `Result<SemanticPlan, PlanErrors>` rather than `Result<(SemanticPlan, Vec<Diagnostic>), (Diagnostic, Vec<Diagnostic>)>` (the `30 §7` primitive form). The `PlanErrors` carrier (§13.2) wraps the error + warnings pair; the success-arm `SemanticPlan.diagnostics` (`35 §3.1`) carries the warnings. The binary difference from `30 §7`'s shape is presentation only: a caller can trivially destructure either form.

### 6.2 Preconditions on `manifest`

The Manifest must satisfy every invariant in `33 §3.1` — in particular, `expr_table` must be populated for every exposed `(name, binding_id)` pair, and every referenced `RelationshipId` must be present in `resolved_relationships`. Feeding a partially-built Manifest is a caller error; the planner's index-inconsistency checks raise `PLAN_E_2052` where possible but are not exhaustive. Multi-thread access is safe per `33 §12` — a single `Manifest` may back concurrent `plan` calls.

### 6.3 Postconditions

**On success** (`Ok(plan)`): every `SemanticPlan` invariant from `35 §3.2` holds — `output_names.len()` equals `root.meta().output_schema.len()`, every `Name` is valid per `35 §5.4`, the tree satisfies `35 §7` well-formedness, and `diagnostics` carries no `Severity::Error` entries. Step 6 (§7.7) is the enforcement point.

**On failure** (`Err(errs)`): `errs.error.severity() == Severity::Error`; `errs.error.code()` is a stable `PLAN_E_*` constant per `30 §6.2` (§13); `errs.warnings` carries every non-error `Diagnostic` emitted before the fail-fast abort.

### 6.4 Thread-safety

`plan` is `Send` on the invoking thread and reentrant under concurrent invocations against the same Manifest (reads only; no interior mutability across calls).

## 7. Plan pipeline sub-steps

Per `10 §3.4`, `plan` executes seven sub-steps in fixed order. The ordering is binding — a strategy MAY NOT reorder sub-steps, and a caller cannot influence ordering. Each sub-step has a defined input, output, and error surface. A failure at any step short-circuits the remaining steps (fail-fast, `30 §7`).

### 7.1 Step 0 — Constraint validation (`11 §8.6`)

**Work.** Invoke `ConstraintValidator::check(request, manifest)` per `11 §8.6`. The v1 checker iterates every Measure and Metric named in `request.measures` / `request.metrics`, resolves each via the Manifest's name index, and evaluates every `MeasureConstraints` entry against the Request's query scope (`dimensions` ∪ filter-field Dimensions) and against the Measure's effective aggregation.

**Output.** Either a successful advance to step 1, or a fail-fast abort with `PLAN_E_0500 ConstraintViolation { entity, message }` per `11 §8.7`. The v1 checker short-circuits on first violation; future fan-out (`[TD-CONSTRAINT-ERROR-FANOUT]`) refines the carrier without changing step-0 placement.

**Why step 0.** Constraint violations express author-declared admissibility rules. Running them before any Manifest-index work avoids redundant field-first resolution / relationship / dispatch work on Requests that cannot be planned by author intent.

### 7.2 Step 1 — Request lookup (`11 §5`)

**Work.** Per `11 §5`'s lookup algorithm and `14b §2.3`'s O(1) guarantee:

1. For each name in `request.dimensions` ∪ `request.measures` ∪ `request.metrics`, look up the `SemanticsName` in the Manifest's name index. Unknown names raise `PLAN_E_0504 UnknownSemantics { name }`.
2. Record each name's `SemanticElement` and owning `DataKindRef`. Mismatch with the field the name was placed in raises `PLAN_E_0512 RequestFieldTypeMismatch`.
3. For each `Filter.field`, look up similarly; arity mismatch raises `PLAN_E_0520`; value-type mismatch raises `PLAN_E_0521`.
4. Duplicate names within any field list raise `PLAN_E_0510`.

No name resolution beyond the pre-built index (I5). Complexity is O(names × log n) over `BTreeMap`-backed indices per `14b §2.1`.

**Output.** Partially-built `ResolvedQueryRequest` — resolved refs populated; `target` not yet set. Step-1 errors sit in the `PLAN_E_05xx` band per `30 §6.2`.

### 7.3 Step 2 — Dataset routing

Two-branch decision:

- **Explicit (`request.from == Some(d)`).** Per `16 §11.6`: look up `d` in `manifest.resolved_datakinds`. Absent → `PLAN_E_2040`. For every resolved Semantics, assert the owner equals `d` (Simple) or appears in `d`'s constituents (Complex); violation → `PLAN_E_0507 SemanticsNotOnSurface`. Set `target = Explicit(d)`.
- **Implicit (`request.from == None`).** Invoke field-first resolution (§10 / `16 §11`). Output is `ResolvedTarget::Implicit(d)` (single-kind fast path) or `ResolvedTarget::SynthesizedComposition` (multi-target). Errors: `PLAN_E_0500` / `PLAN_E_0501` / `PLAN_E_0502` / `PLAN_E_0503` / `PLAN_E_0505`.

### 7.4 Step 3 — Relationship traversal

For composition targets (explicit Complex or `SynthesizedComposition`), the planner materializes the `ComposedSemanticInterface` the strategy will plan against:

- **Synthesized (implicit):** `traversed_paths` from step 2 are pre-walked; no extra work beyond packaging edges for strategy consumption (the synthesized interface is built on demand per `16 §10`).
- **Explicit Complex:** the target's `composed_interface` is already in the Manifest; step 3 is a consistency check.
- **Simple target:** step 3 is a no-op.

`PLAN_E_2052 ManifestIndexInconsistent` surfaces here when a recorded `RelationshipId` is missing from `manifest.resolved_relationships` — a Manifest-integrity bug.

### 7.5 Step 4 — Strategy dispatch (`20 §5.3`)

Invoke `dispatch_strategy(kind, &registry)` per `20 §5.3`: a single `match` on the `ResolvedDataKind` variant returning `&dyn Strategy`. Per `20 §5.3`, this is the **only** variant-match site in the hot path; every other planner site consumes `&dyn Strategy`. Defensive errors: `PLAN_E_2050`, `PLAN_E_2051`.

### 7.6 Step 5 — `PlanNode` construction

The dispatched strategy's `plan` method (§8) builds the subtree rooted at the target DataKind. Algorithm pointers by variant:

- `SimpleStrategy` (§9.1, `21 §4`) — 5-layer shape (Scan / Rename / Expression / Aggregate / Project) with interleaved `Filter`.
- `GrainsetStrategy` (§9.2, `22 §4`–§5) — child selection, recursive dispatch, optional rollup wrap.
- `UnionsetStrategy` (§9.3, `23 §4`) — per-branch recursive dispatch, NULL-fill seam, `Union`, optional re-aggregation.
- `JoinsetStrategy` (§9.4, `24 §5`) — anchor-outward path walk, per-member recursive dispatch, `Join` sequences, reconciling `Project`.

After the strategy returns, the planner wraps with Request-level nodes: (1) user-filter `Filter` nodes at the appropriate layer, (2) `Sort` if `order` non-empty, (3) `Fetch` if `limit` / `offset` set, (4) outermost `Project` pinning output column order.

Errors: per-variant `PLAN_E_21xx` / `PLAN_E_22xx` / `PLAN_E_23xx` / `PLAN_E_24xx` surfaced through the `PlanError::{Simple|Grainset|Unionset|Joinset}` wrapper variants; shared `PLAN_E_0600 PlanNodeConstructionFailed`, `PLAN_E_0601 UnsupportedRequestShape`.

### 7.7 Step 6 — Post-construction validation (`35 §7`)

Invoke `SemanticPlan::validate()` per `35 §8.3`. The validator walks the tree and checks every `35 §7` invariant (schema alignment, type resolution, predicate well-typedness, join-key agreement, union arm parity). Failure wraps an `IR_E_35xx` error into `PLAN_E_0610 PostConstructionInvariantViolated`.

**Production posture.** In optimized builds, step 6 is a no-op by default — strategies are trusted to produce well-formed plans. Test builds (`cfg(test)` + `debug_assertions`) always run it. An operator concerned about planner regressions opts in via the `semstrait.plan.validate` feature toggle (§4.2).

### 7.8 Pipeline summary diagram

```mermaid
flowchart TD
    S0["Step 0 — Constraint validation<br/>(11 §8.6)"] --> S1["Step 1 — Request lookup<br/>(11 §5 / 14b §2.3)"]
    S1 --> S2["Step 2 — Dataset routing<br/>(16 §11.3 / §11.6)"]
    S2 --> S3["Step 3 — Relationship traversal<br/>(16 §11.4 / §11.5)"]
    S3 --> S4["Step 4 — Strategy dispatch<br/>(20 §5.3)"]
    S4 --> S5["Step 5 — PlanNode construction<br/>(21–24)"]
    S5 --> S6["Step 6 — Post-construction validation<br/>(35 §7)"]
    S6 --> OUT[SemanticPlan]
```

Every arrow is fail-fast — a failure short-circuits to `Err(PlanErrors)` with accumulated warnings (§14).

## 8. The `Strategy` trait

### 8.1 Trait surface

Per `20 §5.2`, the `Strategy` trait is the planner-side contract every per-variant strategy implements. `20 §5.2` sketches the shape; `34` ratifies the concrete signature:

```rust
pub trait Strategy: Send + Sync {
    fn id(&self) -> StrategyId;
    fn supports(&self, kind: &ResolvedDataKind) -> bool;
    fn plan(
        &self,
        ctx: &StrategyContext,
        request: &ResolvedQueryRequest,
        datakind: &ResolvedDataKind,
    ) -> Result<PlanNode, PlanError>;
}
```

**`id`.** Stable identifier for diagnostic / debug / registry use — one of `StrategyId::{Simple, Grainset, Unionset, Joinset}`. Third-party strategies (if Q-PLAN-002 opens the trait) add variants via `#[non_exhaustive]`.

**`supports`.** Called by `dispatch_strategy` as a defense-in-depth check AFTER the variant-tag match has chosen the candidate strategy. Mismatch raises `PLAN_E_2051 StrategyMissingForVariant`. Built-in strategies return `true` for exactly one variant tag.

**`plan`.** Plans the Request against the given DataKind, returning the root `PlanNode` of the subtree (not a full `SemanticPlan` — Request-level wrapping is applied by step 5, §7.6, after the strategy returns). `ctx` carries session, registry for recursive dispatch, and the diagnostic sink; `request` is the resolved form from step 1; `datakind` is the variant this call is planning against (consistent with `request.target`).

**Borrow shapes.** `&StrategyContext` (not `&mut`) — mutation flows through interior-mutable fields so `&dyn Strategy` is shareable across threads. `&ResolvedQueryRequest` — constructed once per `plan` call and consumed read-only. Passing `&ResolvedDataKind` separately from `request.target` lets strategies access variant-specific fields (`ResolvedJoinset.path`, `ResolvedGrainset.levels`) without re-matching.

Per I6, no `async` anywhere in the surface; per I12, every failure returns a typed `PlanError` with a stable `PLAN_E_*` code.

### 8.2 `StrategyId`

```rust
#[non_exhaustive]
pub enum StrategyId { Simple, Grainset, Unionset, Joinset }

impl StrategyId {
    pub fn as_str(&self) -> &'static str; // "SimpleStrategy" …
}
```

Used in `Diagnostic` messages and structured logs only; never in user-facing error codes (those use the stable `PLAN_E_*` constants per §13).

### 8.3 `StrategyRegistry`

```rust
pub struct StrategyRegistry {
    simple:   Box<dyn Strategy>,
    grainset: Box<dyn Strategy>,
    unionset: Box<dyn Strategy>,
    joinset:  Box<dyn Strategy>,
}

impl StrategyRegistry {
    pub fn default_v1() -> Self;
    pub fn simple(&self)   -> &dyn Strategy;
    pub fn grainset(&self) -> &dyn Strategy;
    pub fn unionset(&self) -> &dyn Strategy;
    pub fn joinset(&self)  -> &dyn Strategy;
    pub fn with(
        simple:   impl Strategy + 'static,
        grainset: impl Strategy + 'static,
        unionset: impl Strategy + 'static,
        joinset:  impl Strategy + 'static,
    ) -> Self;
}
```

Holds exactly one strategy per variant; replacement is allowed at construction time only (no mutate-in-place API). Registry is `Send + Sync` — a single instance backs every `plan` invocation in a multi-threaded server. `with` is the test-doubles constructor.

### 8.4 `StrategyContext`

```rust
pub struct StrategyContext<'a> {
    pub manifest: &'a Manifest,
    pub session:  &'a SessionContext,
    pub registry: &'a StrategyRegistry,
    pub diagnostics: &'a DiagnosticSink,
    pub plan_builder: &'a dyn PlanBuilder,
}

impl<'a> StrategyContext<'a> {
    pub fn plan_datakind(
        &self,
        request: &ResolvedQueryRequest,
        datakind: &ResolvedDataKind,
    ) -> Result<PlanNode, PlanError>;
    pub fn emit(&self, diagnostic: Diagnostic);
}
```

`plan_datakind` is the recursive-dispatch entry point for Complex strategies (Unionset / Grainset / Joinset) planning child subplans. `emit` adds a non-error `Diagnostic` to the accumulator; at step 6 the accumulator is drained into `SemanticPlan.diagnostics` (success) or `PlanErrors.warnings` (failure).

`DiagnosticSink` is an interior-mutable container (`RefCell` / `Cell`); every field of `StrategyContext` is either `&T: Sync` or an interior-mutable wrapper over a `Send + Sync` type. `PlanBuilder` is the `10 §6` injection-mode hook for adapter-provided deterministic rewrites; `DefaultPlanBuilder` (v1) applies none.

### 8.5 `dispatch_strategy`

```rust
pub fn dispatch_strategy<'r>(
    kind: &ResolvedDataKind,
    registry: &'r StrategyRegistry,
) -> &'r dyn Strategy {
    match kind {
        ResolvedDataKind::Simple(_) => registry.simple(),
        ResolvedDataKind::Complex(ResolvedComplexDataKind::Unionset(_)) => registry.unionset(),
        ResolvedDataKind::Complex(ResolvedComplexDataKind::Grainset(_)) => registry.grainset(),
        ResolvedDataKind::Complex(ResolvedComplexDataKind::Joinset(_))  => registry.joinset(),
    }
}
```

The single variant-tag match site in the planner's hot path (per `20 §5.3`). `#[non_exhaustive]` on `ResolvedDataKind` / `ResolvedComplexDataKind` forces a new arm when a future Complex variant lands — a MINOR bump per `30 §2`. O(1) per dispatch; every other planner site consumes `&dyn Strategy` from here or from `ctx.plan_datakind` recursion.

## 9. Built-in strategies (v1 roster)

Each v1 strategy is a zero-sized, stateless struct constructed via `::new()` and held as `Box<dyn Strategy>` in the registry (§8.3). All four share the same `impl Strategy` shape — `id` returns the fixed `StrategyId`, `supports` pattern-matches on the variant tag, `plan` delegates to the algorithm in the cited data-kind spec. The signatures below omit the repetitive `impl Strategy` block; the authoritative algorithm lives in the cited sections.

### 9.1 `SimpleStrategy`

**Variant.** `ResolvedDataKind::Simple(ResolvedSimpleDataKind)`.

```rust
pub struct SimpleStrategy;
impl SimpleStrategy { pub fn new() -> Self; }
// impl Strategy per §8.1 — dispatches to the `21 §4` algorithm.
```

**Algorithm pointer.** `21 §4.1`–§4.7: L1 `Scan` per-source per `15 §3.6`; L2 `Rename` Semantics → physical columns; L3 `Expression` materializes Measure / Metric / Dimension expressions from `ResolvedExprTable`; L4 `Agg` aggregates per-Measure; L5 `Project` final-column projection. Single-source vs multi-source fan-out per `21 §4.2`; filter interleaving per `21 §4.6`.

**Errors.** `PLAN_E_21xx` per `21 §7`; shared `PLAN_E_0600`, `PLAN_E_0601`.

### 9.2 `GrainsetStrategy`

**Variant.** `ResolvedDataKind::Complex(ResolvedComplexDataKind::Grainset(ResolvedGrainset))`.

```rust
pub struct GrainsetStrategy;
impl GrainsetStrategy { pub fn new() -> Self; }
// impl Strategy per §8.1 — dispatches to the `22 §4` algorithm.
```

**Algorithm pointer.** `22 §4.1`–§4.5: derive `request_grain` from the Request's Dimensions; filter children via the eligibility predicate; apply the rollup-legality gate (cross-ref `17 §8`); pick the winner via the cost function + deterministic tie-break; recursively dispatch into the winner's strategy via `ctx.plan_datakind`; optionally wrap with rollup nodes per `22 §10.1`. The Grainset is a **planner strategy**, not a `PlanNode` variant — the emitted tree rooted at the Grainset's subtree is identical to what would be produced if the Request had targeted the winning child directly, plus the optional rollup wrapper.

**Errors.** `PLAN_E_22xx` per `22 §8`; notably `PLAN_E_2201 NoEligibleChild`, `PLAN_E_2205 SnapshotRollupWithoutPin`.

### 9.3 `UnionsetStrategy`

**Variant.** `ResolvedDataKind::Complex(ResolvedComplexDataKind::Unionset(ResolvedUnionset))`.

```rust
pub struct UnionsetStrategy;
impl UnionsetStrategy { pub fn new() -> Self; }
// impl Strategy per §8.1 — dispatches to the `23 §4` algorithm.
```

**Algorithm pointer.** `23 §4.1`–§4.6: per-branch Coverage narrowing (`23 §4.4`); recursive dispatch into each branch's strategy via `ctx.plan_datakind`; NULL-fill / type-reconciliation seam (`23 §4.3`); NullFill-only branch pruning (`23 §4.6`; advisory `PLAN_W_2301`; fully-NullFilled requests raise `PLAN_E_2303`); emit `PlanNode::Union` over surviving branches; optional terminal re-aggregation (`23 §4.5`).

**Errors.** `PLAN_E_23xx` per `23 §10`.

### 9.4 `JoinsetStrategy`

**Variant.** `ResolvedDataKind::Complex(ResolvedComplexDataKind::Joinset(ResolvedJoinset))`.

```rust
pub struct JoinsetStrategy;
impl JoinsetStrategy { pub fn new() -> Self; }
// impl Strategy per §8.1 — dispatches to the `24 §5` algorithm.
```

**Algorithm pointer.** `24 §5.1`–§5.5: scan-plan the anchor via `ctx.plan_datakind`; for each hop in `ResolvedJoinset.path`, scan-plan the hop target and emit a `PlanNode::Join` with per-hop `JoinType` / `Cardinality` (`24 §5.2`, `16 §4`); after every hop, emit a `Project` reconciling the `UnifiedSemantics` (`16 §6`). `AsOf` join emission is DEFERRED per `17 §5` / `24 §5.4` — the v1 planner emits `PLAN_W_2401 AsOfNotYetActive` and falls back to an inner join on the non-temporal key.

**Errors.** `PLAN_E_24xx` per `24 §10`.

### 9.5 Why no `AdHocStrategy`

The legacy planner (`crates/semstrait-planner/src/ad_hoc_join.rs`) carries a fifth "ad-hoc" dispatch path for Requests whose `entity_name` is empty. `34` subsumes this via field-first resolution (§10) — a `Request` with `from: None` and a multi-target field set produces a `SynthesizedComposition` target that dispatches through `UnionsetStrategy` / `GrainsetStrategy` / `JoinsetStrategy` per the composition kind. There is no distinct "ad-hoc strategy" in the v1 taxonomy; the legacy code path is a migration item tracked as `[TD-ADHOC-INTO-FIELD-FIRST]` in the refactor plan.

## 10. Field-first resolution

### 10.1 When it runs

Step 2 of the pipeline (§7.3) invokes field-first resolution when `request.from == None`. When `request.from` is `Some(d)`, step 2 takes the explicit-routing branch per `16 §11.6` and field-first resolution is skipped entirely.

### 10.2 Algorithm sketch (from `16 §11`)

The algorithm's canonical ratification is `16 §11`; §10 here records the planner-side realization and its integration with the Manifest indices.

1. **Name-to-kind map (`16 §11.2`).** For each `SemanticsName` in `request.dimensions ∪ request.measures ∪ request.metrics ∪ request.filters.field`, consult `manifest.name_index(name)`:
   - `None` → `PLAN_E_0504 UnknownSemantics { name }` (re-raised from step 1 if the lookup missed there).
   - `Some(owning)` → record the `Vec<DataKindRef>`.

2. **Candidate kind set `T`.** Deduplicate `⋃ owning` across selected names. If `|T| == 0`, emit `PLAN_E_0504` (unreachable here because step 1 already enforced non-empty owners).

3. **Single-kind fast path (`16 §11.3`).** If `|T| == 1`, return `ResolvedTarget::Implicit(T[0])`. The planner treats the Request as if `from: Some(T[0])` had been declared.

4. **Multi-target BFS over the `RelationshipGraph` (`16 §11.4`).** If `|T| >= 2`:
   - For `|T| == 2`: single-source shortest-path BFS between the two kinds.
   - For `|T| >= 3`: Steiner-tree enumeration up to `MAX_IMPLICIT_COMPOSITION_DEPTH` edges.
   - Neighbor iteration is deterministic (`(RelationshipId, direction_flag)` sort order).
   - Ambiguous cover trees of equal edge count → `PLAN_E_0500 AmbiguousImplicitComposition`.
   - No cover tree within depth bound → `PLAN_E_0501 NoCompositionPath`.
   - Hop count exceeding bound → `PLAN_E_0502 CompositionDepthExceeded`.
   - `Forward`-directionality violation → `PLAN_E_0503 CrossCompositionForbidden`.

5. **Synthesize the composed surface (`16 §11.5`).** Package the cover tree into a `ResolvedTarget::SynthesizedComposition { constituents, traversed_paths }`. The planner does **not** reify a `ComposedSemanticInterface` back onto the resolved request; composition-aware strategies consume `traversed_paths` and the constituent DataKinds during step 5 via the `ctx.plan_datakind` helper.

6. **Return.** `ResolvedTarget::{Implicit | SynthesizedComposition}`.

### 10.3 Integration with Manifest indices

The algorithm is a thin reader of the Manifest's pre-built indices: `manifest.name_index: BTreeMap<SemanticsName, Vec<DataKindRef>>` (`33 §5`), `manifest.relationship_graph: RelationshipGraph` (`14b §4.2`, adjacency-list with deterministic neighbor order), `manifest.resolved_relationships: BTreeMap<RelationshipId, ResolvedRelationship>` (`33 §3.1`), and the crate constant `MAX_IMPLICIT_COMPOSITION_DEPTH` (§10.4). Every lookup is O(log n); BFS / Steiner walk is O(E × depth-bound). No name resolution occurs (I5).

### 10.4 Depth bound

```rust
pub const MAX_IMPLICIT_COMPOSITION_DEPTH: usize = 3;
```

Per `16 §9.1`'s "implicit composition is bounded to unambiguous shortest-path chains, depth-limited". Attempts beyond the bound abort with `PLAN_E_0502`. Configurable only via an off-by-default feature toggle (`semstrait.plan.implicit_depth_max`); the limit is not a tunable on `SessionContext` because Manifest structure dominates what is ergonomic. A Model needing deep traversal should declare an explicit `Joinset` (`24`), making intent authored and planner work a direct lookup.

### 10.5 Interaction with Joinsets and `14b` path resolution

A `Request` with explicit `from: Some(joinset)` skips §10 entirely. A `Request` with `from: None` that would field-first-resolve onto the same constituents as an existing Joinset emits advisory `PLAN_W_0504 ImplicitCompositionShadowsJoinset`; the author likely meant to name the Joinset. Suppression is a feature toggle (`semstrait.plan.allow_implicit_joinset_shadow`).

Per `16 §11.7`, plan-time field-first resolution and compile-time cross-kind path resolution (`14b §4`) share the same `RelationshipGraph`, neighbor-iteration order, depth bound, and tie-break policy. The distinction is timing and input — `14b §4` runs at `compile` over one `SemanticExpr`, `34 §10` runs at `plan` over a Request's selected names. Both reuse a single `pub(crate)` BFS helper.

## 11. The `optimize` function

### 11.1 Signature

```rust
pub fn optimize(plan: SemanticPlan) -> Result<SemanticPlan, OptimizeErrors>;
```

Optimizer entry point per `10 §3.5`. Consumes a `SemanticPlan` and returns an equivalent plan (same observable results) with canonical-form rewrites applied. Sync (I6), no I/O (I11), fail-fast per `30 §7`. Passes are in-place rewrites over the tree (`PlanNode::transform` per `35 §8`); ownership is consumed because retaining the caller's copy has no utility.

The free function applies `Optimizer::with_v1_passes()`, bundling the four canonical passes (§11.2). A caller wishing to skip optimization simply does not call `optimize` — there is no bypass argument. Callers composing custom pass chains use `OptimizerBuilder` (§12.5) and `Optimizer::apply` directly.

### 11.2 Canonical v1 passes

| Pass | Name | Purpose | Code for failure | Section |
|---|---|---|---|---|
| 1 | `ConstantFolding` | Fold constant `PhysicalExpr` subtrees into `Expr::Literal` leaves. Operates on predicates, Project expressions, and Agg expressions. | `OPT_E_0101` | §11.3 |
| 2 | `MetadataDimensionSubstitution` | Substitute metadata-source Dimensions (per `13 §4.7` / SR-10) with their declared metadata expression. | `OPT_E_0102` | §11.4 |
| 3 | `PredicateSimplification` | Simplify predicates: `true AND x` → `x`, `false OR x` → `x`, `NOT NOT x` → `x`, range fusion (`x >= a AND x <= b` → `x BETWEEN a AND b`). | `OPT_E_0103` | §11.5 |
| 4 | `IdentityProjectElimination` | Remove `PlanNode::Project` nodes whose projection list is a 1:1 identity on the input schema. | `OPT_E_0104` | §11.6 |

Every pass is deterministic, shape-preserving, and pure over the plan tree. No pass introduces or removes a `PlanNode` variant — the tree's variant distribution is stable under the v1 pass chain. (Pass 4 removes a `Project` node, which is variant-count change but not variant-introduction.)

### 11.3 `ConstantFolding`

**Algorithm.** Post-order walk over every `PhysicalExpr` the plan carries. Fold `BinaryOp`, `Negate` / `Not`, `Cast`, `Coalesce`, `NullIf`, and `Case` nodes whose operands are all `Literal` into a single `Literal` per the semantics in `31 §3.1` / `13 §4`. Non-constant subtrees pass through unchanged. Overflow, precision loss, or divide-by-zero is non-fatal — the pass returns the original expression and emits `OPT_W_0101 ConstantFoldSkipped { reason }`.

**Interaction with `SessionContext`.** The pass does NOT fold session-dependent functions (`NOW()`, `CURRENT_DATE()`) even though `session.now` is available — folding them would prevent planner-level caching of the `SemanticPlan` across invocations with different `SessionContext.now`. Session-dependent folds happen at adapter time per `36`.

### 11.4 `MetadataDimensionSubstitution`

**Algorithm.** Per `13 §4.7` (SR-10), a Dimension may be declared `metadata_source: true`, meaning its runtime value is pinned at plan time rather than projected from a Binding. The pass walks every `PhysicalExpr` in the plan and substitutes every `Column { name }` referring to a metadata Dimension with the Dimension's declared metadata expression (typically a `Literal`). The substitution source is `manifest.metadata_index: BTreeMap<SemanticsName, PhysicalExpr>`, populated by `compile` per `14b §2.3`. The pass short-circuits if `metadata_index.is_empty()`.

Lifting the substitution to a uniform pass keeps per-strategy code free of metadata-aware bookkeeping.

### 11.5 `PredicateSimplification`

**Algorithm.** Post-order walk over every `PlanNode::Filter.predicate` and every `PhysicalExpr` child of Agg / Project nodes, applying a fixed rewrite-rule set: `Literal(Bool(true)) AND x` → `x`; `Literal(Bool(false)) AND x` → `false` (and symmetric `OR` rules); `NOT NOT x` → `x`; `NOT Literal(Bool(b))` → `Literal(Bool(!b))`; `x IS NULL AND x IS NULL` → `x IS NULL`; range fusion (`x >= a AND x <= b` → `x BETWEEN a AND b`) for same-type literal bounds; De Morgan only when it strictly shortens the expression. Every rewrite strictly shrinks the expression, so a single pass converges.

### 11.6 `IdentityProjectElimination`

**Algorithm.** Walk the tree. For every `PlanNode::Project { input, expressions, .. }`, replace the node with `input` if and only if (a) every expression is `PhysicalExpr::Column { name }`, (b) the name sequence equals the input's column-name sequence exactly, and (c) the output `Name` for each expression equals the column name (no rename). O(n) in node count.

The outermost Request-level `Project` emitted at step 5 (§7.6 item 4) pins `SemanticPlan.output_names` — the identity check includes the output-order constraint and the Project survives unless its input already presents the required order.

### 11.7 Pass ordering

Passes run in order 1 → 2 → 3 → 4. Pass 2 must run before pass 3 so that metadata-pinned predicates become literals pass 3 can eliminate; pass 1 must run before pass 3 for the same reason; pass 4 must run last because earlier passes MAY create identity-shaped `Project` nodes. Third-party passes MAY be inserted at any position via `OptimizerBuilder::with(...)` (§12.5); the v1 built-in order is fixed.

## 12. `OptimizerPass` trait

### 12.1 Trait surface

```rust
pub trait OptimizerPass: Send + Sync {
    fn name(&self) -> &str;
    fn apply(
        &self,
        plan: SemanticPlan,
        ctx: &OptimizePassContext,
    ) -> Result<SemanticPlan, OptimizeError>;
    fn is_applicable(&self, _plan: &SemanticPlan) -> bool { true }
}
```

Passes are pure, sync, deterministic (`10 §3.5`). `name` is stable per pass for diagnostics (`"constant_folding"`, `"identity_project"`, …); `apply` returns the rewritten plan or a fail-fast `OptimizeError`; `is_applicable` allows skipping a pass silently (default: always applicable). A pass that cannot produce an equivalent plan (e.g. an adapter-specific rewrite requiring dialect information) MUST NOT implement `OptimizerPass` — it belongs in the adapter's `adapt` stage per `36`.

### 12.2 `OptimizePassContext`

```rust
pub struct OptimizePassContext<'a> {
    pub manifest:    Option<&'a Manifest>,
    pub session:     Option<&'a SessionContext>,
    pub diagnostics: &'a DiagnosticSink,
}
```

`manifest` and `session` are `Option` because a `SemanticPlan` passed through `optimize` does not necessarily carry its producing Manifest / SessionContext — a plan deserialized from a wire form (`35 §3.3`) may arrive without either. Passes that require them MUST emit `OPT_E_0110 PassRequiresContext { pass, required }` when invoked without the needed reference.

### 12.3 Externally-implementable

`OptimizerPass` is **non-sealed** per `30 §4.6`: new passes are a primary extensibility vector. Adapter crates in particular MAY expose plan-level passes for engine-independent rewrites they've identified. Soundness (semantic equivalence) cannot be statically enforced by the trait; it relies on the author's discipline. A test harness in `semstrait-planner::tests::optimize` cross-checks every built-in pass against a golden-plan corpus.

### 12.4 `Optimizer`

```rust
pub struct Optimizer { passes: Vec<Box<dyn OptimizerPass>> }

impl Optimizer {
    pub fn empty() -> Self;
    pub fn with_v1_passes() -> Self;
    pub fn apply(
        &self,
        plan: SemanticPlan,
        ctx: &OptimizePassContext,
    ) -> Result<SemanticPlan, OptimizeError>;
    pub fn pass_count(&self) -> usize;
}
```

`apply` runs the passes in registered order, fail-fast on first error; warnings accumulate across passes via the context's diagnostic sink. `with_v1_passes` bundles the four canonical passes in order.

### 12.5 `OptimizerBuilder`

```rust
pub struct OptimizerBuilder { passes: Vec<Box<dyn OptimizerPass>> }

impl OptimizerBuilder {
    pub fn new() -> Self;
    pub fn with(self, pass: impl OptimizerPass + 'static) -> Self;
    pub fn with_v1_passes(self) -> Self;
    pub fn build(self) -> Optimizer;
}
```

The free function `optimize(plan)` is equivalent to `OptimizerBuilder::new().with_v1_passes().build().apply(plan, &default_ctx)`. Callers wishing to insert custom passes before or after the v1 chain compose explicitly via the builder.

## 13. `PlanError` / `PlanErrors` / `OptimizeError` / `OptimizeErrors`

### 13.1 `PlanError`

```rust
/// Typed planner error. Per `30 §6.2`; stable `PLAN_E_*` codes.
///
/// `#[non_exhaustive]` per I10: adding a variant is MINOR per
/// `30 §2.2`.
#[non_exhaustive]
pub enum PlanError {
    // -- step 0: constraint validation (§7.1 / 11 §8.7) --
    ConstraintViolation { entity: String, message: String, location: Option<Location> },

    // -- step 1: request lookup (§7.2) --
    UnknownSemantics        { name: SemanticsName, location: Option<Location> },
    DuplicateRequestedName  { name: SemanticsName, location: Option<Location> },
    EmptyRequest            { location: Option<Location> },
    RequestFieldTypeMismatch{ name: SemanticsName, placed_in: SemanticElement, resolved: SemanticElement, location: Option<Location> },
    OrderByUnknownName      { name: Name, location: Option<Location> },
    FilterArityMismatch     { field: SemanticsName, operator: FilterOperator, expected: usize, got: usize, location: Option<Location> },
    FilterValueTypeMismatch { field: SemanticsName, resolved_type: DataType, value: String, location: Option<Location> },

    // -- step 2/3: dataset routing and relationship traversal (§7.3 / §7.4) --
    AmbiguousImplicitComposition   { candidates: Vec<Vec<DataKindRef>>, location: Option<Location> },
    NoCompositionPath              { disconnected: Vec<DataKindRef>, location: Option<Location> },
    CompositionDepthExceeded       { attempted: usize, bound: usize, location: Option<Location> },
    CrossCompositionForbidden      { relationship: RelationshipId, location: Option<Location> },
    AmbiguousCompositionReference  { name: SemanticsName, candidates: Vec<DataKindRef>, location: Option<Location> },
    SemanticsNotOnSurface          { name: SemanticsName, target: DataKindRef, location: Option<Location> },

    // -- step 4/5: dispatch + construction (§7.5 / §7.6) --
    PlanNodeConstructionFailed     { strategy: StrategyId, reason: String, location: Option<Location> },
    UnsupportedRequestShape        { reason: String, location: Option<Location> },
    PostConstructionInvariantViolated { underlying: String, location: Option<Location> },

    // -- DataKind-specific (§7.5 delegate) --
    DataKindNotInManifest          { name: DataKindRef, location: Option<Location> },
    StrategyDispatchFailed         { kind_variant: String, location: Option<Location> },
    StrategyMissingForVariant      { kind_variant: String, location: Option<Location> },
    ManifestIndexInconsistent      { detail: String, location: Option<Location> },

    // -- per-variant error passthrough (21–24) --
    Simple   (SimpleError),
    Grainset (GrainsetError),
    Unionset (UnionsetError),
    Joinset  (JoinsetError),

    // -- temporal (17, DEFERRED) --
    TemporalDeferred               { code: TemporalDeferredCode, location: Option<Location> },
}

impl PlanError {
    /// Stable `PLAN_E_*` code per `30 §6.2`. Example: `"PLAN_E_0500"`.
    pub fn code(&self) -> &'static str;

    /// `Severity::Error` for every v1 variant.
    pub fn severity(&self) -> Severity;

    pub fn location(&self) -> Option<&Location>;
}

impl IntoDiagnostic for PlanError { fn into_diagnostic(self) -> Diagnostic; }
impl std::fmt::Display for PlanError { /* per-variant messages */ }
impl std::error::Error for PlanError {}
```

**Code allocation** (per `30 §6.2` / `20 §8.1`):

| Code | Variant | Section |
|---|---|---|
| `PLAN_E_0500` | `ConstraintViolation` | §7.1 / `11 §8.7` |
| `PLAN_E_0500` | `AmbiguousImplicitComposition` (alternative; see `16 §14.3`) | §10.2 |
| `PLAN_E_0501` | `NoCompositionPath` | §10.2 |
| `PLAN_E_0502` | `CompositionDepthExceeded` | §10.2 |
| `PLAN_E_0503` | `CrossCompositionForbidden` | §10.2 |
| `PLAN_E_0504` | `UnknownSemantics` | §7.2 |
| `PLAN_E_0505` | `AmbiguousCompositionReference` | §10.2 |
| `PLAN_E_0507` | `SemanticsNotOnSurface` | §7.3 |
| `PLAN_E_0510` | `DuplicateRequestedName` | §7.2 |
| `PLAN_E_0511` | `EmptyRequest` | §7.2 |
| `PLAN_E_0512` | `RequestFieldTypeMismatch` | §7.2 |
| `PLAN_E_0513` | `OrderByUnknownName` | §7.2 |
| `PLAN_E_0520` | `FilterArityMismatch` | §7.2 |
| `PLAN_E_0521` | `FilterValueTypeMismatch` | §7.2 |
| `PLAN_E_0600` | `PlanNodeConstructionFailed` | §7.6 |
| `PLAN_E_0601` | `UnsupportedRequestShape` | §7.6 |
| `PLAN_E_0610` | `PostConstructionInvariantViolated` | §7.7 |
| `PLAN_E_1700`+ | `TemporalDeferred` variants | `17 §9` (DEFERRED) |
| `PLAN_E_2040` | `DataKindNotInManifest` | `20 §8.2` |
| `PLAN_E_2050` | `StrategyDispatchFailed` | `20 §8.2` |
| `PLAN_E_2051` | `StrategyMissingForVariant` | `20 §8.2` |
| `PLAN_E_2052` | `ManifestIndexInconsistent` | `20 §8.2` |
| `PLAN_E_21xx` | `Simple(SimpleError)` | `21 §7` |
| `PLAN_E_22xx` | `Grainset(GrainsetError)` | `22 §8` |
| `PLAN_E_23xx` | `Unionset(UnionsetError)` | `23 §10` |
| `PLAN_E_24xx` | `Joinset(JoinsetError)` | `24 §10` |

The ranges `0506`, `0508`, `0509`, `0514`–`0519`, `0522`–`0599`, `0602`–`0609`, `0611`–`0699` are reserved against future additions within step-level sub-bands.

Collision note: `PLAN_E_0500` is currently referenced by both `ConstraintViolation` (per `11 §8.7`, step 0) and `AmbiguousImplicitComposition` (per `16 §14.3`). This is the sole code-allocation conflict across the spec; §17 and `questions/open/34_questions.md` Q-PLAN-003 track the reconciliation — either move `AmbiguousImplicitComposition` to `PLAN_E_0506` (the next free slot in the composition sub-band) or move `ConstraintViolation` to its own dedicated code. Pending resolution, `34` uses `PLAN_E_0500` for the step-0 constraint carrier and notes the aliasing inline.

### 13.2 `PlanErrors`

```rust
#[non_exhaustive]
pub struct PlanErrors {
    pub error:    PlanError,
    pub warnings: Vec<Diagnostic>,
}

impl PlanErrors {
    pub fn new(error: PlanError) -> Self;
    pub fn with_warnings(error: PlanError, warnings: Vec<Diagnostic>) -> Self;
    pub fn code(&self) -> &'static str { self.error.code() }
    pub fn severity(&self) -> Severity { Severity::Error }
}

impl IntoDiagnostic for PlanErrors { fn into_diagnostic(self) -> Diagnostic; }
```

`error` is the fatal error that aborted planning; `warnings` collects non-error diagnostics emitted up to the abort (typically empty when failure is at step 0/1; populated when a strategy emitted advisories during step 5 before a later sub-step failed).

### 13.3 Non-error diagnostic emission

Strategies emit non-error diagnostics via `ctx.emit(diag)` (§8.4). The per-plan-call `DiagnosticSink` accumulates them in emission order. At step 6 the sink drains into `SemanticPlan.diagnostics` (success) or `PlanErrors.warnings` (failure). Diagnostics flow through both arms — no silent drops (per `30 §7`).

### 13.4 `OptimizeError` / `OptimizeErrors`

```rust
#[non_exhaustive]
pub enum OptimizeError {
    PassFailed          { pass: String, reason: String, location: Option<Location> },
    InvalidRewrite      { pass: String, detail: String, location: Option<Location> },
    PassRequiresContext { pass: String, required: &'static str, location: Option<Location> },
}

impl OptimizeError {
    pub fn code(&self) -> &'static str;
    pub fn severity(&self) -> Severity;
    pub fn location(&self) -> Option<&Location>;
}

impl IntoDiagnostic for OptimizeError { fn into_diagnostic(self) -> Diagnostic; }

#[non_exhaustive]
pub struct OptimizeErrors {
    pub error:    OptimizeError,
    pub warnings: Vec<Diagnostic>,
}
```

**Code allocation** (`OPT_E` subsystem, `0100`–`0199` per `30 §6.2`): `OPT_E_0101`–`OPT_E_0104` — `PassFailed` (one per canonical pass `constant_folding` / `metadata_dimension_substitution` / `predicate_simplification` / `identity_project_elimination`); `OPT_E_0110` `PassRequiresContext`; `OPT_E_0111` `InvalidRewrite`.

Canonical v1 passes are specified to NOT error under well-formed inputs; `OPT_E_01xx` fires in practice only for third-party passes with soundness bugs.

### 13.5 `Diagnostic`-conversion

Every `PlanError` and `OptimizeError` variant carries an `Option<Location>`. `IntoDiagnostic::into_diagnostic` produces a `Diagnostic { code, severity, location, message, source_chain }` per `31 §7.1` with `code` = stable constant, `severity` = `Error`, `message` = `Display` form, and the underlying `source_chain` preserved. Non-error diagnostics use the same shape with `Severity::Warning` or `Severity::Note` and `PLAN_W_*` / `OPT_W_*` codes per `30 §6.2`.

## 14. Diagnostics accumulation policy

### 14.1 `plan` is fail-fast

Per `10 §5` and `30 §7`, the `plan` stage is fail-fast. The first `PlanError` produced by any sub-step aborts the pipeline; later sub-steps do not run (step 0 fails → steps 1–6 skipped; step 1 fails → steps 2–6 skipped; and so on). The first error becomes `PlanErrors.error`; non-error diagnostics emitted before the abort are preserved in `PlanErrors.warnings`.

### 14.2 Constraint violations (step 0) are fail-fast per `11 §8.7`

The v1 `ConstraintValidator` short-circuits on first violation — subsequent Measures / Metrics / constraints are not checked for the same Request. Future refinement (`[TD-CONSTRAINT-ERROR-FANOUT]`) may move constraint evaluation to accumulate mode while leaving the outer `plan` stage fail-fast; the boundary is re-drawable.

### 14.3 Non-error diagnostics accumulate

Non-error diagnostics (warnings, notes) do NOT fail-fast. They accumulate in the per-invocation `DiagnosticSink` (§8.4) and flow out through `SemanticPlan.diagnostics` (success) or `PlanErrors.warnings` (failure), mirroring `30 §7`'s "warnings are never silently dropped" rule.

### 14.4 `optimize` is fail-fast per `30 §7`

The `optimize` stage matches `plan`'s discipline. A pass returning `Err(OptimizeError)` aborts the remaining chain; non-error pass diagnostics accumulate and flow through `SemanticPlan.diagnostics` / `OptimizeErrors.warnings` identically.

### 14.5 Idempotence of re-`optimize`

Re-applying `optimize` to an already-optimized plan is a no-op at the v1 canonical-pass level — every pass reaches a fixed point in a single run. Callers MAY optimize-then-re-optimize without observable change. Third-party passes are not guaranteed idempotent; the convention is encouraged, not enforced.

## 15. Stability

### 15.1 `Strategy` trait — open for adapter extension

The `Strategy` trait is **non-sealed** (`30 §4.6`). Third-party crates MAY implement it. This is a deliberate extensibility point: adapter authors who need a custom plan-tree shape for a novel `Complex` DataKind variant (added under I10 per `20 §5.3`) contribute a new `Strategy` impl and register it into a custom `StrategyRegistry`.

**Caveat.** The built-in variant dispatch (`dispatch_strategy`, §8.5) matches on the ratified `ResolvedDataKind` variant set — a third-party strategy is useful only when paired with a third-party `ResolvedDataKind` variant (also under I10 per `20 §5.1`). The v1 built-in registry holds exactly the four built-in strategies; a custom strategy requires building a custom registry.

Whether the trait should be **sealed** (restricting impls to the workspace) is tracked as `Q-PLAN-002` in open questions. The Round-1 default is non-sealed per `20 §9.1`'s Q-KIND-001 pending resolution.

### 15.2 Built-in strategies — stable

`SimpleStrategy`, `GrainsetStrategy`, `UnionsetStrategy`, `JoinsetStrategy` are **stable types** in the crate's public API. Renaming any of them is MAJOR per `30 §2.1`. Behavioral changes to their `plan` methods are MINOR if they preserve the ratified algorithm (`21 §4`, `22 §4`–§5, `23 §4`, `24 §5`) and MAJOR if they alter the observable shape of the emitted plan tree.

### 15.3 `OptimizerPass` trait — open

The `OptimizerPass` trait is **non-sealed** per §12.3. Third-party passes compose via `OptimizerBuilder::with(...)`. The v1 canonical passes are stable types; the pass-chain order within `Optimizer::with_v1_passes()` is stable per §11.7.

### 15.4 `#[non_exhaustive]` discipline

Every public `pub enum` and every public `pub struct` exposed by `34` carries `#[non_exhaustive]` with the following exceptions (newtype-over-stable per `30 §3.5`):

- `DataKindRef` — newtype over `DataKindName`.
- `StrategyRegistry` — stable shape by construction.
- `Optimizer` / `OptimizerBuilder` — stable internal vector.

Adding a new variant to any `#[non_exhaustive]` enum (e.g. a new `PlanError` variant for a new error condition, a new `StrategyId` variant for an externally-contributed strategy) is MINOR per `30 §2.2`.

### 15.5 Error-code stability

Per `30 §6.3`:

- A published `PLAN_E_*` / `OPT_E_*` code's meaning is frozen at its first release.
- Adding a new code in a reserved sub-range is MINOR.
- Retiring a code is MAJOR; deprecation is MINOR with `#[deprecated(since, note)]`.

The `PLAN_E_0500` aliasing (§13.1) between `ConstraintViolation` and `AmbiguousImplicitComposition` is a **pre-release** allocation conflict to be resolved before v1 release; §17's reconciliation notes it.

## 16. Crate boundaries

### 16.1 NO I/O

No `std::fs`, no `std::net`, no `tokio`, no `reqwest`, no `aws-sdk-*`, no `object_store` in the crate's dependency graph. A `plan` or `optimize` invocation performs zero syscalls on the hot path; every datum consulted is already in the `Manifest` (I8 / I11). `tracing::debug!` is permissible as instrumentation — `tracing` is a no-op when no subscriber is installed.

### 16.2 NO SQL emission

The planner emits `PlanNode`s carrying `PhysicalExpr` trees (`35 §4`). No SQL string is produced and no dialect-aware operator is chosen; SQL emission is strictly `semstrait-adapter`'s concern (`36`) per I1 / I3. Filter placement names `PlanNode::Filter` (not `WHERE` / `HAVING`); aggregation emission names `PlanNode::Agg` with `Aggregation` variants (not `SUM(...)` strings).

### 16.3 NO YAML parsing

YAML parsing lives in `semstrait-model` (`32`). The `Request` the planner accepts is already a Rust value — constructed by `semstrait-api` or any direct Rust caller. No `serde_yaml` / `yaml-rust` in the dependency graph.

### 16.4 NO catalog access

`CatalogProvider` / `FileSystem` (`37`) are strictly a `compile`-stage concern. The planner takes a `&Manifest` that already encodes every piece of catalog data it needs; no `&dyn CatalogProvider` appears on any planner surface. The legacy `SemanticPlanner.catalog: Option<Arc<dyn CatalogProvider>>` field is a migration item (`[TD-PLANNER-NO-CATALOG]`) retained only for the handful of ad-hoc-join paths pending the field-first-resolution refactor (§9.5); the ratified surface drops it entirely.

### 16.5 Dependency posture

A canonical `Cargo.toml` target (matching `31 §12.1`'s discipline):

```toml
[dependencies]
semstrait-core     = { workspace = true }
semstrait-ir       = { workspace = true }
semstrait-manifest = { workspace = true }

thiserror  = "^"    # error enum derivations
tracing    = "^"    # instrumentation-only; no I/O

[dependencies.serde]
version = "^"
optional = true
features = ["derive"]

[features]
default = []
serde = ["dep:serde", "semstrait-core/serde", "semstrait-ir/serde", "semstrait-manifest/serde"]
```

**No runtime async dependencies.** No `tokio`, `async-trait`, `futures`, `reqwest`.

**No engine-identity dependencies.** No `datafusion`, `arrow`, `spark-*`, `duckdb`, `substrait` — these live in `semstrait-adapter`.

**Zero `semstrait-adapter` / `semstrait-catalog` / `semstrait-model` dependencies.** The planner sits strictly above the first four workspace crates and strictly below the adapter / API crates per I7's DAG.

### 16.6 CI enforcement

Per `31 §13`, concrete CI checks guard each boundary:

- `cargo deny` enforces §16.1 and §16.5's dependency bans.
- `cargo clippy -- -D clippy::async_fn_in_trait` enforces I6.
- `cargo public-api` snapshot test enforces the surface in §2.
- An integration test asserts that every exported `pub enum` / `pub struct` (minus the stable-newtype exception set) carries `#[non_exhaustive]`.
- A grep-based CI lint rejects `String`-typed SQL literals inside planner source (the `EXPRESSION_INCLUDES_SQL` regex).

## 17. Round-1 open items

Open items surfaced during the drafting of `34` that cannot be resolved from `10`–`17`, `20`–`25`, `30`–`33`, or `35` alone. Full write-ups and proposed next steps live in `questions/open/34_questions.md`.

| ID | Title | Section | Blocking? |
|---|---|---|---|
| Q-PLAN-001 | `Request.from` shape — `DataKindRef` scalar vs `DataKindPath` for nested Complex targeting | §3.8 | no |
| Q-PLAN-002 | `Strategy` trait openness — sealed vs open for third-party implementers | §8.1 / §15.1 | no |
| Q-PLAN-003 | `PLAN_E_0500` aliasing between `ConstraintViolation` and `AmbiguousImplicitComposition` — re-allocate one to `PLAN_E_0506` | §13.1 | **yes** (pre-release reconciliation) |
| Q-PLAN-004 | `TemporalRequest` vocabulary in `Request` — expose now vs. defer to the `17` milestone | §3.9 | no (follows `17`) |
| Q-PLAN-005 | `SessionContext.feature_toggles` typing — free-form vs. typed catalog | §4.2 | no |
| Q-PLAN-006 | `OptimizerPass` idempotence — enforce via a proof obligation vs. convention | §14.5 | no |
| Q-PLAN-007 | `ResolvedQueryRequest` visibility — `pub` vs. `pub(crate)` given strategies are the only consumers | §5 | no |
| Q-PLAN-008 | Field-first depth bound (`MAX_IMPLICIT_COMPOSITION_DEPTH = 3`) — is 3 the right default? | §10.4 / `16` Q-COMP-001 | no |

Cross-doc fixes flagged while drafting `34`:

| ID | Location | Fix |
|---|---|---|
| CDF-30-02 | `30 §6.2` `PLAN_E` row | Currently lists `"0500–0599 Constraint-violation + request-shape"`. Per Q-PLAN-003, the sub-band is overcommitted — extend the note to spell out the `0500`–`0509` (composition), `0510`–`0519` (request-shape), `0520`–`0529` (filter-shape) sub-bands or move `ConstraintViolation` to its own sub-range. |
| CDF-21-01 | `21 §7` | Per-variant `PLAN_E_21xx` roster should cross-reference `34 §13` as the aggregation surface (`PlanError::Simple(SimpleError)` wraps them). |
| CDF-22-01 | `22 §8` | Same as CDF-21-01 for `PLAN_E_22xx` and `PlanError::Grainset(GrainsetError)`. |
| CDF-23-01 | `23 §10` | Same as CDF-21-01 for `PLAN_E_23xx` and `PlanError::Unionset(UnionsetError)`. |
| CDF-24-01 | `24 §10` | Same as CDF-21-01 for `PLAN_E_24xx` and `PlanError::Joinset(JoinsetError)`. |

Deferred / known-gap items (tracked in the implementation plan, not in open-questions):

- **Code-vocabulary rename — `SemanticPlanner` → free `plan` / `optimize` functions.** The legacy crate (`crates/semstrait-planner/src/planner.rs`) carries a `SemanticPlanner` struct with a `.plan()` method. `34` ratifies free functions at the crate root as the canonical surface. The rename is tracked as `[TD-PLANNER-SHAPE]` in `implementation/40_refactor_plan.md`.
- **`SessionVariables` → `SessionContext`.** See `[TD-SESSION-CONTEXT]` in §4.4.
- **`ResolvedQueryRequest` → (see §3.7)** — legacy shape folds `Request` + `SessionContext` + partial resolution; v1 splits into `Request` (caller surface) + `ResolvedQueryRequest` (internal post-lookup form).
- **Legacy `AdHocJoin` strategy → field-first resolution.** See `[TD-ADHOC-INTO-FIELD-FIRST]` in §9.5.
- **Optional catalog field on `SemanticPlanner` → dropped per §16.4.** See `[TD-PLANNER-NO-CATALOG]`.

---

*Cross-references in this document are by section (e.g. `20 §5.3`, `16 §11.5`, `14b §2.3`). No code-path references are used, per `00 §8`.*
