# Unified Expression Design — Eliminating the Dual DslExpr Problem

**Status:** Implemented (Option B — Phases 1-5 complete) | **Supersedes:** DL-020 (dual DslExpr rationale)

---

## 1. Problem Statement

The codebase has two expression types with overlapping names and semantics:

| Type | Location | Variants | Purpose |
|------|----------|----------|---------|
| `core::DslExpr` | `semstrait-core/src/dsl_expr.rs` | 30 variants (typed: `Sum`, `Add`, `Eq`, `Guard`, etc.) | YAML-parsed semantic model expressions |
| `ir::DslExpr` | `semstrait-ir/src/plan_node.rs` | 22 variants (`BinaryOp { op }`, `FunctionCall`, etc.) | Plan node IR expressions |

A 440-line `expr_lower.rs` in `semstrait-planner` mechanically converts one to the other. This conversion:

1. **Loses type fidelity** — `core::Sum(AggExpr)` becomes `ir::FunctionCall { name: "SUM" }` (string-typed)
2. **Loses precision** — `LiteralExpr::Integer { value: i64 }` becomes `Number(f64)`
3. **Duplicates structure** — both types represent the same logical concepts (column refs, literals, binary ops, aggregations, etc.)
4. **Creates naming confusion** — both are called `DslExpr`, aliased as `CoreExpr`/`IrExpr` at import sites
5. **Forces downstream re-interpretation** — SQL emitter and polyglot builder must re-parse string function names ("SUM", "COUNT") back into typed dispatch

### Dependency chain today

```
YAML → parse → core::DslExpr                      (semstrait-model, semstrait-manifest)
                    │
                    ▼ lower_expr() [440 lines]
               ir::DslExpr                         (semstrait-planner/expr_lower.rs)
                    │
                    ├──▶ ExprConverter → Substrait  (semstrait-ir/expr_converter.rs)
                    ├──▶ DslExprSqlRenderer → SQL   (semstrait-sql/expr_renderer.rs)
                    └──▶ ExprBuilder → polyglot AST (semstrait-sql/polyglot/expr_builder.rs)
```

The lowering step exists primarily to:
- Resolve `EntityRef` → `Column` (semantic name → physical column via `column_mapping`)
- Convert `Guard` → `Case` (sugar expansion)
- Flatten `LogicalExpr { exprs: Vec }` → chained `BinaryOp { And/Or }`
- Extract aggregates into `AggregateMeasure` structs

None of these require a fundamentally different expression type.

---

## 2. Design Goal

**One `Expr` type** used from YAML parsing through to SQL emission, with Substrait serialization as a trait method rather than a separate converter.

---

## 3. Proposed Architecture

### 3.1 Unified `Expr` enum in `semstrait-core`

```rust
// semstrait-core/src/expr.rs

/// The single expression type for the entire pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    // ── Leaf nodes ──
    Column(ColumnRef),
    Literal(Literal),
    EntityRef(EntityRef),       // pre-resolution only; resolved to Column during planning

    // ── Aggregations (typed, not string-based) ──
    Aggregate(AggregateExpr),

    // ── Arithmetic ──
    BinaryOp { left: Box<Expr>, op: BinaryOp, right: Box<Expr> },
    Negate(Box<Expr>),

    // ── Comparison (uses same BinaryOp enum) ──
    // Eq, NotEq, Lt, etc. are BinaryOp variants — no separate enum arms needed

    // ── Logical ──
    Not(Box<Expr>),
    // And/Or are BinaryOp::And / BinaryOp::Or — chains via nesting

    // ── Predicates ──
    InList { expr: Box<Expr>, list: Vec<Expr>, negated: bool },
    Between { expr: Box<Expr>, low: Box<Expr>, high: Box<Expr>, negated: bool },
    Like { expr: Box<Expr>, pattern: Box<Expr> },
    IsNull(Box<Expr>),
    IsNotNull(Box<Expr>),

    // ── Conditional ──
    Case { when_then: Vec<(Expr, Expr)>, else_expr: Option<Box<Expr>> },
    Coalesce(Vec<Expr>),
    NullIf { expr: Box<Expr>, null_expr: Box<Expr> },

    // ── Functions ──
    FunctionCall { name: String, args: Vec<Expr> },
    DateTrunc { grain: Grain, expr: Box<Expr> },

    // ── Sugar (resolved during planning) ──
    Guard { condition: Box<Expr>, expr: Box<Expr> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnRef {
    pub name: String,
    pub qualifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Integer(i64),
    Float(f64),
    String(String),
    Boolean(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityRef {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregateExpr {
    pub function: Aggregation,
    pub expr: Box<Expr>,
    pub distinct: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Aggregation {
    Sum, Avg, Count, CountDistinct, Min, Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinaryOp {
    // Arithmetic
    Add, Subtract, Multiply, Divide, SafeDivide,
    // Comparison
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    // Logical
    And, Or,
}
```

### 3.2 Key design decisions

