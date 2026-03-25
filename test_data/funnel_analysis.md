# Funnel Showcase — Model Restructuring Analysis

**Date:** 2026-03-24 | **Status:** In progress — iterating per category

---

## Category 1: Paid Media Campaign Performance (Unionset)

### Decision

Merge all paid media campaign-level datasets into a single **unionset** kind:
`paid_media_campaign_performance`

**Members (4 datasets):**

| Dataset | Platform | Accounts | Storage | campaign_category |
|---------|----------|----------|---------|-------------------|
| `adwords_campaign_data` | Google Ads | 6 | explicit paths (shared prefix with keyword_clicks) | `search` |
| `bing_campaign_data` | Bing/Microsoft | 6 | glob: `fs1henp3k8p1hqo/bing/*/` | `search` |
| `facebook_adset_data` | Facebook/Meta | 1 | glob: `fs1henp3k8p1hqo/facebookads/*/` | `social` |
| `tiktok_ad_data` | TikTok | 1 | glob: `fs1henp3k8p1hqo/tiktok/*/` | `social` |

### Metadata Dimensions

| Dimension | Type | Extraction | Example Values |
|-----------|------|-----------|----------------|
| `platform` | metadata | `path.token: 1` | adwords, bing, facebookads, tiktok |
| `funnel_account_id` | metadata | `path.token: 2` | 31952b77-..., 30c07d4c-..., etc. |
| `campaign_category` | categorical | **literal binding** (per dataset column_mapping) | `search`, `social` |

> **DECIDED — `campaign_category` uses ColumnMappingValue::Literal**
>
> The dimension is declared as regular categorical at the kind level.
> Each dataset binds it to a constant value via column_mapping:
>
> ```yaml
> # adwords dataset
> column_mapping:
>   campaign_category:
>     literal: "search"
>
> # facebook dataset
> column_mapping:
>   campaign_category:
>     literal: "social"
> ```
>
> **Implementation:** Extend `ColumnMappingValue` enum with `Literal(Literal)` variant.
> This parallels the existing `WithGrain { column, grain }` structured variant.
> No changes to dimension types needed — metadata stays for path/partition extraction only.
>
> ```rust
> enum ColumnMappingValue {
>     Simple(String),
>     WithGrain { column: String, grain: Option<Grain> },
>     Literal(Literal),  // NEW — string, number, boolean constants
> }
> ```

### Storage & Glob Notes

- **Bing:** All datasets under `fs1henp3k8p1hqo/bing/` are campaign performance → safe to glob `bing/*/`
- **Adwords:** Prefix `fs1henp3k8p1hqo/adwords/` contains BOTH campaign perf AND keyword clicks datasets → **cannot glob**, must list 6 explicit paths
- **Facebook:** Single account → `facebookads/*/` works (future-proof for additional accounts)
- **TikTok:** Single account → `tiktok/*/` works

### Adwords explicit paths (campaign performance only)

```
fs1henp3k8p1hqo/adwords/31952b77-699e-45ed-8982-5ef91d4bb613/
fs1henp3k8p1hqo/adwords/71f37169-bfc8-449a-80be-e1ec82633fc3/
fs1henp3k8p1hqo/adwords/e13fbc92-cb8b-499c-9011-b5e0ae72b5f4/
fs1henp3k8p1hqo/adwords/e329a345-294b-4dff-81e3-8b1cf3a4f887/
fs1henp3k8p1hqo/adwords/29ec81d6-caa9-4aa5-a737-f6a951f39496/
fs1henp3k8p1hqo/adwords/317f0b1b-2c44-4e35-902e-3d3af8195aec/
```

### Unified Interface — Shared Dimensions

These dimensions exist across all 4 platforms (mapped via column_mapping per dataset):

