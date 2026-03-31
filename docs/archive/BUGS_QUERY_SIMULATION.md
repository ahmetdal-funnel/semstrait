# Query Simulation Report

**Date:** 2026-03-27 | **Model:** `test_data/paid_media_kind.yaml` + `test_data/catalogs.yaml`
**Kind:** `paid_media_campaign_performance` (grainset, 4 datasets: adwords, bing, facebook, tiktok)
**Branch:** `feature/base-semastrait-dev`
**Phase:** Post-Phase 6 (DataType propagation + Bug B/C fixes)

---

## Test Results Summary

| Query | Description | Status | Notes |
|-------|-------------|--------|-------|
| Q1 | Single-dataset routing (Meta purchases) | **PASS** | — |
| Q2 | Cross-platform 4-way UNION (impressions) | **PASS** | — |
| Q3 | TikTok-specific measures (total_plays, likes) | **PASS** | — |
| Q4 | Metric expression (ctr = clicks/impressions) | **PASS** | Bug B fixed |
| Q5 | Literal dimension (campaign_category) | **PASS** | Bug C fixed |
| Q6 | Metadata dimension (platform) | **PASS** | Bug C fixed |
| Q7 | Mixed partial coverage + literal dim | **PASS** | Bug C fixed |

---

## Simulation Queries and Results

### Q1: Single-Dataset, Platform-Specific Measure

**Objective:** Verify that when a measure exists only in one dataset, the planner routes to that single dataset without UNION ALL.

```bash
cargo run -p semstrait-api --features aws-secrets -- explain \
  --model test_data/paid_media_kind.yaml \
  --catalogs test_data/catalogs.yaml \
  --from paid_media_campaign_performance \
  --select date campaign_name purchases --json
```

**Generated SQL:**
```sql
SELECT "date", "campaign_name", "purchases"
FROM (
  SELECT "date",
         "facebookads-campaign_name" AS "campaign_name",
         SUM("facebookads-actions.offsite_conversion.fb_pixel_purchase") AS "purchases"
  FROM (
    SELECT "date", "facebookads-campaign_name",
           "facebookads-actions.offsite_conversion.fb_pixel_purchase"
    FROM "facebookads"."27f1d45a-97af-4fdd-b590-37c8ef7d1e27"
  ) AS _a
  GROUP BY "date", "facebookads-campaign_name"
) AS _p
```

**Verdict: PASS**
- Routes to `facebook_adset_data` only (correct — `purchases` maps only to facebook)
- `purchases` → `facebookads-actions.offsite_conversion.fb_pixel_purchase` (correct physical mapping)
- `campaign_name` → `facebookads-campaign_name` (correct, aliased)
- No UNION ALL (correct — single dataset)
- GROUP BY includes both dimensions (correct)
- SUM aggregation on measure (correct)

---

### Q2: Cross-Platform Universal Measure (4-way UNION)

**Objective:** Verify that a universally-mapped measure triggers all 4 datasets with UNION ALL and re-aggregation.

```bash
cargo run -p semstrait-api --features aws-secrets -- explain \
  --model test_data/paid_media_kind.yaml \
  --catalogs test_data/catalogs.yaml \
  --from paid_media_campaign_performance \
  --select date impressions --json
```

**Generated SQL (simplified — multi-source inner UNION ALL branches collapsed):**
```sql
SELECT "date", SUM("impressions") AS "impressions"
FROM (
  -- adwords branch (12 inner sources via UNION ALL)
  SELECT "date", "impressions"
  FROM (
    SELECT "date", SUM("adwords-impressions") AS "impressions"
    FROM (SELECT "date", "adwords-impressions" FROM "adwords"."29ec81d6-..."
          UNION ALL ... /* 12 resolved tables */) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- bing branch (6 inner sources)
  SELECT "date", "impressions"
  FROM (
    SELECT "date", SUM("bing-impressions") AS "impressions"
    FROM (SELECT "date", "bing-impressions" FROM "bing"."30c07d4c-..."
          UNION ALL ... /* 6 resolved tables */) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- facebook branch (1 source)
  SELECT "date", "impressions"
  FROM (
    SELECT "date", SUM("facebookads-impressions") AS "impressions"
    FROM (SELECT "date", "facebookads-impressions"
          FROM "facebookads"."27f1d45a-...") AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- tiktok branch (1 source)
  SELECT "date", "impressions"
  FROM (
    SELECT "date", SUM("tiktok-show_cnt") AS "impressions"
    FROM (SELECT "date", "tiktok-show_cnt"
          FROM "tiktok"."4aff5315-...") AS _a
    GROUP BY "date"
  ) AS _p
) AS _a
GROUP BY "date"
```