| Decision | Rationale |
|----------|-----------|
| `Aggregate` is a typed enum variant, not `FunctionCall` | Eliminates string-based dispatch in emitters. `SUM`, `COUNT` etc. are known at compile time. |
| `BinaryOp` enum covers arithmetic + comparison + logical | Same approach as current `ir::BinaryOp`. Collapses `core`'s 14 separate variants into one. |
| `Literal` preserves `i64` vs `f64` distinction | Fixes the current IR's lossy `Number(f64)` which can't represent `i64::MAX` precisely. |
| `EntityRef` remains as a variant | It's a semantic-layer concept resolved during planning. After resolution, it becomes `Column`. No IR needs it. |
| `Guard` remains as sugar | Resolved to `Case { when_then, else_expr: Some(Null) }` during planning. Could also be kept in IR for traceability. |
| `FunctionCall` is the escape hatch | For functions not in the typed set (user-defined, dialect-specific). Aggregations are NOT routed through this. |
| `Grain` stays in `Expr::DateTrunc` | Typed grain (from `semstrait-core::Grain`) is better than the current IR's `String` grain. |

### 3.3 What `expr_lower.rs` becomes

The 440-line lowering module collapses to ~80 lines that do only:

1. **Resolve names**: `EntityRef("revenue")` → `Column { name: "amount" }` via `column_mapping`
2. **Resolve columns**: `Column { name: "revenue" }` → `Column { name: "amount" }` via `column_mapping`
3. **Expand sugar**: `Guard { cond, expr }` → `Case { [(cond, expr)], else: Null }`
4. **Flatten LogicalExpr**: kept only if `core::DslExpr` retains `And(LogicalExpr { exprs: Vec })` shape — but unified `Expr` uses `BinaryOp::And` directly, so YAML parsing would emit nested `BinaryOp` instead.

The mechanical type-mapping (`core::Add(BinaryExpr)` → `ir::BinaryOp { op: Add }`) **disappears entirely**.

### 3.4 Substrait serialization as a trait

```rust
// semstrait-ir/src/substrait_bridge.rs

/// Trait for types that can be serialized to/from Substrait protobuf.
pub trait SubstraitSerializable {
    fn to_substrait(&self, schema: &Schema) -> Result<substrait::proto::Expression, ConvertError>;
    fn from_substrait(expr: &substrait::proto::Expression, schema: &Schema) -> Result<Self, ConvertError>
    where Self: Sized;
}

impl SubstraitSerializable for Expr {
    fn to_substrait(&self, schema: &Schema) -> Result<proto::Expression, ConvertError> {
        match self {
            Expr::Column(col) => { /* ordinal lookup via schema */ }
            Expr::Literal(lit) => { /* direct mapping, i64 → I64, f64 → Fp64 */ }
            Expr::Aggregate(agg) => { /* aggregate function reference */ }
            Expr::BinaryOp { left, op, right } => { /* recursive */ }
            // ...
        }
    }
}
```

This replaces `ExprConverter` (a struct with `&Schema`) with a trait impl. The schema is passed per-call rather than held as a reference — cleaner lifetime management.

### 3.5 SQL emission uses `Expr` directly

```rust
// semstrait-sql — both expr_renderer.rs and polyglot/expr_builder.rs

// Before: match on ir::DslExpr { Column, Number, StringLit, BinaryOp, FunctionCall, ... }
// After:  match on Expr     { Column, Literal, BinaryOp, Aggregate, FunctionCall, ... }
```

The emitters gain:
- **Typed aggregation dispatch** — `Aggregate(AggregateExpr { function: Sum, .. })` instead of pattern-matching `FunctionCall { name: "SUM" }`
- **Precise literals** — `Literal::Integer(42)` emits `42`, `Literal::Float(3.14)` emits `3.14` (no `42.0` artifacts)
- **No string-based function name interpretation** for standard SQL aggregations

### 3.6 PlanNode uses unified `Expr`

```rust
// semstrait-ir/src/plan_node.rs

pub struct FilterNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    pub predicate: Expr,         // was: DslExpr (ir-local)
}

pub struct AggNode {
    pub meta: NodeMeta,
    pub input: Box<PlanNode>,
    pub group_by: Vec<Expr>,     // was: Vec<DslExpr>
    pub aggregates: Vec<AggregateMeasure>,
}

pub struct AggregateMeasure {
    pub function: Aggregation,   // was: in ir::plan_node, now from core::expr
    pub expr: Expr,
    pub distinct: bool,
}
```

---

## 4. Migration Plan

### Phase 1: Define `Expr` in `semstrait-core` (new module, parallel to existing)
- Create `semstrait-core/src/expr.rs` with unified `Expr`, `BinaryOp`, `Aggregation`, `Literal`, `ColumnRef`
- Add convenience constructors mirroring current `core::DslExpr` API
- Add serde support with same tag-based format

### Phase 2: Migrate `semstrait-ir` to use `core::Expr`
- Replace `ir::plan_node::DslExpr` with `use semstrait_core::Expr`
- Replace `ir::plan_node::BinaryOp` with `use semstrait_core::BinaryOp`
- Replace `ir::plan_node::Aggregation` with `use semstrait_core::Aggregation`
- Move `AggregateMeasure` to `semstrait-core` (or keep in IR, importing `Expr` from core)
- Update `ExprConverter` to work with unified `Expr` — or replace with `SubstraitSerializable` trait impl
- Delete `ir::plan_node::DslExpr` and `ir::plan_node::BinaryOp`