| Dimension | data_type | Notes |
|-----------|-----------|-------|
| `date` | date | temporal, grains: [day, week, month, quarter, year] |
| `currency` | string | |
| `country` | string | NULL-filled for Bing (no country in source) |
| `campaign_id` | string | key |
| `campaign_name` | string | |
| `platform` | string | metadata (path token 1) |
| `funnel_account_id` | string | metadata (path token 2) |
| `campaign_category` | string | categorical, bound via ColumnMappingValue::Literal per dataset |

### Unified Interface — Platform-Specific Dimensions

NULL-filled when queried across platforms. Grouped by source:

**Google Ads only:**
campaign_status, campaign_start_date, campaign_end_date, account_name, account_id,
advertising_channel, advertising_sub_channel, bidding_strategy_type, account_timezone,
optimization_score, target_roas_campaign, target_roas_max_conv, campaign_target_cpa,
tracking_url_template, customer_tracking_url_template, url_custom_parameters,
campaign_final_url_suffix, customer_final_url_suffix, conversion_name,
conversion_category, conversion_tracker_id, external_conversion_source

**Bing Ads only:**
account_name, account_id, account_number, account_status, campaign_status,
campaign_type (Bing), ad_distribution, campaign_labels, device_type, device_os,
network, top_vs_other, bid_match_type, delivered_match_type, tracking_template,
custom_parameters, budget_name, budget_status, budget_association_status, goal, goal_type

**Facebook/Meta only:**
campaign_effective_status, campaign_start_time, campaign_stop_time, objective,
buying_type, adset_id, adset_name, adset_effective_status, adset_start_time,
adset_end_time, optimization_goal, ad_account_id, attribution_setting, result_type

**TikTok only:**
campaign_objective, campaign_objective_type, campaign_secondary_status,
campaign_budget_mode, campaign_optimize_goal, campaign_bid_type,
campaign_buying_type, tiktok_campaign_type, adgroup_id, adgroup_name,
adgroup_secondary_status, adgroup_optimize_goal, adgroup_bid_type, adgroup_pacing,
adgroup_placement, adgroup_placement_type, adgroup_landing_page_url,
adgroup_interest_category, promotion_type, ad_id, ad_name, ad_secondary_status,
ad_call_to_action, ad_text, ad_video_id, ad_image_mode, ad_format,
country_region, stat_time_day

**Shared across some platforms:**
- `account_name` — Google + Bing (same semantic, same name)
- `account_id` — Google + Bing (same semantic, same name)
- `campaign_status` — Google + Bing (same semantic, same name)

### Unified Interface — Shared Measures

| Measure | agg | Platforms | Column mapping notes |
|---------|-----|-----------|---------------------|
| `impressions` | sum | all 4 | adwords-impressions, bing-impressions, facebookads-impressions, tiktok-show_cnt |
| `clicks` | sum | all 4 | adwords-clicks, bing-clicks, facebookads-clicks, tiktok-click_cnt |
| `cost` | sum | all 4 | adwords-cost, bing-cost, facebookads-spend, tiktok-stat_cost |
| `conversions` | sum | Google + Bing + TikTok | adwords-conversions, bing-Conversions, tiktok-convert_cnt. Facebook: NULL-filled (uses `purchases` instead) |
| `landing_page_views` | sum | FB + TikTok | facebookads-actions.landing_page_view, tiktok-total_landing_page_view |
| `view_through_conversions` | sum | Google + Bing | adwords-viewThroughConv, bing-ViewThroughConversions |
| `all_conversions` | sum | Google + Bing | adwords-allConv, bing-AllConversions |

### Unified Interface — Platform-Specific Measures

**Google Ads only:**
views, interactions, all_conversions_value, total_conversion_value,
cross_device_conversions, target_roas_numeric

**Bing Ads only:**
total_position, low_quality_clicks, low_quality_impressions,
low_quality_conversions, phone_impressions, phone_calls, bing_revenue, assists

