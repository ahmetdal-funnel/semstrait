# Computed Expressions — Design & Implementation Plan

**Status:** Design approved | **Date:** 2026-04-07
**Approach:** Option C — Hybrid (inline DSL + declarative YAML)
**Branch:** `feature/base-semastrait-dev`
**Prerequisite:** Phase F (Declarative Measures) complete, DataType logical refactor complete
**Engine scope:** DataFusion, DuckDB, Spark. New engines can be added by implementing `EngineAdapter` + `SqlDialect`.

---

## 1. Overview

Computed expressions extend semstrait's semantic model to support **derived dimensions, measures, and metrics** defined via expression trees. The hybrid approach keeps simple arithmetic in inline string DSL (`expr: "cost / clicks"`) while routing complex expressions (CASE/WHEN, IN-list, REGEXP, function compositions) through declarative YAML blocks that map directly to the `Expr` enum.

**Core principle:** Expressions reference **semantic names**. Physical binding is resolved at compile time via `column_mapping`. The planner emits aliasing projections (`physical_col AS semantic_name`) to harmonize naming before expression evaluation. All expression trees are fully resolved and validated during manifest compilation — no per-query re-parsing.

---

## 2. Expression Paths

### 2.1 Inline String DSL (existing, measures/metrics only)

Simple arithmetic and entity references. Parsed by `parse_expr()` in `steps.rs:1845-1900`.

```yaml
measures:
  - name: cpc
    agg: avg
    expr: "cost / clicks"

metrics:
  - name: profit_margin
    expr: "{{ revenue }} - {{ cost }}"
```

**Supported operations:** `+`, `-`, `*`, `/`, `//` (safe divide), entity refs (`{{ name }}`), bare identifiers, numeric literals.

**Boundary:** Any expression requiring CASE/WHEN, IN, LIKE, REGEXP, function calls, or nested conditionals MUST use a declarative block. The inline DSL will NOT be extended with SQL-like syntax.

### 2.2 Declarative YAML Expression Block (new)

Structured YAML that maps 1:1 to `Expr` variants. Serde discriminates by type: `String` value routes to inline DSL parser; `Map` value deserializes directly to `Expr`.

```yaml
dimensions:
  - name: market
    type: categorical
    expr:
      case:
        when:
          - condition:
              in_list: { expr: dataset_name, list: ["adwords", "facebook", "bing", "tiktok", "klaviyo"] }
            then:
              case:
                when:
                  - condition:
                      like: { expr: campaign, pattern: "UK_%" }
                    then: "GB"
                else:
                  upper:
                    regexp_extract: { expr: campaign, pattern: "^([A-Z]{2})_", group: 1 }
          - condition:
              eq: [dataset_name, "impact"]
            then:
              regexp_extract: { expr: campaign, pattern: "^Alpinestars ([A-Z]{2})", group: 1 }
        else: ""
```

**Applies to:** dimensions (new), measures, metrics. Both at shared level and kind level (per-dataset overrides).

### 2.3 Serde Discrimination

```rust
/// Expression source in YAML — discriminated by serde type.
#[derive(Deserialize)]
#[serde(untagged)]
pub enum ExprSource {
    /// Simple string DSL: "cost / clicks", "{{ revenue }}", "amount"
    Inline(String),
    /// Declarative tree: maps directly to Expr
    Declarative(ExprBlock),
}
```

The `ExprBlock` is a YAML-native representation that deserializes into `Expr` variants. Each YAML key corresponds to an `Expr` variant name (snake_case). Leaf values (bare strings) are resolved as semantic name references.

---

## 3. Semantic Name Resolution & Physical Binding

### 3.1 Derivation Rules

Expressions reference **semantic names** (dimension names, measure names). Each semantic name must have a physical binding via `column_mapping`. Resolution order:

1. **Semantic name lookup** — search `column_mapping.physical` for the name
2. **Fallback** — if not found in physical mapping, search for `semantic_name` as a physical column name (identity mapping case)
3. **Error** — if neither resolves, emit `CompileError::UnresolvedExprRef`

This resolution happens at **compile time** (step 8, `compile_exprs`). The compiled `Expr` tree stores `EntityRef(name)` nodes that the planner resolves to `Column(physical_name)` via `expr_lower`.

### 3.2 Aliasing Projection (Planner)

The planner's `build_binding_plan()` pipeline (shared.rs:316-471) already implements:

```
ScanNode (physical columns) -> AggNode (group_by=physical, output=semantic) -> ProjectNode (semantic names)
```

For computed dimensions, the planner inserts expressions into the **ProjectNode**. The AggNode outputs semantic names for regular dimensions; the ProjectNode can reference those semantic names in computed dimension expressions.

**Injection point:** After `AggNode`, computed dimension expressions are added as additional `ProjectNode` expressions. Since AggNode already aliases `physical_col -> semantic_name` in its output schema, computed expressions naturally reference semantic names.

