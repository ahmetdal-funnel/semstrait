# Semantic Resolution Specification

> **Version:** 1.0  
> **Date:** 2026-04-13  
> **Status:** Design specification — implementation follows in phases  
> **Supersedes:** DL-049 (nested untagged enum limitation — to be fixed)

---

## 1. Overview

This document defines how semantic entities (dimensions, measures, metrics) are
defined, referenced, inherited, overridden, and resolved across the three-tier
scope hierarchy in a semstrait semantic model. It covers naming rules, expression
availability, physical binding, and compile-time optimizations.

The core principle is: **define once, reference by name, override closer to the
data, resolve everything at compile time.**

---

## 2. Scope Hierarchy

Semantic models have exactly three tiers. Each tier has specific responsibilities
and constraints.

```
┌──────────────────────────────────────────────────────────────┐
│ Tier 1: Top-Level Definitions                                │
│   dimensions:, measures:, metrics:                           │
│   Global semantic definitions. Shared across all kinds.      │
│   Can carry: name, data_type, description, type, expr, agg  │
├──────────────────────────────────────────────────────────────┤
│ Tier 2: Data Kind Interface                                  │
│   unionsets:, grainsets:, joinsets:                           │
│   Composes refs + local overrides + new local definitions.   │
│   Can carry: dimensions, measures, metrics, keys, filters,   │
│              expr, relationships (joinset only)               │
├──────────────────────────────────────────────────────────────┤
│ Tier 3: Dataset (Physical Leaf)                              │
│   datasets: (nested inside a kind or standalone)             │
│   Physical binding ONLY: column_mapping, storage, catalog.   │
│   CANNOT define: dimensions, measures, metrics, expr, keys.  │
└──────────────────────────────────────────────────────────────┘
```

### 2.1 Tier Responsibilities

| Tier | Can Define Semantics | Can Define Expr | Can Define Keys | Physical Binding |
|------|---------------------|----------------|----------------|-----------------|
| **Tier 1** (top-level) | Yes | Yes | No (keys are per-container) | No |
| **Tier 2** (data kind) | Yes (ref + override + new) | Yes | Yes (local to kind) | No (delegated to datasets) |
| **Tier 3** (dataset) | **No** | **No** | **No** | Yes (column_mapping, storage, catalog) |

### 2.2 Standalone Datasets

A standalone dataset (declared directly under `datasets:` at the model root, not
nested in a kind) is a special case: it implicitly creates a single-dataset kind.
Standalone datasets CAN define dimensions, measures, metrics, and expressions
because they simultaneously serve as both Tier 2 (interface) and Tier 3 (binding).

```yaml
# Standalone dataset = Tier 2 + Tier 3 combined
datasets:
  - name: shopify
    dimensions:          # Tier 2: semantic interface
      - ref: date
      - name: market
        data_type: string
        expr: { case: ... }
    measures:            # Tier 2: semantic interface  
      - name: revenue
        data_type: decimal
        agg: sum
    extras:              # Tier 3: physical binding
      column_mapping:
        date: created_at
        revenue: order_total
      storage:
        tables: ["shopify.*"]
```

When a dataset is nested inside a kind (unionset/grainset/joinset), it is
strictly Tier 3 and CANNOT define semantics. The kind provides the interface;
the dataset provides the physical mapping.

---

## 3. Naming and Uniqueness Rules

### 3.1 SR-1: Unique Names Within Scope

Each dimension, measure, and metric name must be unique within the scope where
it is defined:

- **Tier 1**: No two top-level dimensions may share a name. Same for measures
  and metrics. Cross-type name collision (dim `amount` + measure `amount`) is
  allowed — they are in separate namespaces.
  
- **Tier 2**: No two dimensions may share a name within a single kind's
  interface. A kind may have a local dimension with the same name as a top-level
  dimension (this is an override — see SR-3).

**Violation = compilation error.**

```
Error: duplicate dimension 'country' in unionset 'paid_media'
```

### 3.2 Dimensions, Measures, Metrics Are Separate Namespaces