**Facebook/Meta only:**
bid_amount, daily_budget, lifetime_budget, result_value, link_clicks,
add_to_cart, purchases, initiate_checkout, purchase_value, outbound_clicks

**TikTok only:**
total_plays, average_video_play,
play_2s, play_6s, play_25pct, play_50pct, play_75pct, play_100pct,
engaged_views, engaged_views_15s, likes, paid_follows, paid_shares,
paid_comments, profile_visits, complete_payments, campaign_budget, adgroup_budget

### Unified Metrics

| Metric | Expression | Platforms |
|--------|-----------|-----------|
| `ctr` | `clicks / impressions` | all |
| `cpc` | `cost / clicks` | all |
| `cpa` | `cost / conversions` | all |
| `roas` | `revenue / cost` | requires unified revenue measure (TBD — platform-specific revenue definitions differ) |
| `conversion_rate` | `conversions / clicks` | all |
| `cpm` | `(cost / impressions) * 1000` | all |

> **NOTE — `conversions` semantic alignment:** Google/Bing `conversions` = tracked conversions.
> Facebook `conversions` is not a direct field — `purchases` is the closest.
> TikTok `conversions` = convert_cnt. These may not be semantically identical.
> Consider whether `conversions` should be the unified name or if platform-specific
> conversion measures should remain separate.

### Column Mapping per Dataset

**adwords_campaign_data:**
```yaml
column_mapping:
  campaign_category:
    literal: "search"
  date: date
  campaign_name: adwords-campaign
  campaign_id: adwords-campaignID
  campaign_status: adwords-campaign_status
  campaign_start_date: adwords-campaign_start_date
  campaign_end_date: adwords-campaign_end_date
  account_name: adwords-accountName
  account_id: adwords-accountId
  currency: currency
  country: adwords-country
  advertising_channel: adwords-advertisingChannel
  advertising_sub_channel: adwords-advertisingSubChannel
  bidding_strategy_type: adwords-biddingStrategyType
  account_timezone: adwords-adAccountTimeZone
  optimization_score: adwords-optimization_score
  target_roas_campaign: adwords-target_roas_campaign
  target_roas_max_conv: adwords-targetRoasMaxConv
  campaign_target_cpa: adwords-campaign_target_cpa
  tracking_url_template: adwords-trackingUrlTemplate
  customer_tracking_url_template: adwords-customerTrackingUrlTemplate
  url_custom_parameters: adwords-urlCustomParameters
  campaign_final_url_suffix: adwords-campaign_final_url_suffix
  customer_final_url_suffix: adwords-customer_final_url_suffix
  conversion_name: adwords-conversionName
  conversion_category: adwords-conversionCategoryName
  conversion_tracker_id: adwords-conversionTrackerId
  external_conversion_source: adwords-externalConversionSource
  cost: adwords-cost
  clicks: adwords-clicks
  impressions: adwords-impressions
  views: adwords-views
  interactions: adwords-interactions
  conversions: adwords-conversions
  all_conversions: adwords-allConv
  all_conversions_value: adwords-allConvValue
  total_conversion_value: adwords-totalConvValue
  view_through_conversions: adwords-viewThroughConv
  cross_device_conversions: adwords-cross_device_conversions
  target_roas_numeric: "adwords-target_roas_campaign:funnel-numeric"
```