### 3.3 Complex Bindings (Multi-Physical-to-One-Semantic)

When a computed expression references multiple physical columns that map to the same semantic concept across datasets (e.g., `campaign_name` in one source, `ad_campaign` in another), the resolution uses the dataset's own `column_mapping`:

```yaml
# Kind-level: shared dimension
dimensions:
  - name: market
    expr:
      case:
        when:
          - condition: { like: { expr: campaign, pattern: "UK_%" } }
            then: "GB"

# Dataset A: campaign -> campaign_name
datasets:
  - name: google_ads
    column_mapping:
      campaign: campaign_name    # physical binding for this dataset
      cost: spend

# Dataset B: campaign -> ad_campaign
  - name: facebook_ads
    column_mapping:
      campaign: ad_campaign      # different physical binding
```

Each dataset resolves `campaign` in the expression to its own physical column. The expression tree is **shared** but physical resolution is **per-dataset** — this is already how `expr_lower::resolve_name_physical()` works (O(1) IndexMap lookup per binding).

### 3.4 Compound Physical Bindings

When a single semantic name requires multiple physical columns (e.g., a date split across `year`, `month`, `day` columns), use structured column_mapping:

```yaml
column_mapping:
  order_date:
    anchored:
      year: order_year
      month: order_month
      day: order_day
```

The expression tree references `order_date_year`, `order_date_month`, `order_date_day` — the semantic name acts as namespace prefix. The planner's ScanNode projects each component as `order_year AS order_date_year`, etc.

**Resolution pattern:** `<semantic_name>_<component>` where component names come from the anchored mapping keys. The `ResolvedColumnMapping.anchored` HashMap already stores this structure.

---

## 4. Expression Operators

### 4.1 Current Expr Variants (18)

| Variant | Status | Inline DSL | Declarative | Substrait | SQL |
|---------|--------|------------|-------------|-----------|-----|
| Column | existing | implicit | `column: name` | FieldReference | `"name"` |
| Literal | existing | numeric only | bare string/number | Literal | value |
| EntityRef | existing | `{{ name }}` | bare string | resolved before emit | resolved |
| Aggregate | existing | `SUM(col)` | `aggregate: {fn, expr}` | AggregateRel | `SUM(expr)` |
| BinaryOp | existing | `a + b` | `add: [a, b]` | ScalarFunction | `(a + b)` |
| Negate | existing | n/a | `negate: expr` | ScalarFunction | `-(expr)` |
| Not | existing | n/a | `not: expr` | fn_anchor:205 | `NOT (expr)` |
| Case | existing | n/a | `case: {when, else}` | IfThen | `CASE WHEN...END` |
| InList | existing | n/a | `in_list: {expr, list}` | fn_anchor:206 | `expr IN (...)` |
| Between | existing | n/a | `between: {expr, low, high}` | fn_anchor:207 | `BETWEEN` |
| Like | existing | n/a | `like: {expr, pattern}` | fn_anchor:208 | `LIKE` |
| IsNull | existing | n/a | `is_null: expr` | fn_anchor:202 | `IS NULL` |
| IsNotNull | existing | n/a | `is_not_null: expr` | fn_anchor:203 | `IS NOT NULL` |
| Coalesce | existing | n/a | `coalesce: [exprs]` | fn_anchor:204 | `COALESCE(...)` |
| NullIf | existing | n/a | `null_if: {expr, null_expr}` | fn_anchor:209 | `NULLIF(...)` |
| DateTrunc | existing | n/a | `date_trunc: {grain, expr}` | fn_anchor:210 | `DATE_TRUNC(...)` |
| FunctionCall | existing | n/a | **internal only** | SQL-only | `name(args)` |
| Guard | existing | n/a | `guard: {condition, expr}` | resolved to Case | resolved |

### 4.2 New Expr Variants (3)

These require dedicated variants because they have **cross-engine semantic differences** that the adapter must handle:

#### ILike

Case-insensitive LIKE. All three target engines support it natively.

```rust
/// Case-insensitive LIKE.
ILike(LikeExpr),  // reuses existing LikeExpr struct
```

| Engine | SQL | Substrait |
|--------|-----|-----------|
| DataFusion | `col ILIKE pattern` | `like(input, match, CASE_INSENSITIVE)` |
| DuckDB | `col ILIKE pattern` | n/a (SQL path) |
| Spark | `col ILIKE pattern` (3.3+) | n/a (SQL path) |

YAML:
```yaml
ilike: { expr: campaign, pattern: "uk_%" }
```

#### RegexpMatch

Boolean regex test. Cross-engine function names and match semantics diverge.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegexpExpr {
    pub expr: Box<Expr>,
    pub pattern: Box<Expr>,
    /// true = full-string match (Spark native), false = substring match (DF/DuckDB native).
    /// Adapter adds anchors (^...$) or wraps (.*pat.*) as needed.
    pub full_match: bool,
}