A dimension named `revenue` and a measure named `revenue` do not conflict. The
compiler and planner resolve references by context: `expr` fields on measures
reference other measures/metrics; `expr` fields on dimensions reference other
dimensions; `column_mapping` keys match by name against the appropriate namespace.

---

## 4. Reference and Override Rules

### 4.1 SR-2: Ref = Inherit Global Definition

`ref: X` at Tier 2 pulls the Tier 1 definition of `X` into the kind's
interface as-is. No modification. The kind gets the exact same name, data_type,
description, dim_type, and expr (if any) as the global definition.

```yaml
# Tier 1
dimensions:
  - name: date
    data_type: date
    type:
      temporal:
        grains: [day, week, month]

# Tier 2: inherits everything from Tier 1's 'date'
unionsets:
  - name: paid_media
    dimensions:
      - ref: date
```

If `X` does not exist at Tier 1, compilation fails:

```
Error: ref 'date' not found in top-level dimensions
```

### 4.2 SR-3: Name = Override or New Local

At Tier 2, `name: X` where `X` matches a Tier 1 name creates a **local
override**. The more concrete (closer to data) definition wins. The override
must provide all required fields — it fully replaces the global definition within
this kind's scope.

```yaml
# Tier 1: global 'country' — bare dimension, no expression
dimensions:
  - name: country
    data_type: string

# Tier 2: overrides 'country' with a computed expression
unionsets:
  - name: paid_media
    dimensions:
      - ref: date                    # inherit global
      - name: country                # override — adds expr
        data_type: string
        expr:
          upper: country             # transforms the physical column
```

At Tier 2, `name: Y` where `Y` does NOT exist at Tier 1 creates a **new local
semantic** — it only exists within this kind's interface.

```yaml
unionsets:
  - name: paid_media
    dimensions:
      - name: market                 # new, local only
        data_type: string
        expr:
          case:
            when:
              - condition: ...
                then: ...
```

### 4.3 Override Hierarchy

```
Tier 2 (kind-level)  — most specific for semantics
    ↑ overrides
Tier 1 (top-level)   — global default

Tier 3 (dataset)     — physical binding only, never overrides semantics
```

A dataset CANNOT override a dimension's expression or data_type. Datasets provide
physical mappings to the semantic names defined at Tier 1 or Tier 2.

### 4.4 What Happens When a Kind Uses Both Ref and Name for the Same Semantic

This is a compilation error. A kind's interface cannot contain both `ref: X` and
`name: X`. The intent is ambiguous — pick one.

```yaml
# ERROR: 'country' appears as both ref and inline
unionsets:
  - name: paid_media
    dimensions:
      - ref: country        # inherits global
      - name: country       # tries to override
        data_type: string
        expr: ...
```

```
Error: 'country' defined both as ref and inline in unionset 'paid_media'
```

---

## 5. Expression Rules

### 5.1 SR-5: Expr Available at Tier 1 and Tier 2

Expressions (`expr:`) can be defined on dimensions, measures, and metrics at
both Tier 1 (top-level) and Tier 2 (data kind interface). Expressions are NOT
allowed at Tier 3 (dataset level inside a kind).

```yaml
# Tier 1: global computed dimension
dimensions:
  - name: market_upper
    data_type: string
    expr: { upper: market }

# Tier 2: kind-level computed dimension
unionsets:
  - name: paid_media
    dimensions:
      - name: campaign_country
        data_type: string
        expr:
          regexp_extract:
            col: campaign
            pattern: {lit: "^([A-Z]{2})_"}
            group: 1
```

### 5.2 SR-5a: Expressions Operate on Exposed Semantics Only

An expression can only reference names that are exposed in the same interface
scope. If a dimension `campaign` is not listed in the kind's interface (either
via `ref:` or `name:`), an expression in that kind cannot reference `campaign`.

**The compiler validates this at step 8 (compile_exprs)**: every column
reference in an expression tree is checked against the kind's interface names.
Unknown references produce a compilation error.

