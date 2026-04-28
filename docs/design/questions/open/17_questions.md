---
doc: design/questions/open/17_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `foundations/17_temporal_shape.md`
depends-on:
  - foundations/17_temporal_shape.md
  - foundations/11_names_and_scopes.md
  - foundations/13_types_and_grain.md
  - foundations/16_composition.md
  - apis/30_api_contracts.md
  - apis/32_semstrait_model.md
  - apis/34_semstrait_planner.md
  - apis/35_semstrait_ir.md
---

# Open Questions — `foundations/17_temporal_shape.md`

> Items surfaced during Round-1 drafting of the temporal-shape foundations doc. Each entry restates the question, lists its ratified references, and records the Round-1 default `17` currently uses. Entries migrate out of this file as later docs (`20`–`25`, `32`–`35`, `registry/temporal_shape_mapping.md`) make decisions that either confirm or amend `17`'s defaults.

> **Status summary (2026-04-28).** Of 8 questions, **5 are CLOSED** (Q-TEMPORAL-002, -005, -006 — ratified by the `18`-entity consolidation pass: v1 `ScdType` roster trimmed to `{Type1, Type2}`, flat-field `ScdBody` shape, `valid_to IS NULL` as the open-ended-window signal, no `current_flag_dim` in v1, `Type0` descoped; Q-TEMPORAL-007 — Option A no hoisting, ratified by `17 §3.2`; Q-TEMPORAL-008 — Option A per-family enum, ratified by `17 §5.1`). The closed entries are retained in place for full resolution context; readers seeking only live items can skim past their closure banners or jump directly to Q-TEMPORAL-001 / -003 / -004. See the Summary table at the end of this file for the at-a-glance status roster.

---

## Q-TEMPORAL-001 — `30 §6.2` code-range reconciliation for the 17NN block

**Question.** `17 §9` allocates error codes in the doc-aligned 17NN range (`VALID_E_1700`–`1799`, `COMP_E_1700`–`1799`, `PLAN_E_1700`–`1799`, `PLAN_W_1700`–`1799`). `30 §6.2` as currently ratified allocates subsystem-level ranges that top out at `VALID_E_0999` / `COMP_E_0499` / `PLAN_E_0699` / `PLAN_W_0699`. The 17NN block sits outside every current subsystem range. How should the two allocations be reconciled?

**Refs.**
- `17 §9.6` — code-allocation governance statement.
- `17` [CONTRADICTION-FOUND] block at head of doc — detailed options and Round-1 choice.
- `30 §6.2` — current subsystem code ranges.
- `30 §2` — MINOR-vs-MAJOR policy for code-range additions.
- `16 §14` — `16`'s `04xx` / `05xx` allocations; a precedent for the subsystem-aligned style.

**Options.**
- **A. Doc-aligned allocation.** Widen `30 §6.2` to permit per-doc `NNxx` blocks where `NN` is the doc number; each foundations doc reserves its own 100-range per subsystem. `17` uses `1700`–`1799`; `18` (hypothetical future doc) would use `1800`–`1899`; and so on. Implies `30 §6.2`'s subsystem-level caps widen to `9999`. `[17 §9]` and this file adopt Option A as the Round-1 default per the `[CONTRADICTION-FOUND]` block.
- **B. Subsystem-aligned allocation.** Keep `30 §6.2`'s current ranges unchanged; re-home every `*_E_17NN` / `*_W_17NN` reference to the next free slot in the current subsystem range. Under this reconciliation: `VALID_E` would use `0500`–`0599` (next free; allocation intent was keys per §6.2 but mostly unused), `PLAN_E` and `PLAN_W` would use `0600`–`0699`. `COMP_E` has no 100-block free after `16`'s `0400`–`0499` claim; would need to borrow from `VALID_E`'s unused slots or extend `COMP_E`'s overall cap.
- **C. Hybrid.** Adopt Option A at the foundations-doc layer (each doc reserves its own doc-aligned block), but retain `30 §6.2`'s current subsystem-semantic ranges for the API-contract layer (`30`-series docs themselves). Splits the policy into "authoring-convention-per-doc" vs "subsystem-semantic" allocation.

**Arguments for A (adopted).**
- Doc-aligned blocks mechanize the reading pattern most readers already use: "if an error is in the 17NN band, it's a temporal-shape concern." Lookup-by-code becomes a single-digit-prefix match.
- Every ratifying doc gets a fresh 100-block, avoiding sub-allocations that force "next free 10-slot-gap" arithmetic each time.
- Preserves invariant "every doc owns a specific code range"; simpler governance.