/// Boolean regex test.
RegexpMatch(RegexpExpr),
```

| Engine | Substring match | Full match | Function |
|--------|----------------|------------|----------|
| DataFusion | native | adds `^...$` | `regexp_like(col, pat)` |
| DuckDB | native | adds `^...$` | `regexp_matches(col, pat)` |
| Spark | wraps `.*pat.*` | native | `col RLIKE pat` |

YAML:
```yaml
regexp_match: { expr: campaign, pattern: "^[A-Z]{2}_", full_match: false }
```

#### RegexpExtract

Regex capture group extraction. Function names and array indexing differ across engines.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegexpExtractExpr {
    pub expr: Box<Expr>,
    pub pattern: Box<Expr>,
    /// 1-based capture group index (0 = entire match).
    pub group_index: u32,
}

/// Regex capture group extraction.
RegexpExtract(RegexpExtractExpr),
```

| Engine | Function | Notes |
|--------|----------|-------|
| DataFusion | `regexp_match(col, pat)[group]` | Returns array, index into it |
| DuckDB | `regexp_extract(col, pat, group)` | Direct group parameter |
| Spark | `regexp_extract(col, pat, group)` | Full-match semantics on pattern |

YAML:
```yaml
regexp_extract: { expr: campaign, pattern: "^([A-Z]{2})_", group: 1 }
```

### 4.3 Standard Functions — Direct Declarative Keys

Standard ANSI SQL functions get **first-class declarative YAML keys** for clean user-facing syntax. Internally, these desugar to `Expr::FunctionCall` during YAML->Expr conversion — the `FunctionCall` variant remains the internal representation, but users never write `function_call:` wrappers.

#### String Functions

```yaml
# Direct declarative syntax (user-facing)
upper: campaign                              # UPPER(campaign)
lower: campaign                              # LOWER(campaign)
trim: name                                   # TRIM(name)
ltrim: name                                  # LTRIM(name)
rtrim: name                                  # RTRIM(name)
length: name                                 # LENGTH(name)
concat: [first_name, " ", last_name]         # CONCAT(first_name, ' ', last_name)
replace: { expr: col, old: "foo", new: "bar" } # REPLACE(col, 'foo', 'bar')
substring: { expr: col, start: 1, length: 3 }  # SUBSTRING(col, 1, 3)
left: { expr: col, length: 2 }              # LEFT(col, 2)
right: { expr: col, length: 2 }             # RIGHT(col, 2)
lpad: { expr: col, length: 10, fill: "0" }  # LPAD(col, 10, '0')
rpad: { expr: col, length: 10, fill: " " }  # RPAD(col, 10, ' ')
```

#### Math Functions

```yaml
abs: value                                   # ABS(value)
ceil: value                                  # CEIL(value)
floor: value                                 # FLOOR(value)
round: { expr: value, scale: 2 }            # ROUND(value, 2)
power: { base: col, exponent: 2 }           # POWER(col, 2)
sqrt: value                                  # SQRT(value)
mod: [a, b]                                  # MOD(a, b)
```

#### Date Functions

```yaml
current_date: {}                             # CURRENT_DATE
current_timestamp: {}                        # CURRENT_TIMESTAMP
date_add: { expr: date_col, days: 7 }       # DATE_ADD(date_col, 7)
date_diff: { start: start_date, end: end_date } # DATEDIFF(end, start)
extract: { part: "year", expr: date_col }    # EXTRACT(YEAR FROM date_col)
```

#### Conditional Functions

```yaml
if: [condition, then_val, else_val]          # IF(condition, then, else) -> desugars to CASE
greatest: [a, b, c]                          # GREATEST(a, b, c)
least: [a, b, c]                             # LEAST(a, b, c)
cast: { expr: col, to: "integer" }           # CAST(col AS INTEGER)
```

#### Internal FunctionCall Escape Hatch

`Expr::FunctionCall` remains available internally for:
- Adapter-level SQL emission of standard functions (renders as `name(args)`)
- Substrait serialization (dynamic anchor allocation for standard functions)
- Engine-specific functions not in the standard registry (advanced use case)

Users do NOT interact with `FunctionCall` in declarative YAML. The YAML deserializer maps each direct key (e.g., `upper:`) to `Expr::FunctionCall { name: "upper", args: [...] }` automatically.

### 4.4 Function Registry & Extensibility

Functions are categorized into tiers for validation and adapter handling:

| Tier | Description | Validation | Adapter Handling |
|------|-------------|------------|-----------------|
| **Built-in** | Dedicated Expr variants (Case, Like, ILike, RegexpMatch, etc.) | Structural (type-checked at compile) | Per-variant dialect mapping |
| **Standard** | ANSI SQL functions with direct YAML keys (UPPER, ROUND, etc.) | Name + arity validated against registry | Pass-through (universal syntax) |
| **Engine-specific** | Advanced functions not in standard set | Warning at compile, error if adapter doesn't recognize | Adapter must map or reject |

