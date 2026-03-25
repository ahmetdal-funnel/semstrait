# Unionset Resolution Strategy

**Version:** 1.0 | **Status:** Implemented
**Scope:** Planner, Manifest (validation + compilation), IR

---

## 1. Definition

A **unionset** is a collection of datasets that are **vertically stacked** via UNION ALL (or UNION DISTINCT). Each dataset contributes rows to a combined result set with a unified schema.

- **Semantics**: dimensions and measures are defined at the kind level. Each dataset maps a subset of the interface. Unmapped fields are NULL-filled in the output.
- **Union mode**: `All` (UNION ALL — preserves duplicates) or `Unique` (UNION DISTINCT — deduplicates). Declared at the kind level.
- **NULL-fill**: datasets that don't map a requested dimension or measure contribute NULL for that column. After UNION, re-aggregation collapses rows, and SUM(NULL) = NULL ensures only mapped contributions are counted.

---

## 2. Model Structure

### 2.1 Kind with Union Mode

```yaml
kind: all_orders
type: unionset
  mode: all        # or "unique" for UNION DISTINCT

dimensions:
  - name: date
  - name: region

measures:
  - name: revenue
    agg: sum
```

### 2.2 Datasets with Partial Mapping

```yaml
datasets:
  - name: orders_us
    extras:
      column_mapping:
        date: order_date
        region: region_name
        revenue: amount

  - name: orders_eu
    extras:
      column_mapping:
        date: sale_date
        region: region
        revenue: total

  - name: orders_apac
    extras:
      column_mapping:
        date: transaction_date
        revenue: total_amount
        # region NOT mapped — will be NULL-filled
```

Each dataset maps to its own physical column names. Unlike joinset, there are no relationships — datasets are independent and stacked vertically.

### 2.3 Dimension Source Classification

For each dataset branch, every requested dimension is classified into one of three sources:

| Source | Description | Example |
|--------|-------------|---------|
| **Physical** | Mapped via column_mapping to a physical column | `date: order_date` |
| **MetadataLiteral** | Metadata dimension (path/partition extraction) or literal mapping | `platform: { literal: "us" }` |
| **NullFill** | Not mapped by this dataset | `region` in orders_apac |

This classification determines how each dimension appears in the branch's Scan, Aggregate, and Project nodes.

---

## 3. Resolution Algorithm

### 3.1 Overview

```
Query Request
     |
     v
[1] VALIDATE
     |  Check datasets exist
     v
[2] BUILD BRANCHES
     |  Per-dataset: classify dims, lower measures
     |  Scan -> Aggregate -> Project (NULL-fill to unified schema)
     v
[3] COMBINE
     |  UNION ALL / UNION DISTINCT
     |  (skip Union node if single branch)
     v
[4] RE-AGGREGATE
     |  GROUP BY all dimensions
     |  Infer re-aggregation function per measure
     v
  PlanFragment
```

### 3.2 Step 1: Validate

Single precondition: the kind must have at least one dataset. If empty, return error: "unionset kind has no datasets".

### 3.3 Step 2: Build Branches

Each dataset produces one branch with a unified output schema. The unified schema has all requested dimensions (Utf8) followed by all requested measures (Float64).

**Per-branch construction:**

1. **Classify dimensions**: for each requested dimension, determine its source:
   - Check metadata dimensions first (extracted from path/partition)
   - Check column_mapping: Literal values become MetadataLiteral, physical columns become Physical
   - If not found in either: NullFill

2. **Lower measures**: for each requested measure:
   - If the dataset's column_mapping contains the measure name: lower the expression using the dataset's mapping (declarative or legacy path)
   - If not mapped: mark as NULL (None)

3. **Build Scan**: scan only the physical columns needed (from Physical dimensions + lowered measure expressions). NullFill and MetadataLiteral dimensions don't contribute to scan.

4. **Build Aggregate**: GROUP BY only Physical dimension columns. Aggregate only the measures this dataset covers.

5. **Build Project**: output the unified schema:
   - Physical dimensions: `Expr::column(semantic_name)`
   - MetadataLiteral dimensions: the literal expression (e.g., `Expr::string("us")`)
   - NullFill dimensions: `Expr::null()`
   - Covered measures: `lowered.post_agg_expr`
   - Uncovered measures: `Expr::null()`

```
orders_us branch:
  Scan(order_date, region_name, amount)
  -> Aggregate(GROUP BY [order_date, region_name], [SUM(amount)])
  -> Project(date, region, revenue)        # all mapped

orders_apac branch:
  Scan(transaction_date, total_amount)
  -> Aggregate(GROUP BY [transaction_date], [SUM(total_amount)])
  -> Project(date, NULL AS region, revenue)  # region NULL-filled
```

### 3.4 Step 3: Combine

**Single branch**: if only one dataset, skip the Union node entirely. The branch's Project feeds directly into re-aggregation.

**Multiple branches**: wrap all branches in a UnionNode:
- `distinct: false` for mode=All (UNION ALL)
- `distinct: true` for mode=Unique (UNION DISTINCT)

### 3.5 Step 4: Re-aggregate

A final Aggregate node combines rows from all branches:

- **GROUP BY**: all requested dimensions (by semantic name)
- **Aggregates**: one per requested measure, using `infer_aggregation()` to determine the re-aggregation function

**Re-aggregation function inference** (shared with grainset):