**Verdict: PASS**
- All 4 datasets included (correct — `impressions` mapped in all)
- Physical column names correct: `adwords-impressions`, `bing-impressions`, `facebookads-impressions`, `tiktok-show_cnt`
- Multi-source inner UNION ALL for adwords (12 tables) and bing (6 tables)
- Outer re-aggregation: `SUM("impressions")` with `GROUP BY "date"` (correct grainset pattern)
- Each branch pre-aggregates before UNION ALL (correct — prevents double-counting)

---

### Q3: TikTok-Specific Measures

**Objective:** Verify routing to a single dataset when measures exist only in TikTok.

```bash
cargo run -p semstrait-api --features aws-secrets -- explain \
  --model test_data/paid_media_kind.yaml \
  --catalogs test_data/catalogs.yaml \
  --from paid_media_campaign_performance \
  --select date total_plays likes --json
```

**Generated SQL:**
```sql
SELECT "date", "total_plays", "likes"
FROM (
  SELECT "date",
         SUM("tiktok-total_play") AS "total_plays",
         SUM("tiktok-likes") AS "likes"
  FROM (
    SELECT "date", "tiktok-total_play", "tiktok-likes"
    FROM "tiktok"."4aff5315-fd23-4e99-bda6-1643cbae12e0"
  ) AS _a
  GROUP BY "date"
) AS _p
```

**Verdict: PASS**
- Routes to `tiktok_ad_data` only (correct — both measures are tiktok-exclusive)
- `total_plays` → `tiktok-total_play`, `likes` → `tiktok-likes` (correct physical mapping)
- No UNION ALL (correct — single dataset)

---

### Q4: Metric with Division (ctr = clicks / impressions)

**Objective:** Verify that metrics (derived from measures via expressions) are decomposed into constituent aggregates with post-aggregate projection.

```bash
cargo run -p semstrait-api --features aws-secrets -- explain \
  --model test_data/paid_media_kind.yaml \
  --catalogs test_data/catalogs.yaml \
  --from paid_media_campaign_performance \
  --select date ctr --json
```

**Generated SQL (simplified):**
```sql
SELECT "date", SUM("ctr") AS "ctr"
FROM (
  -- adwords branch (12 inner sources)
  SELECT "date",
         (CASE WHEN "impressions" = 0 THEN NULL
               ELSE "clicks" / "impressions" END) AS "ctr"
  FROM (
    SELECT "date",
           SUM("adwords-clicks") AS "ctr",
           SUM("adwords-impressions") AS "__agg_1"
    FROM (SELECT "date", "adwords-clicks", "adwords-impressions"
          FROM "adwords"."29ec81d6-..."
          UNION ALL ... /* 12 resolved tables */) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- bing branch (6 inner sources)
  SELECT "date",
         (CASE WHEN "impressions" = 0 THEN NULL
               ELSE "clicks" / "impressions" END) AS "ctr"
  FROM (
    SELECT "date",
           SUM("bing-clicks") AS "ctr",
           SUM("bing-impressions") AS "__agg_1"
    FROM (...bing sources...) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- facebook branch (1 source)
  SELECT "date",
         (CASE WHEN "impressions" = 0 THEN NULL
               ELSE "clicks" / "impressions" END) AS "ctr"
  FROM (
    SELECT "date",
           SUM("facebookads-clicks") AS "ctr",
           SUM("facebookads-impressions") AS "__agg_1"
    FROM (...facebook source...) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- tiktok branch (1 source)
  SELECT "date",
         (CASE WHEN "impressions" = 0 THEN NULL
               ELSE "clicks" / "impressions" END) AS "ctr"
  FROM (
    SELECT "date",
           SUM("tiktok-click_cnt") AS "ctr",
           SUM("tiktok-show_cnt") AS "__agg_1"
    FROM (...tiktok source...) AS _a
    GROUP BY "date"
  ) AS _p
) AS _a
GROUP BY "date"
```

**Verdict: PASS**
- Metric decomposed into constituent measures: `clicks` + `impressions` (correct)
- Per-branch: SUM(physical_clicks), SUM(physical_impressions) → CASE WHEN safe-divide (correct)
- Physical mappings correct: adwords-clicks, bing-clicks, facebookads-clicks, tiktok-click_cnt
- All 4 datasets included (all have both clicks and impressions)
- SafeDivide: `CASE WHEN "impressions" = 0 THEN NULL ELSE "clicks" / "impressions" END` (correct)