**Extensibility points:**

1. **Declarative block level** — New YAML keys are added by extending `ExprBlock` deserialization. Each key maps to an `Expr` variant or desugars to `FunctionCall`.

2. **Adapter level** — Each adapter handles known function names in SQL/Substrait emission. Standard functions pass through as `name(args)`. Unknown names produce `AdaptError`.

3. **Function registry** (compile time) — A `FunctionRegistry` validates function names and arity during step 8. All standard functions are pre-registered.

```rust
pub struct FunctionRegistry {
    functions: HashMap<String, FunctionSpec>,
}

pub struct FunctionSpec {
    pub name: String,
    pub min_args: usize,
    pub max_args: Option<usize>,  // None = variadic
    pub category: FunctionCategory,
}

pub enum FunctionCategory {
    Standard,       // ANSI SQL — all engines (DF, DuckDB, Spark)
    EngineSpecific, // requires adapter mapping
}
```

### 4.5 Substrait Anchor Allocation

New anchors for new variants (continuing from existing allocation):

| Anchor | Function | URI Extension |
|--------|----------|---------------|
| 208 | `like` | existing (boolean/3) |
| 211 | `ilike` | boolean/3 |
| 212 | `regexp_match` | boolean/3 |
| 213 | `regexp_extract` | string/5 (new URI) |

Standard FunctionCall functions use **dynamic anchor allocation** starting at 400+. The serializer maintains a name->anchor map built during plan traversal.

---

## 5. Cross-Engine Pattern Matching

### 5.1 LIKE / ILIKE — Universal

All three engines (DataFusion, DuckDB, Spark) support `LIKE` and `ILIKE` natively. Minor divergence: Spark defaults to `\` as escape character. Cross-engine generation should always emit explicit `ESCAPE` when escaping is needed.

### 5.2 REGEXP — Biggest Divergence

| Aspect | DataFusion | DuckDB | Spark |
|--------|------------|--------|-------|
| **Boolean test** | `regexp_like(col, pat)` | `regexp_matches(col, pat)` | `col RLIKE pat` |
| **Extract** | `regexp_match(col, pat)[idx]` | `regexp_extract(col, pat, grp)` | `regexp_extract(col, pat, grp)` |
| **Match semantics** | Substring | Substring | Full string |
| **Regex flavor** | Rust `regex` | RE2 | Java `java.util.regex` |

**Adapter strategy:**
- `RegexpMatch { full_match: false }` -> DataFusion/DuckDB emit as-is, Spark wraps pattern in `.*...*`
- `RegexpMatch { full_match: true }` -> DataFusion/DuckDB add `^...$` anchors, Spark emits as-is
- `RegexpExtract` -> per-engine function name/syntax mapping

**Regex pattern restrictions** (enforced at compile time):
- No lookahead/lookbehind (not in Rust regex or RE2)
- No backreferences (not in Rust regex or RE2)
- Named groups must use `(?P<name>...)` syntax (compatible across all three)

### 5.3 Functions Not Supported (v1)

| Pattern | Reason |
|---------|--------|
| `SIMILAR TO` | DuckDB only — not portable |
| `GLOB` | DuckDB only — lower to LIKE or REGEXP |
| Window functions | Deferred — focus on inline data modification |
| `PARTITION BY` | Deferred |

---

## 6. Declarative Block Schema

### 6.1 ExprBlock YAML Schema

Each YAML key maps to an `Expr` variant or desugars to `FunctionCall`. Leaf strings resolve to semantic name references (`EntityRef`). Quoted strings in lists resolve to `Literal::String`.

```yaml
# ── Leaf nodes ───────────────────────────────────────────────
column: "name"                      # Expr::Column
literal: 42                         # Expr::Literal (auto-typed)
entity_ref: "measure_name"          # Expr::EntityRef (explicit)

# ── Arithmetic (BinaryOp) ───────────────────────────────────
add: [left, right]
subtract: [left, right]
multiply: [left, right]
divide: [left, right]
safe_divide: [left, right]

# ── Comparison (BinaryOp) ───────────────────────────────────
eq: [left, right]
not_eq: [left, right]
lt: [left, right]
gt: [left, right]
lte: [left, right]
gte: [left, right]

# ── Logical ──────────────────────────────────────────────────
and: [left, right]
or: [left, right]
not: expr
negate: expr

# ── Conditional ──────────────────────────────────────────────
case:
  when:
    - condition: <expr>
      then: <expr>
  else: <expr>

coalesce: [expr1, expr2, ...]
null_if: { expr: <expr>, null_expr: <expr> }
if: [condition, then_val, else_val]  # sugar -> desugars to Case
greatest: [a, b, c]
least: [a, b, c]