**Arguments for B.**
- Keeps `30 §6.2` stable; no cross-doc coordination required.
- Subsystem-semantic grouping lets readers find "all composition errors" in one block rather than scattered across doc-specific allocations.
- The underlying rule — one subsystem per error source — is already well-defined in `30`; Option A fragments it.

**Arguments for C.**
- Resolves the foundations-vs-API-contracts authorial styles without forcing one to migrate.

**Current position in `17`.** Option A adopted. The document uses `*_E_17NN` / `*_W_17NN` codes throughout. A single find-and-replace re-homes every reference under Option B or C without changing error semantics.

**Next step.** Coordinate with `30 §6.2`. If `30` is re-ratified to prefer Option B, `17 §9` regenerates its code allocations. Blocking status: **No** — the [CONTRADICTION-FOUND] block explicitly records the coordination as outstanding; `17`'s structural content is independent of the chosen numbering.

**Blocking?** No.

---

## Q-TEMPORAL-002 — `Scd` payload shape: per-subtype vs flat-fields

**CLOSED (Phase-3 cascade, 2026-04-17).** Ratified via `foundations/18_entities.md §3.3`: **Option B (flat fields)** is the v1 shape, combined with the v1 `ScdType` roster trim to **`{Type1, Type2}`**. `ScdBody` carries `{ scd_type: ScdType, valid_from: SemanticsName, valid_to: SemanticsName }`. There is no longer a per-subtype payload divergence to model, because the four subtypes that had distinct payloads (`Type3` / `Type4` / `Type5` / `Type6`) are descoped from v1. Option A (per-subtype) and Option C (hybrid window-struct) are post-v1 concerns — if the roster re-expands to include history-preserving subtypes with divergent shapes, the question reopens.

**Question.** `17 §2.3` adopts a nested `Scd { subtype: ScdSubtype }` form where `ScdSubtype` carries per-subtype payloads (e.g. `Type2 { valid_from_dim, valid_to_dim, current_flag_dim: Option<_> }`, `Type3 { prior_value_dim }`, `Type4 { history_data_kind_ref }`, etc.). The alternative — a **flat** `Scd { subtype: ScdSubtype, valid_from_dim: Option<SemanticsName>, valid_to_dim: Option<SemanticsName>, prior_value_dim: Option<SemanticsName>, history_data_kind_ref: Option<DataKindRef>, current_flag_dim: Option<SemanticsName>, ... }` where fields are present-or-absent per subtype — was mentioned in the authoring brief. Which is canonical?

**Refs.**
- `17 §2.3` — chosen per-subtype form with rationale.
- `crates/semstrait-model/src/types/temporal.rs` — existing implementation uses a per-subtype form (with `ScdVersionedColumns` as a helper struct shared between `Type2 / Type5 / Type6`).
- `00 §9` I10 — non-exhaustive extensibility rule.

**Options.**
- **A. Per-subtype payload (current).** Each `ScdSubtype` variant carries exactly the fields its semantics need. Type-system refuses nonsense combinations (`Type0` with `valid_from_dim` fails to compile). Matches the existing crate code. Adding a new subtype adds a variant with its own payload.
- **B. Flat fields.** Single `Scd { subtype, ...optional_fields }` struct with every possible SCD field as `Option<_>`. Validation at compile time enforces subtype-specific presence rules (e.g. `Type2 → valid_from_dim.is_some()`). YAML authoring is arguably friendlier (one block with named fields rather than nested by subtype key).
- **C. Hybrid — shared window-struct for the common case.** Keep per-subtype payload for structurally divergent subtypes (`Type3`, `Type4`, `Type5`, `Type6`) but share a `ScdWindow { valid_from_dim, valid_to_dim, current_flag_dim }` struct across `Type2 / Type5 / Type6`. Matches the existing crate's `ScdVersionedColumns`.

**Arguments for A (adopted).**
- Type-safety at the Rust layer: impossible states are unrepresentable. `Type0 { valid_from_dim: ... }` is a compile error, not a validation error.
- Matches the existing crate code — zero-refactor adoption path.
- Future additions (a hypothetical `Type7`) get a fresh variant with its own payload shape without disturbing existing variants.

**Arguments for B.**
- YAML ergonomics may prefer flat fields:
  ```yaml
  scd:
    subtype: type_2
    valid_from: ...
    valid_to: ...
  ```
  versus
  ```yaml
  scd:
    type_2:
      valid_from: ...
      valid_to: ...
  ```
  The YAML surface choice is in `32`'s jurisdiction; the Rust model shape can differ from the YAML shape.