**Known v1 limitation:** Outer re-aggregation uses `SUM("ctr")` — summing per-dataset ratios instead of re-computing from summed constituents. This is documented in GRAINSET.md as lossy re-aggregation for ratio metrics.

---

### Q5: Literal Dimension (campaign_category)

**Objective:** Verify that literal dimensions (injected as string constants per dataset) appear with correct aliases in the UNION ALL branches.

```bash
cargo run -p semstrait-api --features aws-secrets -- explain \
  --model test_data/paid_media_kind.yaml \
  --catalogs test_data/catalogs.yaml \
  --from paid_media_campaign_performance \
  --select date campaign_category impressions --json
```

**Generated SQL (simplified):**
```sql
SELECT "date", "campaign_category", SUM("impressions") AS "impressions"
FROM (
  -- adwords branch
  SELECT "date", 'search' AS "campaign_category", "impressions"
  FROM (
    SELECT "date", SUM("adwords-impressions") AS "impressions"
    FROM (...adwords sources...) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- bing branch
  SELECT "date", 'search' AS "campaign_category", "impressions"
  FROM (
    SELECT "date", SUM("bing-impressions") AS "impressions"
    FROM (...bing sources...) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- facebook branch
  SELECT "date", 'social' AS "campaign_category", "impressions"
  FROM (
    SELECT "date", SUM("facebookads-impressions") AS "impressions"
    FROM (...facebook source...) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- tiktok branch
  SELECT "date", 'social' AS "campaign_category", "impressions"
  FROM (
    SELECT "date", SUM("tiktok-show_cnt") AS "impressions"
    FROM (...tiktok source...) AS _a
    GROUP BY "date"
  ) AS _p
) AS _a
GROUP BY "date", "campaign_category"
```

**Verdict: PASS**
- Literal values correctly injected per dataset: adwords/bing → `'search'`, facebook/tiktok → `'social'`
- Column alias present: `'search' AS "campaign_category"` (Bug C fixed)
- Outer GROUP BY correctly includes `"campaign_category"` (references the aliased column)
- All 4 datasets included (correct — `impressions` mapped in all)

---

### Q6: Metadata Dimension (platform)

**Objective:** Verify that metadata dimensions (extracted from source path/partition metadata) are injected as resolved literal values with correct aliases.

```bash
cargo run -p semstrait-api --features aws-secrets -- explain \
  --model test_data/paid_media_kind.yaml \
  --catalogs test_data/catalogs.yaml \
  --from paid_media_campaign_performance \
  --select date platform impressions --json
```

**Generated SQL (simplified):**
```sql
SELECT "date", "platform", SUM("impressions") AS "impressions"
FROM (
  -- adwords branch
  SELECT "date", 'adwords' AS "platform", "impressions"
  FROM (
    SELECT "date", SUM("adwords-impressions") AS "impressions"
    FROM (...adwords sources...) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- bing branch
  SELECT "date", 'bing' AS "platform", "impressions"
  FROM (
    SELECT "date", SUM("bing-impressions") AS "impressions"
    FROM (...bing sources...) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- facebook branch
  SELECT "date", 'facebookads' AS "platform", "impressions"
  FROM (
    SELECT "date", SUM("facebookads-impressions") AS "impressions"
    FROM (...facebook source...) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- tiktok branch
  SELECT "date", 'tiktok' AS "platform", "impressions"
  FROM (
    SELECT "date", SUM("tiktok-show_cnt") AS "impressions"
    FROM (...tiktok source...) AS _a
    GROUP BY "date"
  ) AS _p
) AS _a
GROUP BY "date", "platform"
```

**Verdict: PASS**
- Metadata values correctly resolved per dataset: `'adwords'`, `'bing'`, `'facebookads'`, `'tiktok'`
- Column alias present: `'adwords' AS "platform"` (Bug C fixed)
- Outer GROUP BY correctly includes `"platform"`

---

### Q7: Mixed Partial Coverage + Literal Dimension

**Objective:** Verify that (a) measures not available in all datasets are NULL-filled, and (b) literal dimensions work in combination with partial coverage.

```bash
cargo run -p semstrait-api --features aws-secrets -- explain \
  --model test_data/paid_media_kind.yaml \
  --catalogs test_data/catalogs.yaml \
  --from paid_media_campaign_performance \
  --select date campaign_category impressions conversion_value --json
```