| Original Aggregation | Re-aggregation | Correctness |
|---------------------|----------------|-------------|
| SUM | SUM | Exact |
| COUNT | SUM | Exact (sum of partial counts) |
| MIN | MIN | Exact |
| MAX | MAX | Exact |
| COUNT_DISTINCT | SUM | **Lossy** — overcounts across datasets |
| AVG | SUM | **Lossy** — not decomposable |

The re-aggregation output schema matches the unified schema exactly.

---

## 4. Detailed Scenarios

### 4.1 Full Coverage (All Datasets Map Everything)

```yaml
kind: all_orders
type: unionset
  mode: all

datasets:
  - name: orders_us     # maps: date, region, revenue
  - name: orders_eu     # maps: date, region, revenue
```

**Query:** `SELECT date, region, revenue`

**Resolution:**
1. Both datasets map all fields — no NULL-fill needed
2. Branch orders_us: Scan -> Aggregate -> Project(date, region, revenue)
3. Branch orders_eu: Scan -> Aggregate -> Project(date, region, revenue)
4. UNION ALL -> Re-aggregate GROUP BY (date, region), SUM(revenue)

**Result:** combined rows from both regions, revenue summed where dates and regions overlap.

### 4.2 Partial Coverage with NULL-Fill

```yaml
datasets:
  - name: orders_us     # maps: date, region, revenue
  - name: orders_apac   # maps: date, revenue (NO region)
```

**Query:** `SELECT date, region, revenue`

**Resolution:**
1. orders_us: fully mapped -> Project(date, region, revenue)
2. orders_apac: region unmapped -> Project(date, NULL, revenue)
3. UNION ALL:
   ```
   (2024-01, "West", 100)     -- from orders_us
   (2024-01, NULL,   200)     -- from orders_apac (no region)
   ```
4. Re-aggregate GROUP BY (date, region):
   ```
   (2024-01, "West", 100)     -- US rows grouped by region
   (2024-01, NULL,   200)     -- APAC rows grouped under NULL region
   ```

**Result:** APAC rows appear with NULL region. The NULL is semantically correct — the data source doesn't have region information.

### 4.3 Metadata and Literal Dimensions

```yaml
dimensions:
  - name: date
  - name: source
    type: metadata
    path: { token: 1 }
  - name: market
measures:
  - name: revenue
    agg: sum

datasets:
  - name: us_orders
    extras:
      storage:
        paths: ["bucket/us/orders.parquet"]
      column_mapping:
        date: order_date
        market: { literal: "domestic" }
        revenue: amount
```

**Query:** `SELECT date, source, market, revenue`

**Resolution per branch (us_orders):**
1. `date` -> Physical: `order_date`
2. `source` -> MetadataLiteral: path token[1] = "us" -> `Expr::string("us")`
3. `market` -> MetadataLiteral: literal -> `Expr::string("domestic")`
4. `revenue` -> lowered normally

Project: `(date, "us" AS source, "domestic" AS market, revenue)`

Metadata and literal dimensions are injected as constants in the Project node, not scanned from physical data.

### 4.4 UNION DISTINCT Mode

```yaml
type: unionset
  mode: unique
```

**Query:** `SELECT date, region, revenue`

**Resolution:**
1. Build branches identically to UNION ALL
2. UnionNode with `distinct: true`
3. Duplicate rows across datasets are eliminated before re-aggregation
4. Re-aggregation proceeds normally

Use case: when datasets may contain overlapping rows and deduplication is required before aggregation.

### 4.5 Single Dataset (No Union)

When a unionset kind has only one dataset, the Union node is skipped entirely:

```
Scan -> Aggregate -> Project -> Aggregate (re-aggregate)
```

The re-aggregation still occurs to maintain consistent output structure regardless of dataset count.

---

## 5. Error Cases

### 5.1 Compile-Time Errors

| Error | When |
|-------|------|
| Interface name not mapped by any dataset | Union coverage check — dimension/measure defined but no dataset maps it |

### 5.2 Plan-Time Errors

| Error | When |
|-------|------|
| No datasets | Unionset kind has empty datasets list |

Note: unlike grainset and joinset, unionset does not produce measure-not-found errors at plan time. Unmapped measures are NULL-filled, not rejected. The compile-time union coverage check ensures at least one dataset maps each interface name.

---

## 6. IR Nodes Produced

### 6.1 Single-Dataset Plan

```
ScanNode (physical columns)
  -> AggNode (pre-aggregate: GROUP BY physical dims, aggregate covered measures)
    -> ProjectNode (unified schema with NULL-fill)
      -> AggNode (re-aggregate: GROUP BY semantic dims, infer_aggregation per measure)
```

### 6.2 Multi-Dataset UNION ALL Plan

```
For each dataset:
  ScanNode (physical columns)
    -> AggNode (pre-aggregate: GROUP BY physical dims, aggregate covered measures)
      -> ProjectNode (unified schema: mapped fields + NULL-fill)

UnionNode (inputs: all branches, distinct: false)
  -> AggNode (re-aggregate: GROUP BY semantic dims, infer_aggregation per measure)
```

### 6.3 Multi-Dataset UNION DISTINCT Plan

```
For each dataset:
  ScanNode -> AggNode -> ProjectNode (same as UNION ALL)

UnionNode (inputs: all branches, distinct: true)
  -> AggNode (re-aggregate: GROUP BY semantic dims, infer_aggregation per measure)
```

The only difference from UNION ALL is `distinct: true` on the UnionNode. The SQL emitter translates this to `UNION DISTINCT` instead of `UNION ALL`.