```
Error: expression for 'campaign_country' in unionset 'paid_media'
       references 'campaign' which is not in the interface
```

This rule applies uniformly to all expression variants — arithmetic, conditional,
string functions, date functions, etc. If a name isn't in scope, you can't use it.

### 5.3 SR-5b: Measure and Metric Expression Scope

- **Measure `expr:`** — references column names (dimensions or other physical
  columns). Applied as horizontal transformation before aggregation.
- **Metric `expr:`** — references measure names and other metric names.
  Aggregation functions in metric exprs are not allowed (use `agg:` field).
- **Dimension `expr:`** — references other dimension names or column names
  exposed through the interface. Aggregation functions are not allowed (computed
  dimensions are post-aggregation projections).

### 5.4 Expression on Ref'd Semantics

When a Tier 1 dimension has an `expr:`, and a kind uses `ref: X` to inherit it,
the expression carries over. The expression is evaluated in the kind's context —
all names referenced by the expression must be available in the kind's interface.

```yaml
# Tier 1
dimensions:
  - name: market_upper
    data_type: string
    expr: { upper: market }    # references 'market'

# Tier 2: inherits market_upper, but 'market' must also be in scope
unionsets:
  - name: paid_media
    dimensions:
      - ref: market            # required — market_upper's expr references it
      - ref: market_upper      # inherits expr from Tier 1
```

If `market` is not in the kind's interface, compilation fails:

```
Error: inherited expression for 'market_upper' references 'market'
       which is not in unionset 'paid_media' interface
```

---

## 6. Physical Binding Rules

### 6.1 SR-4: Dataset = Physical Binding Only

Datasets nested inside a kind provide:
- `column_mapping` — maps semantic names to physical column names
- `storage` — paths or tables
- `catalog` — catalog reference
- `temporal` — temporal configuration

Datasets CANNOT define:
- `dimensions:`, `measures:`, `metrics:` — compilation error
- `keys:` — compilation error
- `expr:` on any semantic — compilation error
- `filters:` — compilation error

```yaml
# VALID: dataset provides only physical binding
datasets:
  - name: adwords_data
    extras:
      column_mapping:
        date: date
        country: adwords_country
        cost: adwords_cost
      storage:
        tables: ["adwords.*"]

# INVALID: dataset tries to define semantics
datasets:
  - name: adwords_data
    dimensions:              # ERROR: not allowed in nested dataset
      - name: platform
        data_type: string
```

```
Error: dataset 'adwords_data' nested in unionset 'paid_media'
       cannot define dimensions — use the kind interface instead
```

### 6.2 Dataset Nesting Prohibition

A dataset cannot contain other datasets. `Dataset > Dataset` is a structural
error caught at step 4 (validate_structure).

### 6.3 SR-8: No Empty Semantics

After compilation, every semantic name must resolve to either:
1. A physical column (via `column_mapping` in at least one dataset), OR
2. An expression (`expr:` on the dimension/measure/metric), OR
3. Metadata extraction (`type: metadata`)

If a semantic name has none of these, compilation fails:

```
Error: unionset 'paid_media': interface name 'platform' is not mapped
       by any dataset and has no expression
```

### 6.4 SR-9: Partial Coverage in Multi-Dataset Kinds

In a unionset or grainset with multiple datasets, a semantic name does not need
to be mapped in ALL datasets — but it must be mapped in at least one.

- **Unionset**: unmapped dimensions are filled with `NULL` in the UNION ALL
  branches for datasets that lack the mapping.
- **Grainset**: unmapped dimensions within a grain partition may be skipped
  (grain-specific handling).
- **Joinset**: all joined datasets must provide mappings for the join keys.
  Non-key dimensions follow partial coverage rules.

```yaml
unionsets:
  - name: paid_media
    dimensions:
      - ref: country           # mapped by adwords + facebook, not by klaviyo
    datasets:
      - name: adwords
        extras:
          column_mapping:
            country: adwords_country     # mapped
      - name: facebook
        extras:
          column_mapping:
            country: fb_country          # mapped
      - name: klaviyo
        extras:
          column_mapping:
            # country NOT mapped → NULL in klaviyo branch of UNION ALL
```