**bing_campaign_data:**
```yaml
column_mapping:
  campaign_category:
    literal: "search"
  date: date
  currency: currency
  account_name: bing-AccountName
  account_number: bing-AccountNumber
  account_id: bing-AccountId
  account_status: bing-AccountStatus
  campaign_name: bing-CampaignName
  campaign_id: bing-CampaignId
  campaign_status: bing-CampaignStatus
  bing_campaign_type: bing-CampaignType
  ad_distribution: bing-AdDistribution
  campaign_labels: bing-CampaignLabels
  device_type: bing-DeviceType
  device_os: bing-DeviceOS
  network: bing-Network
  top_vs_other: bing-TopVsOther
  bid_match_type: bing-BidMatchType
  delivered_match_type: bing-DeliveredMatchType
  tracking_template: bing-TrackingTemplate
  custom_parameters: bing-CustomParameters
  budget_name: bing-BudgetName
  budget_status: bing-BudgetStatus
  budget_association_status: bing-BudgetAssociationStatus
  goal: bing-Goal
  goal_type: bing-GoalType
  impressions: bing-impressions
  clicks: bing-clicks
  cost: bing-cost
  total_position: bing-TotalPosition
  low_quality_clicks: bing-LowQualityClicks
  low_quality_impressions: bing-LowQualityImpressions
  low_quality_conversions: bing-LowQualityConversions
  phone_impressions: bing-PhoneImpressions
  phone_calls: bing-PhoneCalls
  view_through_conversions: bing-ViewThroughConversions
  all_conversions: bing-AllConversions
  conversions: bing-Conversions
  bing_revenue: bing-Revenue
  assists: bing-Assists
```

**facebook_adset_data:**
```yaml
column_mapping:
  campaign_category:
    literal: "social"
  date: date
  currency: currency
  country: facebookads-country
  campaign_id: facebookads-campaign_id
  campaign_name: facebookads-campaign_name
  campaign_effective_status: facebookads-campaign_effective_status
  campaign_start_time: facebookads-campaign_start_time
  campaign_stop_time: facebookads-campaign_stop_time
  objective: facebookads-objective
  buying_type: facebookads-buying_type
  adset_id: facebookads-adset_id
  adset_name: facebookads-adset_name
  adset_effective_status: facebookads-adset_effective_status
  adset_start_time: facebookads-adset_start_time
  adset_end_time: facebookads-adset_end_time
  optimization_goal: facebookads-optimization_goal
  ad_account_id: facebookads-ad_account_id
  attribution_setting: facebookads-attribution_setting
  result_type: facebookads-result_type
  cost: facebookads-spend
  impressions: facebookads-impressions
  clicks: facebookads-clicks
  bid_amount: facebookads-bid_amount
  daily_budget: facebookads-daily_budget
  lifetime_budget: facebookads-lifetime_budget
  result_value: facebookads-result_value
  link_clicks: facebookads-actions.link_click
  landing_page_views: facebookads-actions.landing_page_view
  purchases: facebookads-actions.offsite_conversion.fb_pixel_purchase
  add_to_cart: facebookads-actions.offsite_conversion.fb_pixel_add_to_cart
  initiate_checkout: facebookads-actions.offsite_conversion.fb_pixel_initiate_checkout
  purchase_value: facebookads-action_values.offsite_conversion.fb_pixel_purchase
  outbound_clicks: "facebookads-outbound_clicks.outbound_click"
```

