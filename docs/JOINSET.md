# Joinset Resolution Strategy

**Status:** Implemented
**Scope:** Planner, Manifest (validation + compilation), IR
**Taxonomy:** Complex kind (multi-dataset, JOIN chain from an anchor). For the Simple kind (single-dataset fast path), see `DATASET.md`. Peers: `GRAINSET.md`, `UNIONSET.md`.

---

## 1. Definition

A **joinset** is a collection of datasets that are **semantically related** through explicit **relationships** (foreign key associations). Unlike grainset (UNION ALL of equivalent datasets) or unionset (vertical stacking), a joinset combines datasets horizontally via **JOIN operations**.

- **Semantics**: dimensions and measures are distributed across multiple datasets. Each dataset provides a subset of the kind's interface. The planner joins datasets together to satisfy queries that span multiple datasets.
- **Relationships**: explicit edges between datasets, defined with join type, column pairs, and cardinality. Relationships form a graph that the planner traverses via BFS.
- **Anchor**: the dataset chosen as the starting point of the join chain — the one covering the most requested fields.

---

## 2. Model Structure

### 2.1 Datasets with Distributed Semantics

```yaml
joinsets:
  - name: order_details
    associativity: left
    dimensions:
      - name: order_date
      - name: customer_name
      - name: region_name
    measures:
      - name: revenue
        agg: sum
    datasets:
      - name: orders
        extras:
          column_mapping:
            order_date: created_at
            revenue: amount
            customer_id: cust_id        # join key (not a dimension)
      - name: customers
        extras:
          column_mapping:
            customer_name: name
            id: id                       # join key
            region_id: region_id         # join key to regions
      - name: regions
        extras:
          column_mapping:
            region_name: name
            id: id                       # join key
    relationships:
      - name: orders_customers
        from: orders
        to: customers
        join_type: left
        columns:
          - { from: customer_id, to: id }
        cardinality: many_to_one
      - name: customers_regions
        from: customers
        to: regions
        join_type: left
        columns:
          - { from: region_id, to: id }
        cardinality: many_to_one
```

Each dataset maps only its own columns. Join keys (`customer_id`, `id`, `region_id`) must be in the column_mapping but are not dimensions — they are physical columns used for join conditions.

### 2.2 Relationship properties

- `from` / `to`: dataset names forming a directed edge
- `join_type`: `inner`, `left`, `right`, or `full` — maps directly to IR JoinType
- `columns`: pairs of column names used in the ON condition (supports composite keys)
- `cardinality`: `many_to_one`, `one_to_many`, `many_to_many` — informational for optimization

### 2.3 Associativity

`associativity: left | right | full` is declared at the joinset level (see `2.1` example). Currently used as a declaration; the BFS traversal from the anchor determines actual join order.

---

## 3. Resolution Algorithm

### 3.1 Overview

```
Query Request
     |
     v
[1] VALIDATE PRECONDITIONS
     |  Check datasets exist
     |  Check relationships exist (for multi-dataset)
     v
[2] SELECT ANCHOR DATASET
     |  Dataset covering the most requested fields
     v
[3] BFS JOIN ORDER
     |  Traverse relationship graph from anchor
     |  Build ordered list of join steps
     v
[4] BUILD JOIN TREE
     |  Per-dataset: Scan (with join key columns)
     |  Left-fold joins into tree
     v
[5] AGGREGATE + PROJECT
     |  GROUP BY dimensions, aggregate measures
     |  Map to semantic names
     v
  PlanFragment
```

### 3.2 Step 1: Validate Preconditions

Two precondition checks before planning:

1. **No datasets**: error — "joinset kind has no datasets"
2. **Multiple datasets, no relationships**: error — "joinset kind has multiple datasets but no relationships"

**Single-dataset degenerate case**: if the kind has exactly one dataset, the joinset planner delegates to `shared::build_dataset_plan()` — the same Scan → Aggregate → Project path used by grainset. No joins are built.

### 3.3 Step 2: Select Anchor Dataset

The anchor is the dataset covering the **most requested fields** (dimensions + measures). Metadata dimensions are excluded from the coverage count — they don't need column_mapping entries.