---

## 7. Key Scoping Rules

### 7.1 SR-6: Keys Are Local to Container

Keys (primary, unique, foreign) are defined on a data kind's interface. They:
- Do NOT propagate from kind to nested datasets
- Do NOT propagate from parent kinds to child kinds
- Do NOT inherit from Tier 1 (there are no top-level keys)
- Are NOT cross-referenced between kinds

Each kind independently declares its own key structure.

```yaml
unionsets:
  - name: paid_media
    keys:
      primary: [campaign_id, account_id]     # local to this unionset
    dimensions:
      - ref: campaign_id
      - ref: account_id
```

---

## 8. Data Kind Nesting Matrix

### 8.1 Allowed Nesting

Complex data kinds (unionset, grainset, joinset) can contain datasets and
other complex kinds. Nested complex kinds are **flattened to top-level** during
parsing — nesting is a YAML authoring convenience, not a runtime hierarchy.

| Parent ↓ / Child → | Dataset | Grainset | Unionset | Joinset |
|---------------------|---------|----------|----------|---------|
| **Top-level model** | ✅ | ✅ | ✅ | ✅ |
| **Grainset** | ✅ (physical) | ❌ | ✅ (flattened) | ✅ (flattened) |
| **Unionset** | ✅ (physical) | ✅ (flattened) | ✅ (flattened, warning) | ✅ (flattened) |
| **Joinset** | ✅ (physical) | ✅ (flattened) | ✅ (flattened) | ❌ |
| **Dataset** | ❌ | ❌ | ❌ | ❌ |

**Legend:**
- ✅ (physical): dataset provides column_mapping only, no semantics
- ✅ (flattened): nested kind promoted to top-level during parse
- ❌: not allowed, compilation error
- ⚠️ Unionset > Unionset: allowed with compiler warning (COMP_W010)

### 8.2 Flattening Semantics

When a unionset contains a nested grainset, the grainset is extracted and
registered as a top-level kind. The parent unionset does not inherit the
grainset's interface — they are independent entities that happen to be authored
in the same YAML block.

---

## 9. Compile-Time Static Pushdown

### 9.1 SR-10: Conditional Expression Pushdown

Conditional expressions (`case`, `if`) can be partially evaluated at compile
time when their branch conditions depend on values known before data is read.

**Compile-time-known values:**
1. **Metadata dimensions** — extracted from source paths/partitions (e.g.,
   `dataset_name` from path token 5). Each dataset has a deterministic value.
2. **Literal column mappings** — `{ lit: "Paid Search" }` in column_mapping.
   The value is a compile-time constant for that dataset.
3. **Dataset-level constants** — any value that can be statically determined
   per dataset without reading actual data.

### 9.2 Pushdown Mechanism

Given a CASE expression on a unionset computed dimension:

```yaml
expr:
  case:
    when:
      - condition:
          in:
            col: dataset_name
            list: [{lit: "adwords"}, {lit: "facebook"}]
        then:
          regexp_extract:
            col: campaign
            pattern: {lit: "^([A-Z]{2})_"}
            group: 1
      - condition:
          eq: [dataset_name, {lit: "impact"}]
        then:
          regexp_extract:
            col: campaign
            pattern: {lit: "^Alpinestars ([A-Z]{2})"}
            group: 1
    else: {lit: ""}
```

When `dataset_name` is a metadata dimension with known value per dataset:
- **adwords dataset** (`dataset_name = "adwords"`): first branch matches →
  push only `regexp_extract(campaign, "^([A-Z]{2})_", 1)` to this dataset's
  scan plan.
- **impact dataset** (`dataset_name = "impact"`): second branch matches →
  push only `regexp_extract(campaign, "^Alpinestars ([A-Z]{2})", 1)`.
- **klaviyo dataset** (`dataset_name = "klaviyo"`): no branch matches →
  push literal `""` (the else clause).

