# Open Questions — Joinset Shape Semantics

**Status:** parked for post-v1. Captured from the `32` ratification session (2026-04-17) and from `20 §3` / `24 §*` while drafting the data-kinds tree.

This file holds two clusters of deferred decisions about joinset authoring shape. Neither is blocking for v1 — current v1 behavior is explicitly enumerated per cluster.

---

## Cluster A — `JoinAssociativity`

### A.1 Background

Earlier drafts of the Joinset shape carried an `associativity:` field on the body — values such as `left`, `right`, `star`, `snowflake` — as a hint to the planner about how to order joins when the declared `relationships:` graph had multiple valid topological walks.

**v1 decision (ratified 2026-04-17):** dropped. `JoinAssociativity` does NOT exist in the v1 Joinset body. `32 §14` records this as `TD-JOINSET-ASSOCIATIVITY-PARK`.

### A.2 v1 behavior

The Joinset walks its `relationships:` in YAML author order (`32 §7` / `32c §2`). When there is a choice, the planner picks the canonical left-deep traversal starting from the first-referenced member in the first `Relationship` entry. No cross-cutting hint shapes the walk.

Effect: authors who care about join shape sort their `relationships:` array deliberately. For the explicit-binary-join case that is v1 Joinset's only ratified arity, shape choices are degenerate anyway — there is exactly one join to emit.

### A.3 Deferred questions

| ID | Question |
|---|---|
| Q-JS-ASS-001 | When v1's binary-only restriction lifts (`TD-NESTING-NARY-JOIN`), does `associativity:` re-enter as a field, or does an ordering convention on `relationships:` suffice? |
| Q-JS-ASS-002 | If re-introduced, what values are legal? Candidates: `left` (left-deep), `right` (right-deep), `bushy` (planner chooses), `hinted` (planner uses author order). |
| Q-JS-ASS-003 | Is associativity advisory (planner may override for cost) or binding (planner must respect)? |
| Q-JS-ASS-004 | Interaction with `AsOf` joins (`17 §*`, currently DEFERRED): does AsOf carry its own shape-hint axis, or is it orthogonal? |

### A.4 Related authoring shapes in the wild

Reference implementations from the industry:

- **Cube.js** — no shape hint; join graph discovered at query time from declared relationships.
- **dbt_metricflow** — join-path resolution hints via `join_path:` on Metric definitions; implicit associativity.
- **LookML** — explicit `join` block per pair; implicit left-deep walk from the base `view:`.

v1 aligns with the Cube.js / LookML pattern (implicit from author-order). Re-introducing an explicit hint is a future decision when N-ary Joinsets land.

---

## Cluster B — Star / Snowflake / 3NF Shape Tags

### B.1 Background

During Q&A on the joinset YAML, a candidate field was considered that would let authors mark a Joinset as a specific canonical analytical shape:

```yaml
joinsets:
  - name: fact_sales_star
    shape: star              # or: snowflake, 3nf, data_vault, galaxy
    relationships: [ ... ]
```

The intent was to let the planner apply shape-specific optimizations (fact-table detection, conformed dimension lookup, etc.) without having to derive the shape from the `relationships:` graph.

**v1 decision:** deferred. Not in the v1 body.

### B.2 v1 behavior

Shape is derived at plan time from the `relationships:` graph + per-member Cardinality, when a shape-optimizing pass needs it. The planner does not require a declared shape and emits the same `PlanNode::Join` tree regardless.

### B.3 Deferred questions

| ID | Question |
|---|---|
| Q-JS-SHP-001 | Is the `shape:` tag advisory (for diagnostics / tooling) or operationally meaningful (drives a planner pass)? |
| Q-JS-SHP-002 | What shape vocabulary is canonical? Candidates: `star`, `snowflake`, `galaxy`, `3nf`, `data_vault`, `one_big_table`, `junk_dimension_hub`. Open-ended strings vs a closed enum. |
| Q-JS-SHP-003 | If authored, is it validated against the declared relationships (e.g. `star` requires exactly one `many_to_one`-spine from one member to all others)? |
| Q-JS-SHP-004 | Does shape interact with `constraints:` at the joinset level (e.g. a star schema implicitly allows certain dimension rollups)? |
| Q-JS-SHP-005 | Does shape interact with the applicability matrix (`25`) — e.g. specific shapes enable specific aggregation patterns that flat graphs do not? |

### B.4 Shape vocabulary — reference implementations

Candidate vocabulary that recurred in discussion:

- **Star schema** — one fact, many conforming dimensions, single-depth joins from fact.
- **Snowflake** — like star, with dimensions normalized (multi-level dimension hierarchies).
- **Galaxy / fact constellation** — multiple fact tables sharing conforming dimensions.
- **3NF / normalized** — arbitrary graph over normalized entities.
- **Data Vault** — hubs / satellites / links canonical shape.
- **One Big Table (OBT)** — denormalized, single wide member with no joins.

For v1, the Joinset represents explicit joins over ≥ 2 members with a declared graph. Shape classification can be layered on post-v1 without breaking the v1 body.

---

## Status

- Both clusters remain parked as non-blocking for v1.
- A shape-dedicated session, once joinset N-ary support lands (`TD-NESTING-NARY-JOIN`), will re-open them.
- In the meantime, referenced as `joinset_shape_semantics.md` from `32 §12` (the Pointers-to-Child-Docs table) and from `24 §*` (when that doc is re-ratified post-cascade).