# ── Predicates ───────────────────────────────────────────────
in_list: { expr: <expr>, list: [val1, val2, ...] }
not_in_list: { expr: <expr>, list: [val1, val2, ...] }
between: { expr: <expr>, low: <expr>, high: <expr> }
like: { expr: <expr>, pattern: "pattern" }
ilike: { expr: <expr>, pattern: "pattern" }
is_null: <expr>
is_not_null: <expr>

# ── Pattern matching ─────────────────────────────────────────
regexp_match: { expr: <expr>, pattern: "pat", full_match: false }
regexp_extract: { expr: <expr>, pattern: "pat", group: 1 }

# ── String functions ─────────────────────────────────────────
upper: <expr>
lower: <expr>
trim: <expr>
ltrim: <expr>
rtrim: <expr>
length: <expr>
concat: [<expr>, ...]
replace: { expr: <expr>, old: "old", new: "new" }
substring: { expr: <expr>, start: 1, length: 3 }
left: { expr: <expr>, length: 2 }
right: { expr: <expr>, length: 2 }
lpad: { expr: <expr>, length: 10, fill: "0" }
rpad: { expr: <expr>, length: 10, fill: " " }

# ── Math functions ───────────────────────────────────────────
abs: <expr>
ceil: <expr>
floor: <expr>
round: { expr: <expr>, scale: 2 }
power: { base: <expr>, exponent: 2 }
sqrt: <expr>
mod: [<expr>, <expr>]

# ── Date functions ───────────────────────────────────────────
date_trunc: { grain: "month", expr: <expr> }
current_date: {}
current_timestamp: {}
date_add: { expr: <expr>, days: 7 }
date_diff: { start: <expr>, end: <expr> }
extract: { part: "year", expr: <expr> }

# ── Type conversion ──────────────────────────────────────────
cast: { expr: <expr>, to: "integer" }

# ── Guard (sugar) ────────────────────────────────────────────
guard: { condition: <expr>, expr: <expr> }
```

### 6.2 Leaf Resolution Rules

Within a declarative block, bare strings are resolved in order:

1. If the string matches a known semantic name (dimension, measure, metric) -> `EntityRef`
2. If the string is a quoted literal (in a list context) -> `Literal::String`
3. If the string is numeric -> `Literal::Integer` or `Literal::Float`
4. If the string is `"true"` / `"false"` -> `Literal::Boolean`
5. If the string is `"null"` -> `Literal::Null`
6. Otherwise -> `EntityRef` (resolved at compile time, error if unresolvable)

### 6.3 Where Expressions Apply

| Location | Current | After Implementation |
|----------|---------|---------------------|
| Measure `expr:` | Inline DSL -> Expr | Inline DSL (simple) OR declarative block (complex) |
| Metric `expr:` | Inline DSL -> Expr | Inline DSL (simple) OR declarative block (complex) |
| **Dimension `expr:`** | **Not supported** | **New -- computed dimensions** (declarative block) |
| **Kind-level dimension `expr:`** | **Not supported** | **New -- per-dataset override on shared dims** |
| Filter conditions | Structured `CompiledFilter` | Unchanged (already declarative) |
| `column_mapping` value | String (column name) | Unchanged -- physical binding, not computation |

**Important distinction:** `column_mapping` is pure physical binding (semantic -> physical column name). Computed expressions belong on the **dimension/measure/metric definition**. The mapping says *where* the data is; the expression says *how* to derive it.

---

## 7. Compilation Pipeline Changes

### 7.1 Step 8: compile_exprs (Enhanced)

Current step 8 parses inline DSL strings for measures/metrics. Enhanced step 8:

```
Step 8: compile_exprs
  8.1  Parse measure expressions (inline DSL or declarative block)
  8.2  Parse metric expressions (inline DSL or declarative block)
  8.3  Parse dimension expressions (declarative block only) <- NEW
  8.4  Validate all EntityRef nodes against kind interface
  8.5  Validate FunctionCall names against FunctionRegistry
  8.6  Validate physical binding completeness per dataset
  8.7  Store compiled Expr trees in CompiledDimension/CompiledMeasure/CompiledMetric