```
Request: order_date, customer_name, revenue

Coverage scores:
  orders:    order_date (1) + revenue (1)     = 2  <- ANCHOR
  customers: customer_name (1)                = 1
  regions:   (0)                              = 0
```

The anchor becomes the root of the join tree. In case of ties, the first dataset with the highest score wins.

### 3.4 Step 3: BFS Join Order

Starting from the anchor, the planner performs breadth-first search through the relationship graph.

```
Anchor: orders
Relationships: orders -> customers, customers -> regions

BFS:
  Visit: orders (anchor, already visited)
  Queue: [orders]

  Pop: orders
    Relationship orders->customers: customers not visited -> add step
    Queue: [customers]

  Pop: customers
    Relationship customers->regions: regions not visited -> add step
    Queue: [regions]

  Pop: regions
    No unvisited neighbors

Join order: [customers (via orders->customers), regions (via customers->regions)]
```

**Bidirectional traversal**: relationships are directed (`from` → `to`), but BFS traverses both directions. If a relationship's `to` side is the current node and `from` is unvisited, the step is marked as **reversed**. This affects which side of the join condition maps to which dataset.

```
Relationship: A -> B (from=A, to=B)

Normal traversal (current=A, target=B):
  left_ds = A (from), right_ds = B (to)
  reversed = false

Reverse traversal (current=B, target=A):
  left_ds = A (from), right_ds = B (to)  -- but A is the new dataset being joined
  reversed = true
```

**Disconnected datasets**: datasets not reachable from the anchor via any relationship path are silently excluded from the join tree. They contribute no data to the query.

### 3.5 Step 4: Build Join Tree

The join tree is built by left-folding join steps onto the anchor's scan.

**Per-dataset Scan**: each dataset's scan includes:
1. Physical columns for requested dimensions the dataset maps
2. Physical columns for requested measures the dataset maps (via expr lowering)
3. **Join key columns** from all relationships involving this dataset

```
orders scan:    [created_at, amount, cust_id]     # dim + measure + join key
customers scan: [name, id, region_id]              # dim + join keys
regions scan:   [name, id]                         # dim + join key
```

**Join condition**: built from relationship column pairs, resolved through each dataset's column_mapping to physical names. Multiple column pairs are AND'd together.

```
Relationship: orders -> customers, columns: [{from: customer_id, to: id}]

orders.column_mapping[customer_id] = "cust_id"
customers.column_mapping[id] = "id"

Condition: cust_id = id
```

**Left-fold construction**:

```
Step 0: current_plan = Scan(orders)

Step 1: Join orders -> customers
  current_plan = Join(
    left:  current_plan,           # Scan(orders)
    right: Scan(customers),
    type:  Left,
    on:    cust_id = id
  )

Step 2: Join customers -> regions
  current_plan = Join(
    left:  current_plan,           # Join(orders, customers)
    right: Scan(regions),
    type:  Left,
    on:    region_id = id
  )
```

### 3.6 Step 5: Aggregate and Project

After the join tree is complete, an Aggregate node groups by dimension columns and aggregates measures, followed by a Project node mapping to semantic names.

**Dimension resolution**: for each requested dimension, the planner searches joined datasets (in join order) for the first column_mapping entry. Literal mappings are skipped (injected in projection).

**Measure resolution**: for each requested measure, the planner finds which joined dataset provides it and lowers the measure expression using that dataset's column_mapping.

```
Final plan:

Join(Join(Scan(orders), Scan(customers)), Scan(regions))
  -> Aggregate(
       GROUP BY [created_at, name_customers, name_regions],
       aggregates: [SUM(amount)]
     )
  -> Project(
       order_date = created_at,
       customer_name = name_customers,
       region_name = name_regions,
       revenue = SUM(amount)
     )
```

---

## 4. Detailed Scenarios

### 4.1 Two-Dataset Join (Orders + Customers)

```yaml
joinsets:
  - name: order_details
    datasets:
      - name: orders      # maps: order_date, revenue, customer_id (join key)
      - name: customers   # maps: customer_name, id (join key)
    relationships:
      - from: orders
        to: customers
        join_type: left
        columns: [{ from: customer_id, to: id }]
```

**Query:** `SELECT order_date, customer_name, revenue`