- Tooling that introspects the enum (serde, JSON Schema generation) gets one fewer nested level.

**Arguments for C.**
- The **type-safety** benefit applies primarily to the "did the author write `valid_from_dim` on a Type 0" case; the common-windowed-subtype trio (Type 2, Type 5, Type 6) could share a struct without losing much. Option C reduces duplication in §2.3's enum sketch.

**Current position in `17`.** **CLOSED — Option B (flat fields)** ratified via `18 §3.3`. `ScdBody` carries `{ scd_type: ScdType, valid_from: SemanticsName, valid_to: SemanticsName }`. Options A / C are post-v1 concerns that re-open only if `ScdType` re-expands to include subtypes with divergent payloads. Body retained as historical resolution context.

**Next step.** None. Reactivates only on a future `ScdType` roster expansion that introduces shape-divergent subtypes.

**Blocking?** No.

---

## Q-TEMPORAL-003 — `Joinset` `JoinType::AsOf` override: ratify pre- or post-implicit-`AsOf`?

**Question.** `16 §13.3` ratifies per-traversal `JoinType` overrides inside a `Joinset` declaration. Round-1 overrides are limited to `Inner / Left / Right / Full`. Should the `Joinset` YAML surface admit `AsOf` overrides **before** the planner supports implicit `AsOf` synthesis (i.e. author-forced `AsOf` as the path to adoption), **after** the planner supports implicit synthesis (so the Joinset override is a narrowing choice), or **never** (the Joinset always forbids `AsOf`; authors must rely on implicit synthesis only)?

**Refs.**
- `16 §13.3` — Joinset override surface.
- `17 §5.1` — `JoinType::AsOf` ratified, implementation DEFERRED.
- `17 §5.5` — records the `Joinset` `AsOf` override DEFERRAL.
- `17 §10` D1, D3 — DEFERRED items.

