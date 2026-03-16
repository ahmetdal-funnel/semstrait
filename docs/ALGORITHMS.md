# Algorithms Reference

Extracted from the v1.0 implementation (`./semstrait/`). These algorithms capture
domain knowledge for aggregate-aware semantic query planning. The v1.0 types are
gone, but the patterns map directly to v1.2 kind-type resolution (grainset,
unionset, joinset).

---

## 1. Aggregate-Aware Dataset Selection

**Source:** `semstrait/src/selector/select.rs`

Selects the optimal dataset(s) from a semantic model to serve a query.

### Core idea

Given a set of required dimensions and measures, find the smallest (most
aggregated) dataset that can satisfy the query. "Smallest" is approximated by
attribute count -- fewer attributes implies a higher level of pre-aggregation.

### Algorithm

```
function select_datasets(model, required_dims, required_measures):
    1. Extract dataset-group qualifiers from 3-part dimension paths
       e.g. "adwords.dates.year" -> qualifier = "adwords"

    2. Scope candidate groups:
       - If qualifiers present: only those specific groups
       - Otherwise: all groups in the model

    3. Build feasibility matrix:
       For each (group, dataset) pair, check:
       - Every required dimension.attribute exists on the dataset
         (skip virtual `_dataset.*` dimensions -- always available)
       - Every required measure is defined in the group AND
         the dataset declares support for it

    4. Group feasible datasets by their parent group

    5. Ambiguity check:
       - 0 feasible groups -> error with diagnostic of what's missing
       - >1 feasible groups -> AmbiguousDatasetGroup error
         (user must qualify with group prefix or use cross-group metric)

    6. Within the single feasible group:
       a. If datasets have partitions:
          Return all partition datasets (planner applies WHERE pruning)
       b. Otherwise:
          Return the dataset with min(attribute_count)  -- "smallest first"
```

### Multi-Dataset JOIN Selection

When no single dataset has all required measures, multiple datasets within the
same group can be joined on their common dimensions.

```
function select_datasets_for_join(model, required_dims, required_measures):
    1. Identify target group (same qualifier logic as above)

    2. Filter datasets that have ALL required dimensions
       (they must all participate in the JOIN)

    3. Sort by attribute_count ascending (smallest first)

    4. Assign measures using "first smallest wins":
       For each measure, assign to the smallest dataset that has it

    5. Return datasets with their assigned measures
```

**v1.2 mapping:** This becomes grainset resolution (Steps 1-6 in schema.yml).
The "smallest first" heuristic maps to selecting the most aggregated grain level.

---

## 2. Query Routing Decision Tree

**Source:** `semstrait/src/planner/plan.rs`

The top-level router that classifies a query and dispatches to the appropriate
planner.

### Decision tree

```
function plan_semantic_query(model, request):
    extract dimension_attrs from request.rows + request.columns
    detect cross_dataset_metrics (metrics spanning multiple groups)
    extract qualified_groups from 3-part dimension paths
    check is_conformed (all dims exist in all groups)

    if 1 cross-dataset metric:
        -> plan_cross_dataset_group_query (ratio/comparison across groups)

    else if >1 cross-dataset metrics:
        -> plan_multi_cross_dataset_group_query

    else if >1 qualified groups:
        -> plan_multi_tablegroup_query (UNION with NULL-fill per group)

    else if exactly 1 qualified group:
        -> plan_single_tablegroup_query (strip qualifier, plan within group)

    else:  // unqualified query
        try select_datasets(model, dims, measures):
            if multiple partitions selected:
                -> plan_partitioned_union
            else if conformed dims + multiple groups:
                -> plan_conformed_query (UNION across groups)
            else:
                -> resolve_query + plan_query (single dataset)

        on selection failure:
            if conformed + multiple groups:
                -> plan_conformed_query (fallback)
            else:
                try select_datasets_for_join:
                    if 1 dataset: -> single dataset plan
                    else: -> plan_same_tablegroup_join (multi-table JOIN)
```

**v1.2 mapping:** This decision tree maps to kind-type dispatch:
- Grainset -> aggregate-aware selection within a kind
- Unionset -> conformed/partitioned UNION paths
- Joinset -> multi-dataset JOIN path

---

## 3. UNION with Typed NULLs

**Source:** `semstrait/src/planner/union.rs`

### Partitioned UNION

Simple case: each partition produces an identical plan, combined with UNION ALL.

```
function plan_partitioned_union(partitions):
    for each partition:
        resolve_query + plan_query -> branch plan
    return UNION ALL(branch_plans)
```

### Conformed Dimension UNION