### 9.3 Pushdown Benefits

1. **Eliminates dead branches** — each dataset only computes the relevant
   expression, not the full CASE tree.
2. **Enables per-dataset optimization** — the pushed-down expression is
   simpler and may enable further optimizations (e.g., predicate pushdown
   in the engine).
3. **Reduces SQL complexity** — each UNION ALL branch gets a focused
   expression instead of the full conditional.

### 9.4 Pushdown Scope

Pushdown applies to:
- Computed dimension expressions at Tier 1 or Tier 2
- Guard expressions on measures/metrics
- Filter expressions that reference compile-time-known values

Pushdown does NOT apply to:
- Expressions referencing physical columns with runtime-only values
- Aggregation expressions (these operate on grouped data, not per-row)

### 9.5 Pushdown Is an Optimization, Not a Requirement

The compiler MAY perform pushdown. When it cannot statically determine which
branches apply (e.g., condition references a non-constant dimension), the full
expression is preserved and evaluated at query time. Correctness is maintained
in both cases.

---

## 10. Compile-Time Resolution (SR-11)

### 10.1 All Links Resolve at Compile Time

The compiler resolves every reference during compilation. Nothing is deferred to
query time. Specifically:

| Reference Type | Resolved At | Error If Missing |
|----------------|------------|-----------------|
| `ref: X` on dimension/measure/metric | Step 2 (resolve_refs) | `ref 'X' not found` |
| Expr column reference | Step 8 (compile_exprs) | `references 'Y' not in interface` |
| column_mapping key | Step 5 (validate_mappings) | `mapping key 'Z' not in interface` |
| column_mapping value (physical column) | Step 5 | Validated against interface names |
| Metric → measure reference | Step 6 (build_metric_graph) | `metric 'M' references unknown measure` |
| Storage glob patterns | Step 3 (resolve_sources) | `glob requires catalog/storage provider` |
| Catalog references | Step 3 (resolve_sources) | `catalog 'C' not in registry` |

### 10.2 No Runtime Name Resolution

The planner receives a `CompiledManifest` where every semantic name is fully
resolved. The planner does not perform name lookups against the YAML model — it
operates on compiled, validated data structures. This is a hard invariant.

---

## 11. Summary of Rules

| Rule | ID | Description |
|------|------|-------------|
| Unique names within scope | SR-1 | No duplicate dim/measure/metric names at the same tier level |
| Ref = inherit | SR-2 | `ref: X` pulls Tier 1 definition into Tier 2 as-is |
| Name = override or new | SR-3 | `name: X` at Tier 2 overrides Tier 1's X or creates new local |
| Dataset = physical only | SR-4 | Nested datasets: column_mapping + storage + catalog only |
| Expr everywhere (Tier 1+2) | SR-5 | Expressions available at top-level and kind-level |
| Expr scope validation | SR-5a | Expressions can only reference names in the same interface |
| Keys are local | SR-6 | Keys scoped to container, no inheritance |
| Dataset nesting prohibited | SR-7 | Dataset cannot contain datasets |
| No empty semantics | SR-8 | Every semantic resolves to physical column, expr, or metadata |
| Partial coverage allowed | SR-9 | Multi-dataset kinds: at least one dataset maps each semantic |
| Static pushdown | SR-10 | Conditional exprs pruned on metadata dims, literals, constants |
| All links compile-time | SR-11 | Every ref, expr ref, mapping resolved during compilation |

---

## 12. YAML Examples

### 12.1 Complete Three-Tier Model