**tiktok_ad_data:**
```yaml
column_mapping:
  campaign_category:
    literal: "social"
  date: date
  currency: currency
  country: tiktok-country
  campaign_id: tiktok-campaign_id
  campaign_name: tiktok-campaign_name
  campaign_objective: tiktok-campaign_objective
  campaign_objective_type: tiktok-campaign_objective_type
  campaign_secondary_status: tiktok-campaign_secondary_status
  campaign_budget_mode: tiktok-campaign_budget_mode
  campaign_optimize_goal: tiktok-campaign_optimize_goal
  campaign_bid_type: tiktok-campaign_bid_type
  campaign_buying_type: tiktok-campaign_buying_type
  tiktok_campaign_type: tiktok-campaign_type
  adgroup_id: tiktok-adgroup_id
  adgroup_name: tiktok-adgroup_name
  adgroup_secondary_status: tiktok-adgroup_secondary_status
  adgroup_optimize_goal: tiktok-adgroup_optimize_goal
  adgroup_bid_type: tiktok-adgroup_bid_type
  adgroup_pacing: tiktok-adgroup_pacing
  adgroup_placement: tiktok-adgroup_placement
  adgroup_placement_type: tiktok-adgroup_placement_type
  adgroup_landing_page_url: tiktok-adgroup_landing_page_url
  adgroup_interest_category: tiktok-adgroup_interest_category_v2
  promotion_type: tiktok-promotion_type
  ad_id: tiktok-ad_id
  ad_name: tiktok-ad_name
  ad_secondary_status: tiktok-ad_secondary_status
  ad_call_to_action: tiktok-ad_call_to_action
  ad_text: tiktok-ad_text
  ad_video_id: tiktok-ad_video_id
  ad_image_mode: tiktok-ad_image_mode
  ad_format: tiktok-ad_format
  country_region: tiktok-country_region
  stat_time_day: tiktok-stat_time_day
  cost: tiktok-stat_cost
  impressions: tiktok-show_cnt
  clicks: tiktok-click_cnt
  total_plays: tiktok-total_play
  average_video_play: tiktok-average_video_play
  play_2s: tiktok-play_duration_2s
  play_6s: tiktok-play_duration_6s
  play_25pct: tiktok-play_first_quartile
  play_50pct: tiktok-play_midpoint
  play_75pct: tiktok-play_third_quartile
  play_100pct: tiktok-play_over
  engaged_views: tiktok-engaged_view
  engaged_views_15s: tiktok-engaged_view_15s
  likes: tiktok-likes
  paid_follows: tiktok-paid_follows
  paid_shares: tiktok-paid_shares
  paid_comments: tiktok-paid_comments
  profile_visits: tiktok-profile_visits
  landing_page_views: tiktok-total_landing_page_view
  conversions: tiktok-convert_cnt
  complete_payments: tiktok-complete_payment
  campaign_budget: tiktok-campaign_budget
  adgroup_budget: tiktok-budget
```

---

## Resolved Decisions

1. **`campaign_category` literal binding** — DECIDED: extend `ColumnMappingValue` with `Literal(Literal)` variant.
   Dimension declared as regular categorical at kind level. Each dataset binds via `column_mapping: { literal: "value" }`.
   No changes to MetadataDimension. See "Metadata Dimensions" section above for details.

2. **`conversions` semantic alignment** — DECIDED: Unify Google/Bing/TikTok under `conversions`.
   Facebook `purchases` stays separate (not semantically identical to platform conversions).
   - adwords: `conversions: adwords-conversions`
   - bing: `conversions: bing-Conversions`
   - tiktok: `conversions: tiktok-convert_cnt`
   - facebook: no `conversions` mapping (NULL-filled). Keeps `purchases` as platform-specific measure.

3. **`cost` is the unified name** — DECIDED: Drop `spend` from shared measures.
   `cost` maps per-dataset: adwords-cost, bing-cost, facebookads-spend, tiktok-stat_cost.

4. **Bing `country` mapping** — DECIDED: NULL-filled. Standard strategy for missing columns.

5. **`campaign_category` naming** — DECIDED: Use `campaign_category` (not `campaign_type`).
   Bing/TikTok platform-specific fields renamed: `bing_campaign_type`, `tiktok_campaign_type`.

---

## Category 2: Adwords Keyword Clicks (Standalone Dataset → multi-path)

### Decision

Collapse 6 identical UUID-named datasets into a **single standalone dataset** with multi-path storage.
No kind needed — all datasets share identical schema and column_mapping.

**Before:** 6 unionset datasets with UUID names, identical column_mapping
**After:** 1 dataset `adwords_keyword_clicks` with 6 paths + metadata dims

