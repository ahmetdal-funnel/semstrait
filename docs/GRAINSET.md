# Grainset Resolution Strategy

**Status:** Implemented
**Scope:** Planner, Manifest (validation + compilation), IR
**Taxonomy:** Complex kind (multi-dataset). For the Simple kind (single-dataset fast path), see `DATASET.md`. Peers: `UNIONSET.md`, `JOINSET.md`.

---

## 1. Definition

A **grainset** is a collection of unique datasets that are **semantically equivalent** but provide data at **different grains**.

- **Semantics**: dimensions, measures, metrics, keys — the semantic elements of the kind interface.
- **Datasets**: each dataset is a collection of paths or tables sharing the same physical structure, mapped to the kind's semantic interface via `column_mapping`.
- **Grains** express the resolution of data along two axes:
  - **Vertical grain** (temporal): the inline value granularity of a column — e.g., a `date` column at day, week, or month grain. Expressed through truncation/conversion of the column value (DATE_TRUNC).
  - **Horizontal grain** (schema): the number of dimensions/measures available in a dataset's mapping — a dataset that maps 25 of 46 dimensions has a coarser horizontal grain than one that maps 40.

---

## 2. Model Structure

### 2.1 Kind-Level Temporal Dimension

```yaml
dimensions:
  - name: date
    type: temporal
    grains: [day, week, month, quarter, year]   # all possible grains for this kind
```

The `grains` list declares all grain levels consumers can request. The planner validates that the requested grain is in this list.

### 2.2 Dataset-Level Column Mapping with Grain

```yaml
datasets:
  - name: google_daily
    extras:
      column_mapping:
        date:
          column: reporting_date
          grain: day                # native grain of this column in this dataset
        cost: adwords_cost
        clicks: adwords_clicks

  - name: google_monthly
    extras:
      column_mapping:
        date:
          column: report_month
          grain: month              # coarser native grain
        cost: monthly_cost
        # clicks NOT mapped — not available at monthly grain
```

**Grain defaults**: if `grain` is omitted from a temporal column mapping, all kind-level grains are assumed available (the data is at the finest resolution and supports rollup to any coarser grain).

### 2.3 Partial Mapping

Datasets map only the semantics they provide. The compile-time invariant is **union coverage**: every interface name must be mapped by at least one dataset across the kind.

```yaml
# Valid: union of all datasets covers the full interface
datasets:
  - name: google_daily    # maps: date, cost, clicks, impressions
  - name: bing_daily      # maps: date, cost, clicks
  - name: tiktok_daily    # maps: date, cost, total_plays, likes
```

### 2.4 Keys

```yaml
keys:
  primary: [date, campaign_id]
```

Primary keys identify the natural grain of the entity. Used for join conditions in multi-grain combination and for validation.

---

## 3. Resolution Algorithm

### 3.1 Overview

```
Query Request
     |
     v
[1] PRUNE DATASETS
     |  1a. Metadata filter pruning
     |  1b. Literal dimension filter pruning
     |  1c. Grain eligibility pruning
     |  1d. Zero-coverage pruning
     v
[2] ASSIGN MEASURES TO GRAIN GROUPS
     |  Group datasets by native temporal grain
     |  Assign each measure to cheapest grain group
     v
[3] BUILD PLAN
     |  Per-dataset: Scan -> DATE_TRUNC -> Pre-aggregate -> Project (NULL-fill)
     |  UNION ALL all branches
     |  Re-aggregate
     v
  PlanFragment
```

### 3.2 Step 1: Prune Datasets

Four sequential pruning stages narrow the dataset candidates before planning.

**1a. Metadata filter pruning** (existing)

Datasets are excluded if a user's equality filter on a metadata dimension (extracted from source path/partition) doesn't match the dataset's extracted value.

```
Filter: platform = "google"
Dataset google_daily: path = "bucket/google/..." → extract token[0] = "google" → MATCH
Dataset tiktok_daily: path = "bucket/tiktok/..." → extract token[0] = "tiktok" → EXCLUDE
```