```yaml
version: 3.1
semantic_model:
  name: cross-platform-media
  namespace: tenant_001

  # ── Tier 1: Global Definitions ────────────────────────────
  dimensions:
    - name: date
      data_type: date
      type:
        temporal:
          grains: [day, week, month, quarter, year]

    - name: campaign
      data_type: string

    - name: country
      data_type: string

    - name: dataset_name
      data_type: string
      type:
        metadata:
          path:
            token: 5

  measures:
    - name: cost
      data_type: decimal
      agg: sum

    - name: clicks
      data_type: decimal
      agg: sum

    - name: impressions
      data_type: decimal
      agg: sum

  metrics:
    - name: cpc
      data_type: decimal
      expr: "cost / clicks"

    - name: ctr
      data_type: decimal
      expr: "clicks / impressions"

  # ── Tier 2: Data Kind Interface ───────────────────────────
  unionsets:
    - name: paid_media
      keys:
        primary: [campaign_id, account_id]

      dimensions:
        - ref: date                    # inherit global
        - ref: campaign                # inherit global
        - ref: country                 # inherit global
        - ref: dataset_name            # inherit metadata dim
        - name: market                 # new, local with expr
          data_type: string
          expr:
            case:
              when:
                - condition:
                    in:
                      col: dataset_name
                      list:
                        - lit: "adwords"
                        - lit: "facebook"
                  then:
                    upper:
                      regexp_extract:
                        col: campaign
                        pattern: {lit: "^([A-Z]{2})_"}
                        group: 1
              else: {lit: ""}
        - name: measurement_channel    # new, local (literal-mapped)
          data_type: string
        - name: traffic_source         # new, local (literal-mapped)
          data_type: string

      measures:
        - ref: cost
        - ref: clicks
        - ref: impressions

      metrics:
        - ref: cpc
        - ref: ctr

      extras:
        temporal:
          grain: day
          type:
            events:
              occurred_at: date

      # ── Tier 3: Datasets (physical binding only) ──────────
      datasets:
        - name: adwords_data
          extras:
            catalog: polaris
            storage:
              tables: ["adwords.*"]
            column_mapping:
              date: date
              campaign: adwords-campaign
              country: adwords-country
              cost: adwords-cost
              clicks: adwords-clicks
              impressions: adwords-impressions
              measurement_channel:
                lit: "Paid Search"
              traffic_source:
                lit: "Google"

        - name: facebook_data
          extras:
            catalog: polaris
            storage:
              tables: ["facebookads.*"]
            column_mapping:
              date: date
              campaign: facebookads-campaign_name
              country: facebookads-country
              cost: facebookads-spend
              clicks: facebookads-clicks
              impressions: facebookads-impressions
              measurement_channel:
                lit: "Paid Social"
              traffic_source:
                lit: "Facebook"

  # ── Standalone Dataset (Tier 2 + Tier 3 combined) ─────────
  datasets:
    - name: shopify
      dimensions:
        - ref: date
        - name: country
          data_type: string
        - name: market
          data_type: string
          expr:
            case:
              when:
                - condition: {eq: [country, {lit: "Germany"}]}
                  then: {lit: "DE"}
                - condition: {eq: [country, {lit: "United Kingdom"}]}
                  then: {lit: "GB"}
              else: {lit: ""}
      measures:
        - name: revenue
          data_type: decimal
          agg: sum
      extras:
        column_mapping:
          date: created_at
          country: billing_country
          revenue: order_total
        storage:
          tables: ["shopify.*"]
```

### 12.2 Override Example

```yaml
# Tier 1: bare 'country' — just a column
dimensions:
  - name: country
    data_type: string

# Tier 2: unionset overrides 'country' with an expression
unionsets:
  - name: regional_media
    dimensions:
      - name: country              # override: adds expr
        data_type: string
        expr: { upper: country }   # normalize to uppercase
    datasets:
      - name: raw_data
        extras:
          column_mapping:
            country: raw_country   # physical binding for the semantic
```

### 12.3 Invalid: Dataset Defines Semantics

```yaml
# INVALID — nested dataset cannot define dimensions
unionsets:
  - name: paid_media
    dimensions:
      - ref: date
    datasets:
      - name: adwords
        dimensions:                # ← COMPILATION ERROR
          - name: platform
            data_type: string
        extras:
          column_mapping:
            date: date
```

---

## 13. Implementation Notes

### 13.1 DL-049 Fix Required

