---
doc: design/questions/open/17_questions
status: Living
purpose: Open questions surfaced while drafting `foundations/17_temporal_shape.md`
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

> One framework-level question remains open: Q-TEMPORAL-001 (17NN code-range reconciliation with `30 §6.2`). Closed items moved to [`../closed/17_questions.md`](../closed/17_questions.md); deferred items in [`../deferred/17_questions.md`](../deferred/17_questions.md). Q-TEMPORAL-001 does not block ratification of `17`; it is a governance / coordination item.

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