```

### 7.2 Binding Validation (Step 8.6)

For each expression tree, walk all `EntityRef` nodes and verify:

1. The referenced name exists in the `KindInterface` (as dimension, measure, or metric)
2. For each `DatasetBinding`, the referenced name has a physical mapping in `column_mapping`
3. For unionset kinds, all branches must have physical mappings for all referenced names

```rust
fn validate_expr_bindings(
    expr: &Expr,
    iface: &KindInterface,
    bindings: &[DatasetBinding],
) -> Result<(), CompileError> {
    let refs = collect_entity_refs(expr);
    for ref_name in &refs {
        // Must exist in kind interface
        if !iface.has_semantic_name(ref_name) {
            return Err(CompileError::UnresolvedExprRef { name: ref_name.clone() });
        }
        // Must have physical mapping in every binding
        for binding in bindings {
            if !binding.column_mapping.physical.contains_key(ref_name)
                && !binding.column_mapping.literals.contains_key(ref_name) {
                return Err(CompileError::MissingPhysicalBinding {
                    semantic: ref_name.clone(),
                    dataset: binding.dataset_name.clone(),
                });
            }
        }
    }
    Ok(())
}
```

### 7.3 CompiledDimension Changes

```rust
pub struct CompiledDimension {
    pub name: String,
    pub data_type: DataType,
    pub dim_type: DimensionType,
    pub expr: Option<Expr>,       // NEW -- None = regular column, Some = computed
    pub expr_source: Option<String>, // NEW -- original YAML for debugging
}
```

When `expr` is `Some`, this dimension is **computed** -- the planner must emit it as a ProjectNode expression rather than a ScanNode column.

---

## 8. Planner Changes

### 8.1 Computed Dimension Detection

In `build_binding_plan()` (shared.rs:316-471), partition requested dimensions into:

- **Regular dimensions** -- have direct physical mapping, included in ScanNode + AggNode group_by
- **Literal dimensions** -- mapped to constant values (existing)
- **Metadata dimensions** -- extracted from source metadata (existing)
- **Computed dimensions** -- have `expr: Some(...)`, emitted in ProjectNode after aggregation

```rust
let (computed_dims, regular_dims): (Vec<_>, Vec<_>) = request.dimensions
    .iter()
    .partition(|d| iface.dimensions.get(*d).and_then(|cd| cd.expr.as_ref()).is_some());
```

### 8.2 Expression Lowering for Computed Dimensions

Computed dimension expressions reference semantic names. The planner resolves these to physical names per-dataset using `expr_lower`:

```rust
fn lower_computed_dimension(
    expr: &Expr,
    physical_mapping: &IndexMap<String, String>,
) -> Result<Expr, PlannerError> {
    expr.transform(&|e| {
        if let Expr::EntityRef(ref entity) = e {
            if let Some(physical) = physical_mapping.get(&entity.name) {
                return Ok(Some(Expr::column(physical.clone())));
            }
        }
        Ok(None)
    })
}
```

**Critical detail:** Computed dimensions reference semantic names that AggNode already outputs in its schema. So for computed dimensions that operate **post-aggregation** (the common case -- e.g., deriving `market` from `campaign`), the expression should reference the AggNode output names (which are semantic). The transform resolves `EntityRef` -> `Column` with the semantic name as the column name (not the physical name), since AggNode's output schema uses semantic names.

For computed dimensions that need to operate **pre-aggregation** (rare -- e.g., computing a value that then gets grouped), the expression is lowered to physical names and injected as an additional ScanNode projection.

### 8.3 Injection into Plan Tree

```
ScanNode (physical columns + computed-pre-agg projections)
    |
    v
AggNode (group_by = regular dims + computed-pre-agg dims, output = semantic names)
    |
    v
ProjectNode (regular dims from AggNode + computed-post-agg exprs + measures)
```

The ProjectNode already handles measure post-aggregation expressions (`lowered.post_agg_expr`). Computed dimensions are added to the same ProjectNode as additional expressions referencing AggNode output columns.

### 8.4 Unionset Handling

For unionsets, each branch resolves the computed dimension expression against its own dataset's column_mapping. The expression tree is shared across branches but physical resolution is per-branch. The UnionNode schema includes the computed dimension with its semantic name.

---

## 9. Adapter Restructure: Absorb semstrait-sql

### 9.1 Motivation

Currently `semstrait-sql` is a separate crate consumed only by `semstrait-adapter` (primary) and `semstrait-api` (debug SQL, which can route through `adapter.debug_sql()`). The adapter IS the artifact generation boundary — it should own both SQL and Substrait emission. Merging eliminates an unnecessary crate hop.

### 9.2 Current Architecture

```
semstrait-api -> semstrait-adapter -> semstrait-sql -> semstrait-ir, semstrait-core
                                   -> semstrait-ir (Substrait serializer)
```

Each SQL adapter is a one-liner delegation:
```rust
// DuckDbAdapter::adapt()
let emitter = AnsiSqlEmitter::new(DuckDbDialect);
let sql = emitter.emit(plan)?;
Ok(PlanArtifact::Sql(sql))
```

### 9.3 New Architecture

```
semstrait-api -> semstrait-adapter -> semstrait-ir, semstrait-core
```

```
semstrait-adapter/
  src/
    lib.rs                  # EngineAdapter trait, re-exports
    traits.rs               # Core traits
    error.rs
    sql/                    # SQL emission (absorbed from semstrait-sql)
      mod.rs                # SqlEmitter trait, SqlDialect trait, exports
      emitter.rs            # AnsiSqlEmitter<D>
      expr_renderer.rs      # ExprSqlRenderer
      dialect.rs            # AnsiDialect, DuckDbDialect, SparkDialect
      polyglot/             # feature-gated polyglot transpilation
        mod.rs
        plan_builder.rs
        expr_builder.rs
        polyglot_emitter.rs
    engines/                # Per-engine adapters
      mod.rs
      datafusion.rs         # Substrait path (SubstraitSerializer)
      duckdb.rs             # SQL path (DuckDbDialect)
      spark.rs              # SQL path (SparkDialect / AnsiDialect)
    registry.rs             # FunctionRegistry (new, for computed expressions)
