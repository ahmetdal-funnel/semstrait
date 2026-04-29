---
doc: design/questions/deferred/32_questions
status: Deferred
purpose: Items deferred to v2 (or later) ratification, originally raised against `apis/32_semstrait_model.md`
---

# Deferred Questions — `apis/32_semstrait_model.md`

> Items deferred to v2 (or later) ratification. Live items are in [`../open/32_questions.md`](../open/32_questions.md); closed items in [`../closed/32_questions.md`](../closed/32_questions.md).

---

## Q-MODEL-006 — `AggregationConstraints.allowed` / `.prohibited` ordering — DEFERRED

**Status: DEFERRED.** Parked: blocks on the constraints session (see [`../deferred/11_questions.md`](11_questions.md)). The question resolves itself if (a) `31 §6.3` matching becomes order-sensitive (then sorting is wrong), or (b) the content-hashing use case materializes (then sort — at the hashing boundary, not at parse). Neither is a v1 concern.

**Question.** `31 §6.3` exposes `AggregationConstraints { allowed: Vec<String>, prohibited: Vec<String> }`. `32 §12.2` commits to preserving YAML author order in both vectors for I4. `31`'s matching algorithm is token-based and order-insensitive. Is preserving author order correct, or should `32` sort the vectors for a stronger canonical form?

**Refs.**

- `31 §6.3` — token-based matching; order does not affect semantics.
- `32 §12.2` — YAML author order preserved for I4 determinism.
- `30 §2` — stability rules for public fields.
- `11 §8.4.1` — constraint DSL YAML shape.

**Arguments for preserving author order.**

- Authors may have ordered the list intentionally (e.g. preferred-first policy for a future ordering-sensitive feature).
- Matches the `serde_yaml` round-trip — sorting at parse changes the on-disk form when re-serialized.
- I4 is satisfied either way (both author order and sort order are deterministic); author order matches the input byte sequence.

**Arguments for sorting.**

- Canonicalisation: `{ allowed: [SUM, MIN] }` and `{ allowed: [MIN, SUM] }` become byte-identical, which enables content-hashed caching at a lower level than the full `SemanticModel`.
- Diagnostic messages citing the constraint can be position-stable.
- Removes a trap: if `31 §6.3`'s matching ever becomes order-sensitive (e.g. "first match wins" for conflict resolution), the doc says "order doesn't matter" but code behaviour would depend on it.

**Current position in `32`.** YAML author order preserved per §12.2. The vectors are not sorted at parse.

---

## Q-MODEL-007 — YAML crate choice (`serde_yaml` vs `yaml-rust2` / `saphyr`) — DEFERRED

**Status: DEFERRED.** Tracked as `[TD-MODEL-YAML-CRATE]` in `32 §15`; pre-v1 migration not blocking. Migration lands post-v1 as a transparent internal swap — the public surface (`ParseError`, `parse`) is stable.

**Question.** Current code uses `serde_yaml` (upstream archived March 2024). `32`'s dependency posture (`§13.4`) assumes `serde_yaml`. Should the crate migrate to a maintained alternative (`yaml-rust2`, `saphyr`) before v1?

**Refs.**

- `32 §13.4` — dependency posture table lists `serde_yaml`.
- `crates/semstrait-model/src/parse.rs` — current parser uses `serde_yaml` throughout.
- Upstream: `serde_yaml` archived by maintainer; `yaml-rust2` is active; `saphyr` is the emerging alternative.

**Arguments for migration (pre-v1).**

- Unmaintained dependencies are a supply-chain risk.
- Error-quality improvements in `yaml-rust2` / `saphyr` (span tracking, incremental parse) would strengthen `ParseError.location: Option<Location>` (§11.1).
- Migrating post-v1 is a breaking change if any `ParseError` variant message embeds `serde_yaml`-specific strings.

**Arguments against migration (pre-v1).**

- `serde_yaml`'s API is `serde`-idiomatic; alternatives require bespoke deserialization plumbing.
- The crate is functional and stable. "Archived" ≠ "broken."
- Migration cost: non-trivial; the current parse code uses `serde_yaml` idioms throughout (`#[derive(Deserialize)]`, `serde_yaml::Value`, etc.).
- v1 shipping is the priority; migrations can land as `[TD-MODEL-YAML-CRATE]` in v1.x.

**Current position in `32`.** `serde_yaml` remains the v1 choice. Tracked as `[TD-MODEL-YAML-CRATE]` in §15.

**Next step.** Monitor. If a concrete parse-error-quality blocker emerges (e.g. "line / column tracking on `YamlSyntax` errors is too poor"), re-open and spike a `yaml-rust2` / `saphyr` adapter.