**Paths (all under adwords/ but separate from campaign performance UUIDs):**
```
fs1henp3k8p1hqo/adwords/2bfdf5a9-97f2-4fe0-aeba-b6bcde2f9692/
fs1henp3k8p1hqo/adwords/566e88d4-15c2-480e-9c00-a5251610ef23/
fs1henp3k8p1hqo/adwords/728b558f-ccd5-4615-a345-de5949793f2b/
fs1henp3k8p1hqo/adwords/8f68ac10-7677-4816-93e3-033ddec451e6/
fs1henp3k8p1hqo/adwords/e1c1188a-8be9-4aa1-bc41-91671e3a7c5a/
fs1henp3k8p1hqo/adwords/e5461b63-cc97-4ad3-8bd3-d25174c3a2b3/
```

**Metadata:** `funnel_account_id` from `path.token: 2`

**Key:** `click_id` (gclid) — primary key

**Dimensions:** date, campaign_name, campaign_id, campaign_start_date, campaign_end_date, account_id, keyword, click_id

**Measures:** None (click-level attribution, no aggregates)

---

## Category 3: Cross-Channel Attribution (Standalone Dataset)

### Decision

Keep as **standalone dataset** `funnel_measurement_performance`. Single source, single path, unique schema.

**v2.0 changes:**
- Remove `type: categorical:` blocks (v2.0 default)
- Convert measures from `expr: "SUM(x)"` to `agg: sum`
- Remove `spend` shared ref → use `cost` (unified name from Category 1 decision)

**Dimensions:** date, currency, country, channel, partition_id, brand, conversion_event, report_name, a2_source, campaign_name, traffic_source