```

### 9.4 Key Changes

| Change | Detail |
|--------|--------|
| **Move all semstrait-sql code** | `emitter.rs`, `expr_renderer.rs`, `dialect.rs`, `polyglot/` -> `semstrait-adapter/src/sql/` |
| **Remove semstrait-sql crate** | Delete `crates/semstrait-sql/` directory and Cargo.toml entry |
| **Remove Trino** | `TrinoDialect`, `TrinoAdapter`, `TrinoConnector`, CLI `query-trino` — all removed (feature-gated, ~560 lines total). Trino referenced only in docs as extensibility example. |
| **Update dependencies** | Remove `semstrait-sql` from workspace Cargo.toml. Update `semstrait-api`, `semstrait` facade to drop `semstrait-sql` dep. |
| **Move tests** | `semstrait-sql/src/tests.rs` (74 tests) -> `semstrait-adapter/src/sql/tests.rs`. Remove Trino-specific tests. |
| **debug_sql() routing** | `semstrait-api` uses `adapter.debug_sql()` (already exists) instead of direct `SqlEmitter` calls. |

### 9.5 Trino Removal Scope

Trino is a substantive implementation (~560 lines) but outside v1 scope. Removed files/sections:

| Location | What | Lines |
|----------|------|-------|
| `semstrait-adapter/src/trino.rs` | TrinoAdapter + EngineProfile | 168 |
| `semstrait-connectors/src/trino.rs` | TrinoConnector (REST client, auth, pagination) | 341 |
| `semstrait-sql/src/dialect.rs` | TrinoDialect impl | 21 |
| `semstrait-sql/src/tests.rs` | Trino-specific tests | ~30 |
| `semstrait-api/src/cli.rs` | QueryTrino command + handler | ~40 |
| Feature flags | `trino` feature in 5 Cargo.toml files | — |

Trino can be re-added later by implementing `EngineAdapter` + `SqlDialect` in the new module structure.

### 9.6 New Expr -> SQL Mappings

After the merge, SQL emission for new variants lives in `semstrait-adapter/src/sql/expr_renderer.rs`:

| Expr | DataFusion (ANSI) | DuckDB | Spark |
|------|-------------------|--------|-------|
| `ILike` | `expr ILIKE pattern` | `expr ILIKE pattern` | `expr ILIKE pattern` |
| `RegexpMatch` | `regexp_like(expr, pat)` | `regexp_matches(expr, pat)` | `expr RLIKE pat` |
| `RegexpExtract` | `regexp_match(expr, pat)[grp]` | `regexp_extract(expr, pat, grp)` | `regexp_extract(expr, pat, grp)` |
| `FunctionCall` (standard) | `name(args)` | `name(args)` | `name(args)` |

### 9.7 Substrait Mappings

| Expr | Substrait | Anchor |
|------|-----------|--------|
| `ILike` | `ScalarFunction { fn_anchor: 211, args: [expr, pattern] }` | 211 |
| `RegexpMatch` | `ScalarFunction { fn_anchor: 212, args: [expr, pattern, full_match_lit] }` | 212 |
| `RegexpExtract` | `ScalarFunction { fn_anchor: 213, args: [expr, pattern, group_lit] }` | 213 |

Standard FunctionCall functions use **dynamic anchor allocation** starting at 400+.

---

## 10. Implementation Plan

### Phase G0: Crate Restructure (prerequisite)

**Goal:** Absorb `semstrait-sql` into `semstrait-adapter`, remove Trino.

1. Create `semstrait-adapter/src/sql/` module directory
2. Move `semstrait-sql/src/{emitter,expr_renderer,dialect,error}.rs` -> `sql/`
3. Move `semstrait-sql/src/polyglot/` -> `sql/polyglot/`
4. Move tests to `sql/tests.rs`, remove Trino tests
5. Create `semstrait-adapter/src/engines/` module, move adapter files
6. Remove `TrinoDialect` from `dialect.rs`, `TrinoAdapter` from engines
7. Remove `crates/semstrait-connectors/src/trino.rs` and `trino` feature
8. Remove `semstrait-api/src/cli.rs` QueryTrino command
9. Remove `trino` feature from all Cargo.toml files
10. Delete `crates/semstrait-sql/` directory
11. Update workspace Cargo.toml — remove semstrait-sql member
12. Update `semstrait-api` to use `adapter.debug_sql()` instead of direct SQL emitter
13. `cargo test --workspace` — all tests pass

### Phase G1: New Expr Variants & Infrastructure

**Files:** `semstrait-core/src/expr.rs`, `semstrait-ir/src/substrait/expr_converter.rs`, `semstrait-ir/src/substrait/serializer.rs`

1. Add `ILike(LikeExpr)`, `RegexpMatch(RegexpExpr)`, `RegexpExtract(RegexpExtractExpr)` to `Expr`
2. Add `RegexpExpr`, `RegexpExtractExpr` structs
3. Add convenience constructors: `Expr::ilike()`, `Expr::regexp_match()`, `Expr::regexp_extract()`
4. Extend `transform_children()` for new variants
5. Substrait: add anchors 211-213, `to_substrait`/`from_substrait` for new variants
6. Tests: serde roundtrip, Substrait roundtrip

### Phase G2: SQL Emission for New Variants

**Files:** `semstrait-adapter/src/sql/expr_renderer.rs`, `semstrait-adapter/src/sql/dialect.rs`

1. `ExprSqlRenderer::render()` — add ILike, RegexpMatch, RegexpExtract arms
2. `SqlDialect` trait — add `regexp_like()`, `regexp_extract()` for per-dialect syntax
3. `AnsiDialect` — DataFusion syntax (default)
4. `DuckDbDialect` — `regexp_matches`, `regexp_extract`
5. `SparkDialect` — `RLIKE`, `regexp_extract`
6. PolyglotEmitter passthrough (if enabled)
7. Tests: SQL output per dialect for each new variant

### Phase G3: Declarative YAML Deserializer

**Files:** `semstrait-model/src/types.rs`, `semstrait-model/src/expr_block.rs` (new)

1. Define `ExprSource` enum (Inline | Declarative) with `#[serde(untagged)]`
2. Define `ExprBlock` struct with all declarative keys (built-in + standard functions)
3. Implement `ExprBlock -> Expr` conversion (standard functions -> `Expr::FunctionCall`)
4. Update `Dimension`, `Measure`, `Metric` model types to use `ExprSource`
5. Add `expr: Option<ExprSource>` to `Dimension` struct
6. Tests: YAML->Expr roundtrip for all block variants including standard functions