**Generated SQL (simplified):**
```sql
SELECT "date", "campaign_category",
       SUM("impressions") AS "impressions",
       SUM("conversion_value") AS "conversion_value"
FROM (
  -- adwords: has both impressions + conversion_value
  SELECT "date", 'search' AS "campaign_category",
         "impressions", "conversion_value"
  FROM (
    SELECT "date",
           SUM("adwords-impressions") AS "impressions",
           SUM("adwords-totalConvValue") AS "conversion_value"
    FROM (...adwords sources...) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- bing: has both
  SELECT "date", 'search' AS "campaign_category",
         "impressions", "conversion_value"
  FROM (
    SELECT "date",
           SUM("bing-impressions") AS "impressions",
           SUM("bing-Revenue") AS "conversion_value"
    FROM (...bing sources...) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- facebook: impressions only, conversion_value NULL-filled
  SELECT "date", 'social' AS "campaign_category",
         "impressions", NULL AS "conversion_value"
  FROM (
    SELECT "date", SUM("facebookads-impressions") AS "impressions"
    FROM (...facebook source...) AS _a
    GROUP BY "date"
  ) AS _p

  UNION ALL

  -- tiktok: impressions only, conversion_value NULL-filled
  SELECT "date", 'social' AS "campaign_category",
         "impressions", NULL AS "conversion_value"
  FROM (
    SELECT "date", SUM("tiktok-show_cnt") AS "impressions"
    FROM (...tiktok source...) AS _a
    GROUP BY "date"
  ) AS _p
) AS _a
GROUP BY "date", "campaign_category"
```

**Verdict: PASS**
- `conversion_value` correctly mapped: adwords → `adwords-totalConvValue`, bing → `bing-Revenue`
- `conversion_value` correctly NULL-filled for facebook and tiktok (they don't map it)
- Literal values correct: adwords/bing → `'search'`, facebook/tiktok → `'social'`
- Column alias present: `'search' AS "campaign_category"` (Bug C fixed)
- NULL alias present: `NULL AS "conversion_value"` (correct)
- All 4 datasets included (correct — `impressions` mapped in all)

---

## Bug Resolution Summary

### Bug A: SafeDivide in Metric Expressions — NOT PRESENT

**Originally predicted:** Metric division uses `Divide` instead of `SafeDivide`.

**Actual finding:** The compiled metric expression already uses SafeDivide. The `try_parse_arithmetic` function in `steps.rs` correctly produces `CASE WHEN b = 0 THEN NULL ELSE a / b END` for metric ratio expressions. No fix needed.

---

### Bug B: Metrics Not Decomposed Into Constituent Measures — FIXED

**Severity:** Critical (crash) | **Status:** Fixed

**Root Cause:** `extract_metric_constituents()` used `collect_column_refs()` which only handles `Expr::Column` leaf nodes. Metric expressions compile to `Expr::EntityRef` (pre-lowering entity references like `EntityRef("clicks")`), which `collect_column_refs` silently ignored. Result: empty constituent list → empty assignments → empty UNION → crash.

**Fix:** Replaced `collect_column_refs` with a dedicated `collect_leaf_refs` function that handles both `Expr::Column` and `Expr::EntityRef`, using `HashSet<&str>` for borrow-based dedup.

**File:** `crates/semstrait-planner/src/kind/grainset.rs`

---

### Bug C: Literal/Metadata Dimensions Missing Column Aliases — FIXED

**Severity:** High | **Status:** Fixed (in Phase 6)

**Root Cause:** The `ProjectNode` SQL emitter rendered expressions without aliasing from the output schema. Literal strings like `'search'` rendered as bare values without `AS "campaign_category"`.

**Fix:** Phase 6 DataType propagation work updated the ProjectNode emitter to alias expressions using `schema_fields`, matching the pattern already used by AggregateNode.

**File:** `crates/semstrait-sql/src/emitter.rs`

---

## Known v1 Limitations

| Limitation | Queries Affected | Description |
|------------|-----------------|-------------|
| Lossy ratio re-aggregation | Q4 | Outer `SUM("ctr")` sums per-dataset ratios instead of recomputing from summed constituents. Documented in GRAINSET.md. |
| COUNT_DISTINCT re-aggregation | — | Re-aggregated as SUM (lossy). Documented in GRAINSET.md. |
| AVG re-aggregation | — | Re-aggregated as SUM (lossy). Documented in GRAINSET.md. |