When multiple groups share the same dimensions (conformed), produce one branch
per group with NULL-fill for columns that don't exist in that group.

```
function plan_conformed_query(model, request, dimension_attrs):
    separate physical_dims from virtual_dims

    if only virtual dims and no metrics:
        -> plan_virtual_only_query (VirtualTable, no scan needed)

    for each dataset_group in model:
        find feasible dataset (has all physical dims + required measures)
        if partitioned: create branch per partition
        else: create single branch
        each branch = Scan -> JOIN dims -> Aggregate -> Project

    Project step handles NULL-fill:
        - Columns belonging to this group: project actual column
        - Columns belonging to OTHER groups: project typed NULL literal
        - Virtual dimensions: project literal constant value

    return UNION ALL(branches)
```

### Multi-Group Qualified UNION

When dimensions are qualified for specific groups (e.g., `adwords.dates.year`
and `facebookads.dates.year`), each group produces a branch. Columns not
belonging to a group are projected as typed NULLs.

**v1.2 mapping:** This is unionset resolution (Steps 1-6 in schema.yml).

---

## 4. Multi-Table JOIN within Same Group

**Source:** `semstrait/src/planner/join.rs`

When measures span multiple datasets in the same group, JOIN them on shared
dimension columns.

### Algorithm

```
function plan_same_tablegroup_join(model, selection, dims, metrics):
    1. Separate physical dims from virtual dims

    2. For each selected dataset (with its assigned measures):
       Build sub-query:
         Scan(dataset)
         -> Aggregate(group_by=dim_columns, aggs=assigned_measures)
         -> Project(dim_columns + metric_columns)
       Alias as t0, t1, t2, ...

    3. Chain sub-queries with FULL OUTER JOIN:
       t0 FULL JOIN t1 ON t0.dim_col = t1.dim_col
          FULL JOIN t2 ON *.dim_col = t2.dim_col ...

       If no physical dims: use CROSS JOIN instead

    4. Final projection:
       - Dimensions: COALESCE(t0.dim, t1.dim, ...) AS "dim.attr"
         (handles NULLs from FULL OUTER JOIN)
       - Virtual dims: literal values
       - Metrics: reference from their assigned sub-query alias

    5. Sort by dimension columns (ascending)
```

**Key pattern:** COALESCE across all sub-query aliases for shared dimension
columns. This ensures the dimension value is present even if one side of the
FULL OUTER JOIN has no matching rows.

**v1.2 mapping:** This is joinset resolution (Steps 1-7 in schema.yml), with
cardinality-aware join type selection.

---

## 5. Dimension Path Resolution

**Source:** `semstrait/src/resolver/resolve.rs`

Resolves string-based dimension references to concrete schema objects.

### Path formats

| Format | Example | Resolution |
|---|---|---|
| `dimension.attribute` | `dates.year` | Look up in current group, then model |
| `group.dimension.attribute` | `adwords.dates.year` | Look up in specific group |
| `_dataset.attribute` | `_dataset.partition` | Virtual metadata (model/group/dataset properties) |

### Resolution steps

```
function resolve_attribute(model, group, dataset, attr_str):
    parts = attr_str.split(".")

    if 2 parts (dim.attr):
        if dim == "_dataset":
            -> resolve_meta_attribute (literal values from model metadata)
        if group_dim is degenerate (inline attributes, no join):
            -> AttributeRef::Degenerate { group_dim, attribute }
        else:
            -> AttributeRef::Joined { group_dim, dimension, attribute }

    if 3 parts (group.dim.attr):
        find target_group by name
        resolve within that group (same as 2-part)
```

### Dimension types

- **Degenerate:** Attributes are columns on the fact table. No JOIN needed.
  Defined inline on the dataset-group dimension.
- **Joined:** Attributes live in a separate dimension table. Requires JOIN
  using the join spec (leftKey on fact, rightKey on dimension table).
- **Virtual (`_dataset`):** Metadata attributes resolved to literal constants
  at plan time. Values come from: model name, namespace, group name, dataset
  name, uuid, partition value, or arbitrary `properties` map.

### Metric and Measure Resolution

```
function resolve_metrics(model, metric_names):
    for each name: lookup model.get_metric(name)

function collect_metric_measures(group, dataset, metrics):
    extract all measure names referenced by metric expressions
    (traverse MeasureRef, Structured/Add/Subtract/Multiply/Divide/Case nodes)
    verify each measure exists in group AND dataset supports it
```

**v1.2 mapping:** Path resolution stays relevant but types change. Dimensions
become typed discriminated unions (temporal, categorical, binary, geo, bucketed).
`ref:` syntax replaces inline definitions for shared dimensions/measures.