**Options.**
- **A. Author-forced-first.** Admit `AsOf` in the `Joinset` YAML now; planner handles it on Joinset traversals before implicit synthesis lands. Authors can adopt shape-aware queries early at the cost of writing explicit Joinset paths.
- **B. Implicit-first.** Planner implements implicit `AsOf` synthesis first (driven by `Relationship` shape inference per `16 §11`); Joinset-level override adds later as a narrowing mechanism.
- **C. Never.** `Joinset` overrides stay `Inner / Left / Right / Full` permanently; `AsOf` is solely an implicit-synthesis-driven join type. Authors who need explicit control reach for a Request-level mechanism (unspec'd).

**Arguments for A.**
- Authors get a knob now; they don't have to wait for the implicit-synthesis algorithm to land.
- Joinset is already the explicit-authoring escape hatch per `16 §13`; fitting `AsOf` there is the natural next step.

**Arguments for B.**
- Implicit synthesis is the desired "happy path" per `16 §11`'s field-first discovery. If `AsOf` is mostly consumed through implicit paths, the Joinset override is just disambiguation — which doesn't land until disambiguation is needed.
- Easier to reason about: one `AsOf` path, not two.

**Arguments for C.**
- Keeps the Joinset surface minimal. `AsOf` becomes a purely planner-inference concern.

**Current position in `17`.** Joinset `AsOf` override is DEFERRED; Option B is the implicit default. No `AsOf` admitted in `Joinset` until the implicit-synthesis algorithm ships.

**Next step.** `32 §…` (YAML surface for Joinset) gates on this choice. Answer lands with the planner's `AsOf` implementation milestone.

**Blocking?** No.

---

## Q-TEMPORAL-004 — Multi-shape heterogeneous `Request.temporal` resolution

**Question.** `17 §6.5` notes that a `Request` spanning a composed surface with heterogeneous constituent shapes (e.g. one `Scd::Type2` + one `Snapshot` + one `Events`) needs a well-defined resolution of `Request.temporal.as_of` across all shapes. What is the ratified algorithm?

**Refs.**
- `17 §6.5` — illustrative cases and DEFERRED statement.
- `17 §8.4` — shape-aware composition pass (DEFERRED) that runs anchoring + rollup per constituent.
- `16 §11` — implicit composition algorithm; doesn't ratify temporal-composition semantics.
- `34 §…` (pending) — planner strategies.

**Options.**
- **A. Per-constituent as-of independence.** Each shape-classified constituent interprets `Request.temporal.as_of` through its own lens (§6.2 rules); no cross-constituent consistency enforcement. The composed result is the natural join / union of per-constituent anchored views.
- **B. Dominant-shape-driven.** One shape in the composition is nominated "dominant" (typically the `from:` clause's root kind) and drives the as-of; other constituents follow. Introduces an ordering that `16 §11`'s field-first discovery does not currently produce.
- **C. Per-Request explicit anchor declaration.** Author declares `Request.temporal.per_data_kind: { "orders": as_of_A, "customers_scd": as_of_B }` when multi-shape. Round-1 default is the uniform as-of of Option A; Option C is the explicit override.

**Arguments for A.**
- Simplest ratification. Each shape already has well-defined per-shape as-of semantics from §6.2.
- No new vocabulary. `Request.temporal` stays a single `as_of`.
- Degenerate case when all constituents agree on shape = identity (which is what Round 1 happens).

**Arguments for B.**
- Authors often *intend* a single anchor moment ("as of end-of-quarter 2024-12-31"); Option A's per-constituent independence could surface as surprising when e.g. a Snapshot aligns to a cadence boundary but the SCD doesn't.
- Matches how dbt MetricFlow and similar tools treat a "metric_time" — one anchor for the whole query.

**Arguments for C.**
- Ratifies the escape hatch upfront; authors can opt in to per-constituent control when they need it.
- Complements Option A (use Option A as default, Option C as explicit override).

**Current position in `17`.** DEFERRED; Option A looks likely but the `34` algorithm ratification is where this settles.

**Next step.** `34 §…` ratifies. Possibly as A + C (default + explicit override).

**Blocking?** No.

---

## Q-TEMPORAL-005 — Default-current row semantics for SCD without `current_flag_dim`

**CLOSED (Phase-3 cascade, 2026-04-17).** Ratified via `foundations/18_entities.md §3.3`: v1 `ScdBody` does not carry a `current_flag_dim` field at all — the v1 roster `{Type1, Type2}` uses `valid_from` / `valid_to` exclusively. Default-current selection on `Type2` is the `valid_to IS NULL` convention (the open-ended-window signal); max-`valid_from` per entity is retained as a secondary fallback heuristic with `PLAN_W_1731` advisory. `current_flag_dim` re-enters the spec only if a future roster extension (e.g., `Type6`) reintroduces it. The sentinel-aware Option B and refuse-without-signal Option C stay deferred.

**Question.** `17 §6.3` specifies that default-current selection on an `Scd::Type2 / Type5 / Type6` kind looks for `current_flag_dim = TRUE` when the flag Dim is declared, else falls back to the `valid_to_dim IS NULL` (open-ended) convention. When neither signal is available — the author declared a `Type2` SCD with no `current_flag_dim` and uses a sentinel value for `valid_to` — the planner emits `PLAN_W_1731 ScdCurrentRowHeuristic` and picks the row with the maximum `valid_from` per entity. Is this heuristic ratified semantics, or a placeholder?

**Refs.**
- `17 §6.3` — the heuristic.
- `17 §10` D13 — DEFERRED.
- `registry/temporal_shape_mapping.md` (pending) — per-engine sentinel conventions.

**Options.**
- **A. Heuristic is ratified.** Max-`valid_from` per entity is the canonical "current row" when no flag is declared. Authors opting out of `current_flag_dim` accept that the query emits a subquery / window function per-entity.
- **B. Explicit sentinel-aware ratification.** The author declares `valid_to_sentinel: "9999-12-31"` (or `NULL`) on the SCD shape; default-current matches rows where `valid_to = sentinel`. `17 §2.3` does not currently carry a `valid_to_sentinel` field on `Type2` / etc.; this would be a MINOR extension.
- **C. Refuse without current-signal.** Treat "author declared an SCD history-preserving subtype without `current_flag_dim` or sentinel" as a compile-time error (`COMP_E_17xx`) — "declare one of the two, we don't guess."

**Arguments for A.**
- No new authoring surface. Works on the existing shape.
- The heuristic is defensible: in a well-formed Type 2 table, the latest `valid_from` per entity is the currently-active row.

**Arguments for B.**
- Explicit is better than implicit. The sentinel convention varies by engine / shop (`NULL` vs `'9999-12-31'` vs `'2999-12-31'`); the model should know which to match.
- Avoids a subquery for the common "sentinel row" case (planner can emit `WHERE valid_to = sentinel` directly).

**Arguments for C.**
- "No current signal at all" is likely an authoring mistake; refusing forces the author to declare intent.

**Current position in `17`.** Option A as Round-1 default with `PLAN_W_1731` advisory. Option B likely lands in a future MINOR once adapter sentinel conventions are ratified.

**Next step.** `registry/temporal_shape_mapping.md` or `32 §…` decides.

**Blocking?** No.

---

## Q-TEMPORAL-006 — Append-only enforcement for `Scd::Type0`

**CLOSED (Phase-3 cascade, 2026-04-17).** Ratified via `foundations/18_entities.md §3.3`: **`Scd::Type0` is not in the v1 roster**. The v1 `ScdType` roster is trimmed to `{Type1, Type2}`, `#[non_exhaustive]`. The append-only-enforcement question is moot for v1; it re-enters the spec only if a future roster extension re-includes `Type0`. The "out of scope for the semantic layer" disposition (Option A) remains the design guidance for any future reintroduction.

**Question.** `Scd::Type0` — "retain original; no updates after insert" — is a runtime-behavior promise: once a row is written, it is never re-written. Should semstrait validate this at query time (emit an advisory if an `UPDATE`-like plan is synthesized over a `Type0` kind), at ingest time (out of scope for the semantic layer), or neither?

**Refs.**
- `17 §2.2` — `Type0` definition.
- `17 §10` — DEFERRED roster; `Type0` runtime enforcement not listed.

**Options.**
- **A. Neither. Runtime-invariant out of scope.** semstrait is a query-plan compiler; ingest-side correctness is the data pipeline's concern. `Type0` carries its meaning at the vocabulary level (downstream readers know the kind is immutable) but semstrait does not enforce it.
- **B. Advisory at query time.** When semstrait synthesizes a plan that would logically require updating a `Type0` kind (e.g. a compiled `AsOf` join that materializes into a CTE with per-entity "latest" semantics — even though this doesn't update the source), emit `PLAN_W_17xx` if the shape suggests confusion.
- **C. Advisory at compile time on the DataKind declaration.** When a DataKind declares `Type0` but also carries (via its binding's catalog metadata) columns that look like update-tracking columns (`updated_at`, `last_modified`), emit `COMP_W_17xx` advisory ("you declared `Type0` but the table has mutation columns — was this intentional?").

**Arguments for A (adopted).**
- Narrow scope. semstrait reads data; it doesn't write. Runtime mutation invariants belong in the ingest / catalog layer.
- No extra vocabulary. Authors who need `Type0` enforcement use their catalog's mechanisms (Iceberg branch protection, catalog-level table-type constraints).

**Arguments for B.**
- Harder to justify — semstrait doesn't emit writes in the first place.

**Arguments for C.**
- Low-cost authoring safety net. Catches obvious mistakes.

**Current position in `17`.** Option A. `Type0` carries vocabulary meaning only; no runtime / query-time enforcement in Round 1.

**Next step.** Revisit if authoring-time advisories (Option C) prove valuable once `32` catalog-metadata access is ratified.

**Blocking?** No.

---

## Q-TEMPORAL-007 — Hoisting `TemporalShape` to `ComplexDataKind`? — CLOSED (2026-04-28)

**Status: CLOSED.** Option A (no hoisting) ratified. `ComplexDataKind` shape propagates via `16 §8` composition rules; no `temporal_shape:` block on the complex variants in v1. Shape hoisting is MINOR per I10 and can land later. Round-1 framing retained for historical reference.

**Question.** `17 §3.2` ratifies that `ComplexDataKind` (`Unionset`, `Grainset`, `Joinset`) does not carry its own `temporal_shape:` block; shape propagates via §8's composition rules. But a `Joinset` with a single root child (say the root is a `Timeseries { grain: Day }`) could reasonably inherit its root's shape as a first-class property — the `Joinset`'s observation-cadence is the root's cadence. Should `17` ratify shape hoisting for these cases?

**Refs.**
- `17 §3.2` — `ComplexDataKind` shape stance.
- `16 §5.3` — `CompositionKind` hierarchy.
- `22 §…` (pending) — Grainset root; `24 §…` (pending) — Joinset root.

**Options.**
- **A. No hoisting (current).** Every `ComplexDataKind` presents its shape composition to the planner per §8; the planner works through constituents. Simpler, fewer special cases.
- **B. Hoist for Joinset when root is shape-classified.** A `Joinset`'s root-kind shape is the "default" shape of the composed surface. Convenience; common case.
- **C. Hoist universally.** `ComposedSemanticInterface` carries an `effective_temporal_shape: Option<TemporalShape>` field populated during compile. Consumers need not walk constituents.

**Arguments for A (adopted).**
- Simplest structural invariant.
- `ComposedSemanticInterface`'s per-constituent shape access is already needed for shape-aware joins (§5, §8); hoisting doesn't remove that requirement.

**Arguments for B.**
- Authors reading `Joinset`-level documentation see a single `TemporalShape` that represents the composed surface; matches mental model.

**Arguments for C.**
- Consumers (downstream docs like `34`) get a single field to read; fewer walks.

**Current position in `17`.** Option A. Hoisting is MINOR per I10 and can land as an optimization later.

**Next step.** `24 §…` (Joinset root-shape) may re-propose Option B.

**Blocking?** No.

---

## Q-TEMPORAL-008 — `AsOfAnchor` shape: per-family enum vs tagged struct — CLOSED (2026-04-28)

**Status: CLOSED.** Option A (per-family enum) ratified — `AsOfAnchor::ScdWindow { .. }` / `AsOfAnchor::SnapshotLatestAtOrBefore { .. }`. Matches `TemporalShape`'s own per-variant payload style; exhaustiveness-checking on `match` lets `35` / `36` downstream emitters catch unhandled anchor families at compile time. Round-1 framing retained for historical reference.

**Question.** `17 §5.1` specifies `AsOfAnchor` as a `#[non_exhaustive]` enum with two variants (`ScdWindow { ... }`, `SnapshotLatestAtOrBefore { ... }`). An alternative is a tagged struct with optional fields: `AsOfAnchor { probe_dim: SemanticsName, kind: AsOfAnchorKind, scd_valid_from: Option<SemanticsName>, scd_valid_to: Option<SemanticsName>, snapshot_at: Option<SemanticsName> }`. Which is canonical?

**Refs.**
- `17 §5.1` — current enum form.
- Q-TEMPORAL-002 — a parallel Rust-model-shape choice for SCD payloads.

**Options.**
- **A. Per-family enum (current).** `AsOfAnchor::ScdWindow { .. }` / `AsOfAnchor::SnapshotLatestAtOrBefore { .. }` — matches `TemporalShape`'s own per-variant-payload style. Future anchor families (e.g. `BiTemporalWindow { .. }`) add as new variants.
- **B. Tagged struct.** Flat struct with optional fields; semantics gated by the `kind` discriminator. Convenient for serialization, harder for pattern-matching.

**Arguments for A (adopted).**
- Consistent with `TemporalShape` itself (per-variant payload).
- Exhaustiveness-checking on `match` lets `35` / `36` downstream emitters catch unhandled anchor families at compile time.

**Arguments for B.**
- Serialization / JSON-Schema simpler (one type, not a sum).

**Current position in `17`.** Option A ratified.

**Next step.** Revisit only if downstream serialization ergonomics surface complaints.

**Blocking?** No.

---

## Summary — Round-1 position

| Q | Title | Round-1 position | Blocking? |
|---|---|---|---|
| Q-TEMPORAL-001 | 17NN code-range reconciliation with `30 §6.2` | Option A (doc-aligned). `[CONTRADICTION-FOUND]` records coordination. | No |
| Q-TEMPORAL-002 | SCD per-subtype vs flat-field payload | **CLOSED** — `18 §3.3` ratifies Option B (flat fields) with v1 roster `{Type1, Type2}`. | No |
| Q-TEMPORAL-003 | Joinset `AsOf` override pre- or post-implicit | Implicit-first (Option B). DEFERRED. | No |
| Q-TEMPORAL-004 | Multi-shape heterogeneous `Request.temporal` | DEFERRED; Option A likely. | No |
| Q-TEMPORAL-005 | Default-current without `current_flag_dim` | **CLOSED** — `18 §3.3` drops `current_flag_dim` from v1; `valid_to IS NULL` is the current-row signal. | No |
| Q-TEMPORAL-006 | `Type0` append-only enforcement | **CLOSED** — `18 §3.3` descopes `Type0` from v1 roster. | No |
| Q-TEMPORAL-007 | `ComplexDataKind` shape hoisting | **CLOSED (2026-04-28)** — Option A no hoisting (MINOR later if `24`-side need surfaces). | No |
| Q-TEMPORAL-008 | `AsOfAnchor` per-family vs tagged | **CLOSED (2026-04-28)** — Option A per-family enum (`17 §5.1`). | No |

None of the open questions blocks ratification of `17`. All are governance / coordination / deferred-implementation items whose Round-1 defaults are stable enough for downstream docs to build on.