### Phase 3: Simplify `expr_lower.rs`
- Rewrite to only do: name resolution + EntityRef resolution + Guard expansion
- No more type mapping — `Expr` flows through unchanged
- Aggregate extraction (`extract_aggregates`) stays, but works on `Expr` directly

### Phase 4: Migrate YAML parsing (`semstrait-model` / `semstrait-manifest`)
- Update `steps.rs` and `compiled.rs` to emit `Expr` instead of `core::DslExpr`
- Adapt serde tags if needed (backward-compatible YAML format)

### Phase 5: Update downstream consumers
- `semstrait-sql/expr_renderer.rs` — match on `Expr` variants
- `semstrait-sql/polyglot/expr_builder.rs` — match on `Expr` variants
- `semstrait-connectors` — no direct expr dependency, transparent

### Phase 6: Remove old `core::DslExpr`
- Delete `semstrait-core/src/dsl_expr.rs`
- Update re-exports in `semstrait-core/src/lib.rs`
- Update `semstrait-ir/src/lib.rs` re-exports

---

## 5. What Changes Per Crate

| Crate | Change | Risk |
|-------|--------|------|
| `semstrait-core` | New `expr.rs` module; eventually delete `dsl_expr.rs` | Low — additive first |
| `semstrait-ir` | Delete local `DslExpr`+`BinaryOp`; import from core; update `ExprConverter` | Medium — many files reference these |
| `semstrait-planner` | Simplify `expr_lower.rs` from ~440 to ~80 lines | Medium — core logic change |
| `semstrait-sql` | Update `match` arms in renderers/builders | Low — same structure, different enum paths |
| `semstrait-model` | Update expression parsing to emit `Expr` | Low — constructor API stays similar |
| `semstrait-manifest` | Update `CompiledMeasure.expr` type | Low — type change only |

---

## 6. What Does NOT Change

- `PlanNode` enum (Scan, Filter, Project, Aggregate, Join, Union, Sort, Fetch) — unchanged
- `LogicalPlan` wrapper — unchanged
- YAML format — unchanged (serde tags adapted)
- Substrait serialization semantics — unchanged (just different code organization)
- SQL output — unchanged (same expressions, same rendering)
- Aggregate extraction in planner — stays, operates on unified `Expr`

---

## 7. Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| `semstrait-core` grows heavier | `Expr` is a pure data structure with no logic beyond constructors. Same weight class as current `DslExpr`. |
| `Aggregation` in core means core knows about aggregation | It already does — `core::DslExpr` has `Sum`, `Count`, etc. We're just making it a named enum. |
| YAML serde compatibility | Use `#[serde(tag = "type")]` with same tag names. Can add `#[serde(alias = "...")]` for old names. |
| Large PR | Phased approach. Phase 1 is additive (no deletions). Each phase is independently testable. |

---

## 8. Before/After Comparison

### Expression lowering (core → IR)

**Before** (440 lines, mechanical conversion):
```rust
CoreExpr::Add(bin) => lower_binary(bin, BinaryOp::Add, column_mapping),
CoreExpr::Subtract(bin) => lower_binary(bin, BinaryOp::Subtract, column_mapping),
// ... 20+ more variants, each creating a new ir::DslExpr
```

**After** (~80 lines, only semantic operations):
```rust
Expr::EntityRef(e) => Ok(Expr::Column(resolve_name(&e.name, column_mapping))),
Expr::Guard { condition, expr } => Ok(Expr::Case { ... }),
Expr::Column(col) => Ok(Expr::Column(resolve_column(col, column_mapping))),
_ => Ok(expr.clone()), // everything else passes through
```

### SQL emission

**Before** (string-based function dispatch):
```rust
ir::DslExpr::FunctionCall { name, args, distinct } => {
    match name.as_str() {
        "SUM" => format!("SUM({})", render(args[0])),
        "COUNT" if distinct => format!("COUNT(DISTINCT {})", render(args[0])),
        // ...
    }
}
```

**After** (typed dispatch):
```rust
Expr::Aggregate(agg) => {
    match agg.function {
        Aggregation::Sum => format!("SUM({})", render(&agg.expr)),
        Aggregation::Count if agg.distinct => format!("COUNT(DISTINCT {})", render(&agg.expr)),
        // ...
    }
}
```

---

## 9. Verdict

The dual-expression architecture was a reasonable initial design choice (DL-020) that kept `semstrait-core` and `semstrait-ir` loosely coupled. But in practice:

- The coupling is already tight (IR re-exports core types, planner imports both)
- The conversion is purely mechanical (no optimization, no type changes except precision loss)
- Every downstream consumer must re-interpret string-typed functions
- The `expr_lower.rs` module is the single largest source of boilerplate in the planner

Unifying to a single `Expr` type eliminates ~360 lines of conversion code, improves type safety for aggregation dispatch, preserves integer precision, and makes the expression pipeline easier to reason about.