**Measures (all agg: sum, fully additive):**
- spend (→ renamed to `cost` for consistency? NO — this dataset uses Funnel's own spend field, not ad platform cost. Keep as `spend`)
- clicks, impressions
- 5× attribution conversions: a2, mta, platform, lastclick, firstclick
- 5× attribution revenue: a2, mta, platform, lastclick, firstclick
- 3× delta: delta_spend, delta_conversions, delta_revenue

---

## Category 4: Web Analytics — GA4 (Two Standalone Datasets)

### Decision

Keep `ga4_event_performance` and `ga4_ecommerce_sessions` as **separate standalone datasets**.

**Rationale against grainset/unionset:**
- Different grain: events (event_name level) vs. sessions (session/transaction level)
- Different dimensions: source/medium/channel_grouping vs. firstUser/session attribution
- Only shared dims: date, currency, country
- No unifiable measures (conversions in events vs. sessions/purchases in ecommerce)
- These are complementary views, not the same data at different grains

**v2.0 changes for both:**
- Remove `type: categorical:` blocks
- Convert measures to `agg: sum`

### ga4_event_performance
**Dimensions:** date, currency, country, campaign_name, source, medium, source_medium, default_channel_grouping, event_name, hostname, is_conversion_event, samples_read_rate
**Measures:** conversions, event_value, purchase_revenue

### ga4_ecommerce_sessions
**Dimensions:** date, currency, country, first_user_campaign_name, first_user_source_medium, session_campaign_name, session_medium, session_source, session_source_medium, transaction_id, samples_read_rate
**Measures:** sessions, new_users, add_to_carts, checkouts, ecommerce_purchases, transactions, purchase_revenue, total_revenue, refund_amount, shipping_amount, tax_amount

---

## Category 5: Email Marketing — Klaviyo (Two Standalone Datasets)

### Decision

Keep `klaviyo_campaign_performance` and `klaviyo_flow_performance` as **separate standalone datasets**.

**Rationale against unionset:**
- Different grain: campaigns (by campaign_id/subject) vs. flows (by flow name)
- Different dimension sets: campaigns have campaign_id, subject, send_time, send_channel, metric_id. Flows have only flow name.
- Different measure sets: campaigns have recipients, clicks_unique, click_to_open_rate. Flows have received_email, bounced_email, bounce_rate, placed_order, placed_order_value, checkout_started, viewed_product, subscribed/unsubscribed.
- Only 3 shared measures: opens, opens_unique, open_rate — insufficient basis for unionset.

**v2.0 changes for both:**
- Remove `type: categorical:` blocks
- Convert measures to `agg: sum`
- Fix column_mapping inconsistency: `email_clicks` maps to `klaviyo-clicks2` (campaigns) vs `klaviyo-clicks` (flows) — keep as-is, this is a real source column difference.

---

## Category 6: Affiliate — Impact (Joinset, keep)

### Decision

Keep `impact_enriched` as a **joinset** with 2 datasets. Structure is correct.

**v2.0 changes:**
- Remove `type: categorical:` blocks
- Convert measures to `agg: sum`
- Give datasets descriptive names (replace UUID names):
  - `8da22144-...` → `impact_conversion_detail` (anchor)
  - `68028aea-...` → `impact_campaign_metadata` (satellite)

---

## Category 7: Ecommerce — Shopify (Joinset, keep)

### Decision

Keep `shopify_enriched` as a **joinset** with 2 datasets. Structure is correct.

**v2.0 changes:**
- Remove `type: categorical:` blocks
- Convert measures to `agg: sum`
- Give datasets descriptive names (replace UUID names):
  - `d067774d-...` → `shopify_order_lines` (anchor)
  - `d6d7da76-...` → `shopify_inventory_snapshot` (satellite)

---

## Category 8: Customer Support — Gorgias (5 Standalone Datasets)

### Decision

Keep all 5 as **separate standalone datasets**. No joinset/unionset candidates.

**Rationale:**
- `gorgias_ticket_detail` — ticket-level detail (has primary key: ticket_id)
- `gorgias_support_summary` — daily aggregate (date-only dimension)
- `gorgias_ticket_volume_by_channel` — daily by channel
- `gorgias_csat_distribution` — daily CSAT scores
- `gorgias_ticket_message_csat_detail` — message-level detail (has primary key: message_id, FK to ticket_detail)

These could form a joinset (ticket_detail + message_csat_detail via ticket_id FK), but the existing model keeps them separate. The summary/volume/csat datasets have fundamentally different grains and dimensions — no unification benefit.

**v2.0 changes for all:**
- Remove `type: categorical:` blocks
- Convert measures to `agg: sum` (where applicable)
- Keep keys (primary, foreign) as-is

---

## Summary — Final Model Structure

| # | Entity | Type | v2.0 Change |
|---|--------|------|-------------|
| 1 | `paid_media_campaign_performance` | kind: unionset (4 datasets) | **NEW** — merges 3 old kinds + 2 standalone into one |
| 2 | `adwords_keyword_clicks` | standalone dataset (multi-path) | Collapsed from 6-dataset unionset kind |
| 3 | `funnel_measurement_performance` | standalone dataset | Minimal v2.0 updates |
| 4a | `ga4_event_performance` | standalone dataset | Minimal v2.0 updates |
| 4b | `ga4_ecommerce_sessions` | standalone dataset | Minimal v2.0 updates |
| 5a | `klaviyo_campaign_performance` | standalone dataset | Minimal v2.0 updates |
| 5b | `klaviyo_flow_performance` | standalone dataset | Minimal v2.0 updates |
| 6 | `impact_enriched` | kind: joinset (2 datasets) | Descriptive dataset names + v2.0 syntax |
| 7 | `shopify_enriched` | kind: joinset (2 datasets) | Descriptive dataset names + v2.0 syntax |
| 8a | `gorgias_ticket_detail` | standalone dataset | v2.0 syntax |
| 8b | `gorgias_support_summary` | standalone dataset | v2.0 syntax |
| 8c | `gorgias_ticket_volume_by_channel` | standalone dataset | v2.0 syntax |
| 8d | `gorgias_csat_distribution` | standalone dataset | v2.0 syntax |
| 8e | `gorgias_ticket_message_csat_detail` | standalone dataset | v2.0 syntax |

**Totals:** 3 kinds (1 unionset, 2 joinsets) + 11 standalone datasets = 14 queryable entities