The current `ExprSource` enum uses `#[serde(untagged)]` and nests inside
`DimensionEntry` (also `#[serde(untagged)]`), causing serde_yaml 0.9 parse
failures for declarative expr blocks in kind-level dimensions. Fix: add a
custom `Deserialize` impl to `ExprSource` (same pattern as `ExprBlock`).

### 13.2 Validation Steps Affected

| Rule | Compilation Step | Current Status |
|------|-----------------|----------------|
| SR-1 (unique names) | Step 4 (validate_structure) | Partially implemented (within-container only) |
| SR-2/SR-3 (ref vs override) | Step 2 (resolve_refs) | Implemented for ref; override detection needed |
| SR-4 (dataset = physical) | Step 4 (validate_structure) | Not yet validated |
| SR-5a (expr scope) | Step 8 (compile_exprs) | Not yet validated |
| SR-8 (no empty) | Step 5 (validate_mappings) | Implemented |
| SR-9 (partial coverage) | Step 5 (validate_mappings) | Implemented |
| SR-10 (pushdown) | New optimizer step | Not implemented |
| SR-11 (compile-time) | Multiple steps | Implemented |

### 13.3 Pushdown Implementation Path

Static pushdown (SR-10) is a planner optimization that can be implemented
incrementally:

1. **Phase 1**: Identify compile-time-known values per dataset (metadata dims +
   literal mappings) during compilation, store in `DatasetBinding`.
2. **Phase 2**: In planner, when building per-dataset scan plans for unionsets,
   substitute known values into conditional expressions and simplify.
3. **Phase 3**: Dead branch elimination — prune CASE/IF branches that evaluate
   to false for a given dataset.

---

## 14. Implementation Status

| Rule | Description | Status | Where |
|------|-------------|--------|-------|
| SR-1 | Unique names within scope level | Implemented | `common.rs:vec_to_btreemap_unique()` — parse-time duplicate detection |
| SR-2 | `ref: X` = inherit Tier 1 as-is | Implemented | `parse.rs:resolve_refs()` (unchanged) |
| SR-3 | `name: X` at Tier 2 = override/new | Implemented | BTreeMap dedup + `vec_to_btreemap_unique()` |
| SR-4 | Datasets CANNOT define semantics | Implemented | `data_kind.rs:DataKindBinding` with `#[serde(deny_unknown_fields)]` |
| SR-5 | `expr:` at Tier 1 + Tier 2 | Implemented | DL-049 fixed — custom `Deserialize` for `ExprSource` (no `#[serde(untagged)]`) |
| SR-5a | Expr scope validation | Implemented | `steps.rs:validate_expr_scope()` — column refs must be in interface |
| SR-6 | Keys local to container | Structural | `SemanticInterface.keys` is per-kind (no cross-kind sharing) |
| SR-7 | Dataset > Dataset = error | Structural | `DataKindBinding` has no `datasets:` field + `deny_unknown_fields` |
| SR-8 | No empty semantics | Existing | `validate_mappings()` ensures physical coverage |
| SR-9 | Partial coverage OK (NULL-fill) | Existing | Unionset planner fills missing dims with NULL |
| SR-10 | Static pushdown | Implemented | `crates/semstrait-planner/src/simplify.rs` — metadata/literal substitution + constant folding |
| SR-11 | All links resolved at compile time | Existing | `resolve_refs()` + `validate_structure()` + nesting matrix |

---

## 15. Relationship to Other Documents

- `crates/semstrait-manifest/README.md` — compilation pipeline (the step numbers SR-* rules attach to)
- `DECISION_LOG.md` — DL-047 (computed dims excluded from mapping), DL-048 (computed dims as ProjectNode), DL-049 (nested-enum serde limitation)
- `docs/COMPUTED_EXPRESSIONS.md` — computed expression reference
- `docs/DATASET.md` — single-dataset (Simple kind) planning
- `docs/GRAINSET.md`, `docs/UNIONSET.md`, `docs/JOINSET.md` — Complex kind strategies
