# Computed Expressions

**Status:** Implemented
**Scope:** Derived dimensions, measures, and metrics defined via expression trees. Hybrid inline DSL + declarative YAML block.
**Engine scope:** DataFusion (primary), plus DuckDB and Spark SQL dialect support in `semstrait-adapter`.

See also: `SEMANTIC_RESOLUTION.md` (rules SR-5, SR-10), `FUNCTION_CATALOG.md` (engine function mapping), `crates/semstrait-core/README.md` (Expr variants), `crates/semstrait-manifest/README.md` (step 8 compilation).

---

## 1. Overview

Computed expressions reference **semantic names** (dimensions, measures). Physical binding is resolved at compile time via `column_mapping`. The planner emits aliasing projections before expression evaluation. All expression trees are fully resolved and validated during manifest compilation — no per-query re-parsing.

Two authoring surfaces, chosen by YAML shape:

| Surface | Use for | YAML shape |
|---------|---------|------------|
| **Inline DSL** | Simple arithmetic, entity refs, safe-divide | `expr: "cost / clicks"` |
| **Declarative block** | CASE/WHEN, IN-list, LIKE/ILIKE/REGEXP, function composition | `expr:` followed by a YAML map that maps 1:1 to `Expr` variants |

`ExprSource` is an untagged serde enum — a scalar routes to the inline parser, a map routes to the declarative deserializer.

```rust
#[derive(Deserialize)]
#[serde(untagged)]
pub enum ExprSource {
    Inline(String),
    Declarative(ExprBlock),
}
```

---

## 2. Inline DSL

Supported: `+`, `-`, `*`, `/`, `//` (safe divide), entity refs (`{{ name }}`), bare identifiers, numeric literals.

```yaml
measures:
  - name: cpc
    agg: avg
    expr: "cost / clicks"

metrics:
  - name: profit_margin
    expr: "{{ revenue }} - {{ cost }}"
```

The inline DSL is deliberately minimal. Anything requiring `CASE`, `IN`, `LIKE`, `REGEXP`, or function calls MUST use a declarative block.

---

## 3. Declarative block

Each YAML key corresponds to an `Expr` variant name in snake_case. Leaves (bare strings) are resolved as semantic name references.

```yaml
dimensions:
  - name: market
    type: categorical
    expr:
      case:
        when:
          - condition:
              in_list: { expr: dataset_name, list: ["adwords", "facebook", "bing", "tiktok"] }
            then:
              case:
                when:
                  - condition:
                      like: { expr: campaign, pattern: "UK_%" }
                    then: "GB"
                else:
                  upper:
                    regexp_extract: { expr: campaign, pattern: "^([A-Z]{2})_", group: 1 }
        else: ""
```

Applies to dimensions, measures, and metrics — at both shared (top-level) and kind-internal scope.

### 3.1 Known limitation (DL-049)

Nested `ExprSource` inside certain untagged enum hierarchies (grainsets/unionsets/joinsets kind-level computed dims) fails to parse under `serde_yaml 0.9`. Workaround: use inline string expressions for kind-level computed dims. Declarative blocks work in top-level `datasets:`.

---

## 4. Semantic-name resolution

Resolution happens at compile time in step 8 (`compile_exprs`):

1. **Semantic name lookup** — search `column_mapping.physical` for the name
2. **Fallback** — search for the name as a physical column (identity mapping case)
3. **Error** — otherwise `CompileError::UnresolvedExprRef`

The compiled `Expr` tree stores `EntityRef(name)` nodes; the planner resolves them to `Column(physical_name)` via `expr_lower`.

---

## 5. Static pushdown (SR-10)

For computed dimensions, metadata- and literal-backed references are **substituted at compile time** using the binding's known values. After substitution the expression is `simplify()`-folded. This is why a computed dim like `CASE WHEN dataset_name = 'adwords' THEN ...` can collapse to a single `Literal` for a dataset whose `dataset_name` is a known literal value.

See `SEMANTIC_RESOLUTION.md` §SR-10 for the full rule.

---

## 6. Expression variants added for this feature

| Variant | Purpose |
|---------|---------|
| `Expr::ILike` | Case-insensitive LIKE; emitted per-dialect (native where supported, `LOWER(x) LIKE LOWER(p)` elsewhere) |
| `Expr::RegexpMatch` | Boolean regex match |
| `Expr::RegexpExtract` | Capture-group extraction |

Full engine-rewrite behavior for these variants lives in `FUNCTION_CATALOG.md` §8 (Pattern Matching).

---

## 7. Function registry (DL-050)

28 ANSI SQL functions are arity-validated at compile time via `FunctionRegistry` in `semstrait-manifest/src/function_registry.rs`. Categories: String (13), Math (7), Date (5), Conditional (3). Unknown functions pass with a warning (extensibility). Computed-dim expressions may not contain aggregation (enforced during step 8).

---

## 8. Where expressions are injected

- **Computed dimension** → ProjectNode appended after `AggNode` (DL-048). Dependent physical columns are collected for the `ScanNode`.
- **Measure expression (horizontal part)** → inside `AggNode` aggregate input.
- **Metric expression** → `post_agg_expr` in the final `ProjectNode`.

See `DATASET.md` §2 (layered plan) for the full emission order.

---

## 9. Related Documentation

- `docs/SEMANTIC_RESOLUTION.md` — SR-5 (expr rules), SR-10 (static pushdown), SR-11 (compile-time resolution)
- `docs/FUNCTION_CATALOG.md` — canonical functions and per-engine rewriting
- `crates/semstrait-core/README.md` — `Expr` variant reference
- `crates/semstrait-manifest/README.md` — compilation pipeline, `FunctionRegistry`
- `crates/semstrait-planner/README.md` — computed-dim partitioning and injection