**Resolution:**
1. Anchor: orders (covers order_date + revenue = 2 fields)
2. BFS: orders -> customers (1 step)
3. Join tree: `Join(Scan(orders), Scan(customers), Left, cust_id = id)`
4. Aggregate: GROUP BY (created_at, name), SUM(amount)
5. Project: (order_date, customer_name, revenue)

**IR:**
```
Scan(orders, [created_at, amount, cust_id])
  JOIN LEFT Scan(customers, [name, id])
    ON cust_id = id
  -> Aggregate(GROUP BY [created_at, name], [SUM(amount)])
  -> Project([order_date, customer_name, revenue])
```

### 4.2 Multi-Hop Join (Orders + Customers + Regions)

**Query:** `SELECT order_date, region_name, revenue`

**Resolution:**
1. Anchor: orders (covers order_date + revenue = 2 fields)
2. BFS: orders -> customers -> regions (2 steps)
3. Join tree: `Join(Join(Scan(orders), Scan(customers)), Scan(regions))`
4. Even though `customer_name` is not requested, the customers dataset is joined because it's on the path to regions

**IR:**
```
Join(
  left: Join(
    left: Scan(orders, [created_at, amount, cust_id]),
    right: Scan(customers, [id, region_id]),
    type: Left, on: cust_id = id
  ),
  right: Scan(regions, [name, id]),
  type: Left, on: region_id = id
)
-> Aggregate(GROUP BY [created_at, name_regions], [SUM(amount)])
-> Project([order_date, region_name, revenue])
```

### 4.3 Single-Dataset Degenerate Case

**Query:** `SELECT order_date, revenue` (all fields from orders)

**Resolution:**
1. Kind has multiple datasets, but single-dataset check: only 1 dataset
   - OR: kind has only 1 dataset total
2. Delegate to `shared::build_dataset_plan()`
3. Result: Scan -> Aggregate -> Project (no Join node)

### 4.4 Composite Join Keys

```yaml
relationships:
  - from: order_lines, to: products
    columns:
      - from: product_id, to: id
      - from: variant_code, to: variant
```

**Join condition:** `product_id = id AND variant_code = variant`

Multiple column pairs are AND'd together to form the full join condition.

---

## 5. Error Cases

### 5.1 Compile-Time Errors

| Error | When |
|-------|------|
| No datasets | Joinset kind defined with empty datasets list |
| No relationships (multi-dataset) | Multiple datasets but no relationships defined |

### 5.2 Plan-Time Errors

| Error | When |
|-------|------|
| Measure not found | Requested measure not mapped by any joined dataset |
| No covering dataset | No datasets available for anchor selection |
| Disconnected dataset | Dataset not reachable via BFS — silently excluded (not an error, but may cause measure-not-found if needed fields are on the disconnected dataset) |

---

## 6. IR Nodes Produced

### 6.1 Single-Dataset Plan (Degenerate)

```
ScanNode (physical columns)
  -> AggNode (GROUP BY dims, aggregate measures)
    -> ProjectNode (semantic names)
```

### 6.2 Two-Dataset Join Plan

```
JoinNode
  +-- ScanNode (anchor dataset, includes join keys)
  +-- ScanNode (joined dataset, includes join keys)
  condition: col_a = col_b
-> AggNode (GROUP BY dims, aggregate measures)
  -> ProjectNode (semantic names)
```

### 6.3 Multi-Hop Join Plan

```
JoinNode (outer)
  +-- JoinNode (inner)
  |     +-- ScanNode (anchor)
  |     +-- ScanNode (hop 1)
  |     condition: key_1a = key_1b
  +-- ScanNode (hop 2)
  condition: key_2a = key_2b
-> AggNode (GROUP BY dims, aggregate measures)
  -> ProjectNode (semantic names)
```

Join tree is left-deep: each new hop is joined as the right child of a new JoinNode whose left child is the accumulated tree.

---

## 7. Related Documentation

- `docs/DATASET.md` — single-dataset (Simple kind) planning.
- `docs/GRAINSET.md`, `docs/UNIONSET.md` — peer Complex kinds.
- `crates/semstrait-planner/src/data_kind/joinset.rs` — implementation.
- `test_data/comprehensive_ecommerce.yaml` — working joinset fixture.