### Phase G4: Compilation Pipeline Enhancement

**Files:** `semstrait-manifest/src/steps.rs`

1. Implement `FunctionRegistry` with standard function specs (28 functions)
2. Enhance `compile_exprs` (step 8) to handle `ExprSource::Declarative`
3. Add dimension expression compilation (step 8.3)
4. Implement `validate_expr_bindings()` (step 8.6)
5. Populate `CompiledDimension.expr` and `CompiledDimension.expr_source`
6. Tests: compilation of computed dimensions, binding validation errors

### Phase G5: Planner Integration

**Files:** `semstrait-planner/src/kind/shared.rs`, `semstrait-planner/src/expr_lower.rs`

1. Partition dimensions into regular/literal/metadata/computed in `build_binding_plan()`
2. Implement `lower_computed_dimension()` in `expr_lower.rs`
3. Add computed dimension expressions to ProjectNode
4. Update ProjectNode schema to include computed dimensions
5. Handle computed dimensions in unionset/joinset branches
6. Tests: plan generation with computed dimensions, unionset with per-dataset resolution

### Phase G6: End-to-End Validation

**Files:** `tests/e2e_pipeline_test.rs`, `test_data/alpinestars_eu_ad_platform_v2.yaml`

1. Update alpinestars model with computed `market` dimension using declarative block
2. E2E test: YAML -> compile -> plan -> adapt -> SQL/Substrait for computed dimension
3. E2E comparison: computed dimension across unionset branches
4. Regression: all existing tests pass unchanged

---

## 11. Scope Exclusions

| Item | Reason | Future |
|------|--------|--------|
| Window functions | Separate design space, different planner integration | Phase H |
| PARTITION BY | Requires window function support | Phase H |
| SIMILAR TO | DuckDB only — not portable | Never |
| GLOB | DuckDB only — lower to LIKE or REGEXP | If needed |
| SQL expression parser (pest/nom) | Inline DSL stays simple; complex goes declarative | Not planned |
| Cross-kind expression references | Prohibited in v1 (COMP_E006) | v2 multi-kind |
| Trino engine support | Outside v1 scope | Re-add via EngineAdapter + SqlDialect |

---

## 12. Open Problems (from alpinestars model)

| # | Problem | Resolution |
|---|---------|------------|
| 1 | Declarative dimension expressions not supported | **Solved by this design** — Phase G3-G5 |
| 2 | `temporal.timeseries` schema mismatch | Separate ticket — config naming alignment |
| 3 | Undeclared Klaviyo measures | Model fix — add measure declarations |
| 4 | Duplicate `ai_context.synonyms` | Model fix — deduplicate synonyms |
| 5 | REGEXP/pattern matching in Expr | **Solved by this design** — Phase G1 (RegexpMatch, RegexpExtract) |
| 6 | `catalog: polaris` shorthand | **Already works** — CatalogRef string deserialization (Phase 5) |