**1b. Literal dimension filter pruning** (new)

Same logic as metadata pruning, but for `Literal` column mapping values. If a filter targets a literal dimension and the dataset's literal value doesn't match, exclude the dataset.

```
Filter: campaign_category = "search"
Dataset google_daily:   campaign_category: { lit: "search" }  → MATCH
Dataset facebook_daily: campaign_category: { lit: "social" }  → EXCLUDE
```

**1c. Grain eligibility pruning** (new)

If the query specifies a temporal grain, exclude datasets whose native grain is **coarser** than the requested grain (can't disaggregate).

```
Request grain: day
Dataset google_daily:   date grain = day   → day <= day   → ELIGIBLE
Dataset google_monthly: date grain = month → month > day  → EXCLUDE (can't disaggregate)
```

If no request grain is specified, no grain pruning occurs.

**1d. Zero-coverage pruning**

Exclude datasets that map zero requested semantics (keys + dimensions + metrics + measures ). These contribute nothing to the query.

### 3.3 Step 2: Assign Measures or Metric to Grain Groups

After pruning, group remaining datasets by their native temporal grain.

```
Remaining datasets:
  google_daily   (grain=day):   maps [date, cost, clicks, impressions]
  bing_daily     (grain=day):   maps [date, cost, clicks]
  google_monthly (grain=month): maps [date, cost]

Grain groups:
  day:   {google_daily, bing_daily}
  month: {google_monthly}
```

For each requested measure or metric, assign it to the **cheapest (coarsest) grain group** whose union of datasets includes that measure:

```
Request: date (grain=month), cost, clicks

Measure/Metric assignment:
  cost   → month group (google_monthly has cost; month is coarser = cheaper)
  clicks → day group   (only day group has clicks; month group doesn't)
```

**Error case**: if a measure cannot be assigned to any eligible grain group:
```
Error: measure 'clicks' cannot be provided at grain 'day'
       — no dataset maps 'clicks' at grain 'day' or finer
```

### 3.4 Step 3: Build Plan

Each dataset contributes only the measures assigned to its grain group.

**Per-dataset branch:**

```
google_daily:
  Scan(reporting_date, adwords_clicks)
  → DATE_TRUNC(reporting_date, 'month') AS date    # rollup to requested grain
  → Pre-aggregate: GROUP BY (date), SUM(clicks)
  → Project: (date, NULL AS cost, clicks)           # NULL-fill cost (assigned to month group)

bing_daily:
  Scan(report_date, bing_clicks)
  → DATE_TRUNC(report_date, 'month') AS date
  → Pre-aggregate: GROUP BY (date), SUM(clicks)
  → Project: (date, NULL AS cost, clicks)

google_monthly:
  Scan(report_month, monthly_cost)
  → Pre-aggregate: GROUP BY (date), SUM(cost)        # no DATE_TRUNC (native = requested)
  → Project: (date, cost, NULL AS clicks)
```

**Combine:**

```
UNION ALL (google_daily_branch, bing_daily_branch, google_monthly_branch)
→ Re-aggregate: GROUP BY (date), SUM(cost), SUM(clicks)
```

**Result:** `(date_month, total_cost, total_clicks)` — cost from month group only, clicks from day group rolled up. No double-counting because each measure is sourced from exactly one grain group.

**Single-dataset optimization:** if pruning + assignment leaves exactly one dataset, skip UNION ALL and build a simple Scan → Aggregate → Project plan.

---

## 4. Detailed Scenarios

### 4.1 Platform Union (Paid Media)

```yaml
kind: paid_media_campaign_performance
type: grainset
dimensions:
  - name: date
    type: temporal
    grains: [day, week, month, quarter, year]
  - name: platform          # metadata: extracted from path
    type: metadata
    path: { token: 1 }
  - name: campaign_category  # literal: injected per dataset
  - name: campaign_id
  - name: adgroup_placement  # TikTok-specific
measures:
  - name: cost              # all platforms
    agg: sum
  - name: clicks            # all platforms
    agg: sum
  - name: total_plays       # TikTok only
    agg: sum

datasets:
  - name: google_daily       # maps: date(day), campaign_id, cost, clicks
    extras:
      column_mapping:
        campaign_category: { lit: "search" }
        date: { column: reporting_date, grain: day }
        campaign_id: adwords_campaign_id
        cost: adwords_cost
        clicks: adwords_clicks

  - name: tiktok_daily       # maps: date(day), campaign_id, adgroup_placement, cost, clicks, total_plays
    extras:
      column_mapping:
        campaign_category: { lit: "social" }
        date: { column: stat_date, grain: day }
        campaign_id: tiktok_campaign_id
        adgroup_placement: placement_type
        cost: tiktok_spend
        clicks: tiktok_clicks
        total_plays: tiktok_plays
```

**Query:** `SELECT date (grain=month), cost, total_plays`

**Resolution:**
1. Prune: no metadata/literal filters in query → all datasets remain
2. Grain eligibility: both at day grain, request=month → both eligible (day ≤ month)
3. Grain groups: `day: {google_daily, tiktok_daily}` (only one group)
4. Assign measures: cost → day group, total_plays → day group
5. Build plan:
   - google_daily: DATE_TRUNC(date, month), SUM(cost), NULL AS total_plays
   - tiktok_daily: DATE_TRUNC(date, month), SUM(cost), SUM(total_plays)
   - UNION ALL → re-aggregate GROUP BY date: SUM(cost), SUM(total_plays)
6. Result: combined cost from both platforms, total_plays from TikTok only

**Query with literal filter:** `SELECT date, cost WHERE campaign_category = 'search'`

**Resolution:**
1. Prune 1b (literal): campaign_category='search' → google_daily MATCH, tiktok_daily EXCLUDE
2. Single dataset google_daily → Scan → Aggregate → Project (no UNION)

### 4.2 Multi-Grain Temporal (Orders)

```yaml
kind: order_performance
type: grainset
dimensions:
  - name: date
    type: temporal
    grains: [day, week, month]
  - name: region
measures:
  - name: revenue
    agg: sum
  - name: unique_customers
    agg: count_distinct

datasets:
  - name: orders_daily
    extras:
      column_mapping:
        date: { column: order_date, grain: day }
        region: ship_region
        revenue: order_amount
        unique_customers: customer_id

  - name: orders_monthly
    extras:
      column_mapping:
        date: { column: report_month, grain: month }
        region: ship_region
        revenue: monthly_revenue
        # unique_customers NOT mapped at monthly grain
```

**Query:** `SELECT date (grain=month), region, revenue`

**Resolution:**
1. Grain groups: `day: {orders_daily}`, `month: {orders_monthly}`
2. Both eligible (day ≤ month, month ≤ month)
3. Assign revenue: month group has it → cheapest → assigned to month group
4. Single dataset orders_monthly → Scan → Aggregate → Project
5. No DATE_TRUNC needed (native grain = requested grain)

**Query:** `SELECT date (grain=month), region, revenue, unique_customers`

**Resolution:**
1. Grain groups: `day: {orders_daily}`, `month: {orders_monthly}`
2. Assign measures:
   - revenue → month group (cheapest that has it)
   - unique_customers → day group (only day group has it)
3. Build plan:
   - orders_daily: DATE_TRUNC(order_date, month), GROUP BY (date, region), COUNT_DISTINCT(customer_id) → Project: (date, region, NULL AS revenue, unique_customers)
   - orders_monthly: GROUP BY (date, region), SUM(monthly_revenue) → Project: (date, region, revenue, NULL AS unique_customers)
   - UNION ALL → re-aggregate: GROUP BY (date, region), SUM(revenue), SUM(unique_customers)
4. Note: unique_customers re-aggregated as SUM is lossy for COUNT_DISTINCT (v1 known limitation)

**Query:** `SELECT date (grain=day), revenue, unique_customers`

**Resolution:**
1. Grain eligibility: orders_monthly has grain=month, request=day → month > day → EXCLUDE
2. Only orders_daily remains → single dataset plan
3. Scan → Aggregate → Project

**Query:** `SELECT date (grain=day), revenue` (but revenue only in monthly)

Wait — revenue IS in orders_daily too. Both datasets map revenue. So:
1. Grain eligibility: orders_monthly EXCLUDED (month > day)
2. orders_daily covers revenue → single dataset plan

If revenue were ONLY in orders_monthly and query asked for grain=day:
1. orders_monthly EXCLUDED (can't disaggregate)
2. No dataset maps revenue at eligible grain → Error: "measure 'revenue' cannot be provided at grain 'day'"

### 4.3 Horizontal Grain (Schema Drift)

Datasets with different column subsets but no temporal grain differences.

```yaml
kind: campaign_analytics
type: grainset
dimensions:
  - name: date
    type: temporal
    grains: [day]
  - name: campaign_id
  - name: region            # only in dataset A
  - name: device_type       # only in dataset B
measures:
  - name: cost
    agg: sum
  - name: clicks
    agg: sum

datasets:
  - name: campaign_geo      # has region, no device_type
    extras:
      column_mapping:
        date: { column: dt, grain: day }
        campaign_id: cid
        region: geo_region
        cost: spend
        clicks: click_count

  - name: campaign_device   # has device_type, no region
    extras:
      column_mapping:
        date: { column: dt, grain: day }
        campaign_id: cid
        device_type: device
        cost: spend
        clicks: click_count
```

**Query:** `SELECT date, campaign_id, region, device_type, cost, clicks`

**Resolution:**
1. All datasets at same grain (day) → one grain group
2. Both datasets map cost and clicks → all measures assigned to day group
3. Build:
   - campaign_geo: (date, campaign_id, region, NULL AS device_type, cost, clicks)
   - campaign_device: (date, campaign_id, NULL AS region, device_type, cost, clicks)
   - UNION ALL → re-aggregate GROUP BY (date, campaign_id, region, device_type)
4. Result: separate rows per (region, device_type) combination. Google rows have region but NULL device_type; device rows have device_type but NULL region. This is semantically correct — the data sources genuinely don't have the other dimension.

**Query:** `SELECT date, cost` (both datasets have it)

**Resolution:**
1. Both datasets map cost → both contribute
2. campaign_geo: GROUP BY date, SUM(cost) → (date, cost)
3. campaign_device: GROUP BY date, SUM(cost) → (date, cost)
4. UNION ALL → re-aggregate: GROUP BY date, SUM(cost)
5. Result: combined cost from both sources

### 4.4 Grain Rollup with DATE_TRUNC

When a dataset's native grain is finer than the requested grain, the planner emits `DATE_TRUNC` in the pre-aggregation GROUP BY.

```
Native grain: day
Requested grain: month

Physical column: reporting_date (contains '2024-01-15')

Pre-aggregate GROUP BY:
  DATE_TRUNC(reporting_date, 'month')   →  '2024-01-01'

This truncation happens BEFORE aggregation, so all January rows
group together under '2024-01-01'.
```

**Grain compatibility rules:**

| Native Grain | Request Grain | Action |
|-------------|---------------|--------|
| day | month | DATE_TRUNC(col, 'month') |
| day | day | No transformation |
| month | month | No transformation |
| month | day | **Error** — cannot disaggregate |
| (unspecified) | any | Treated as finest kind-level grain; DATE_TRUNC if coarser requested |

### 4.5 Dataset Pruning by Literal Filters

Literal dimensions have known constant values per dataset. When a query filters on a literal dimension, datasets whose literal value doesn't match are pruned before planning.

```
Kind definition:
  campaign_category dimension (categorical)

Dataset mappings:
  google_daily:   campaign_category: { lit: "search" }
  bing_daily:     campaign_category: { lit: "search" }
  facebook_daily: campaign_category: { lit: "social" }
  tiktok_daily:   campaign_category: { lit: "social" }

Query: WHERE campaign_category = 'search'

After pruning: {google_daily, bing_daily}  (facebook, tiktok excluded)
```

This pruning eliminates unnecessary dataset reads at plan time, reducing scan scope.

### 4.6 Metadata Pruning (Existing)

```
Metadata dimension: platform (path extraction, token: 1)

Dataset sources:
  google_daily:  paths: ["bucket/google/account1/", ...]
  tiktok_daily:  paths: ["bucket/tiktok/account1/", ...]

Query: WHERE platform = 'google'

Extraction: google_daily → token[1] = "google" → MATCH
            tiktok_daily → token[1] = "tiktok" → EXCLUDE
```

---

## 5. Re-aggregation Semantics

After UNION ALL, a final re-aggregation combines rows from different branches.

### 5.1 Aggregation Function Inference

| Original Aggregation | Re-aggregation | Correctness |
|---------------------|----------------|-------------|
| SUM | SUM | Exact |
| COUNT | SUM | Exact (sum of partial counts) |
| MIN | MIN | Exact |
| MAX | MAX | Exact |
| COUNT_DISTINCT | SUM | **Lossy** — overcounts when same value appears in multiple datasets |
| AVG | SUM | **Lossy** — not decomposable (would need SUM+COUNT preservation) |

Lossy re-aggregation is a v1 known limitation, consistent with unionset behavior.

### 5.2 NULL-fill and SUM

Measures not mapped by a dataset are NULL-filled in the project step. During re-aggregation, `SUM(NULL) = NULL` — only non-NULL contributions are summed. This ensures measures are sourced only from their assigned grain group.

```
Branch A: (2024-01, 100, NULL)    -- cost from month group
Branch B: (2024-01, NULL, 5000)   -- clicks from day group

UNION ALL:
  (2024-01, 100,  NULL)
  (2024-01, NULL, 5000)

Re-aggregate GROUP BY date:
  SUM(cost)   = 100    (NULL ignored)
  SUM(clicks) = 5000   (NULL ignored)

Result: (2024-01, 100, 5000)
```

---

## 6. Error Cases

### 6.1 Compile-Time Errors

| Error | When |
|-------|------|
| Interface name not mapped by any dataset | Union coverage check fails — a dimension/measure defined in the kind interface has no mapping in any dataset |
| Incompatible temporal grain definitions | Multiple temporal dimensions in a kind with conflicting grain hierarchies |
| Request grain not in kind's grain list | Requested grain (e.g., 'hour') not declared in dimension's `grains` list |

### 6.2 Plan-Time Errors

| Error | When |
|-------|------|
| Measure cannot be provided at grain | No eligible grain group (after pruning) maps the requested measure |
| No datasets after pruning | All datasets excluded by metadata/literal/grain filters |
| Grain disaggregation impossible | Request asks for finer grain than any dataset provides |

---

## 7. IR Nodes Produced

### 7.1 Single-Dataset Plan

```
ScanNode (physical columns)
  → AggNode (GROUP BY dims, aggregate measures)
    → ProjectNode (semantic names, literal injection, grain rollup)
```

### 7.2 Multi-Dataset UNION ALL Plan

```
For each dataset:
  ScanNode
    → ProjectNode (DATE_TRUNC for grain rollup)
      → AggNode (pre-aggregate)
        → ProjectNode (NULL-fill to unified schema)

UnionNode (inputs: all branches, distinct: false)
  → AggNode (re-aggregate: GROUP BY dims, infer_aggregation per measure)
```

---

## 8. Related Documentation

- `docs/DATASET.md` — single-dataset (Simple kind) planning; referenced for single-dataset fast path behavior.
- `docs/UNIONSET.md`, `docs/JOINSET.md` — peer Complex kinds.
- `crates/semstrait-planner/src/data_kind/grainset.rs` — implementation.
- `test_data/paid_media_kind.yaml` — working grainset fixture.
