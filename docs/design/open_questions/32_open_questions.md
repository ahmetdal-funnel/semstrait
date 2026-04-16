# 32 — Open Questions

Unresolved items arising while drafting `docs/design/apis/32_semstrait_model.md`. Each entry restates the question, lists the relevant ratified references, summarizes the arguments on each side, and proposes a lean next step so a later decision pass can resolve without re-reading the whole doc.

---

## Q-MODEL-001 — Multi-file / directory loader helper

**Question.** Should `semstrait-model` expose a helper like `parse_dir(path: &Path) -> Result<SemanticModel, ParseErrors>` that reads a directory of YAML files, merges them into a single `SemanticModel`, and handles any future `$include:` directive? Or should file loading stay strictly outside the crate (owned by the CLI / LSP)?

**Refs.**
- `32 §10.1` — `parse(&str) -> Result<SemanticModel, ParseErrors>` takes string input only.
- `32 §10.3` — multi-file loading declared out of scope for v1; deferred as `[TD-MODEL-DIR-LOADER]`.
- `32 §14.1` — "no I/O" boundary; `std::fs` is forbidden in the crate source.
- `00 §9` I11 — no downward I/O surprises.
- `00 §9` I6 — plan hot path is synchronous.

**Arguments pro in-crate helper.**
- Every CLI / LSP / tooling frontend ends up re-implementing the same directory walk + merge logic. Centralising reduces drift.
- The merge semantics (same-name collision handling, `namespace:` scoping across files) benefits from a single authoritative implementation tested in one place.
- A single-helper contract lets adapters / plugins consume the aggregate tree without caring how it was assembled.

**Arguments against.**
- Violates I11's "no I/O in semstrait-core / semstrait-model" posture. Adding `std::fs` to `semstrait-model` is a precedent for future erosion.
- Directory walking is trivially implemented in ~30 lines by each consumer; the saving is negligible.
- Merge policy is a product decision (does a `$include` override siblings? are `relationships:` unioned? do `semantics:` definitions deduplicate by name?) — parking it in `32` forces a decision we don't need to make in v1.
- LSP's re-parse-on-change logic wants a diff-aware merge, not a strict re-read; exposing a helper that consumers will then bypass is negative value.

**Current position in `32`.** Declared out of scope for v1. `parse` takes `&str`. Tracked as `[TD-MODEL-DIR-LOADER]` in `32 §10.3`.

**Next step.** Defer until the CLI specification (`semstrait-cli`, future `38`) and LSP specification (if any) have concrete use cases. When the helper ships, it lives in a separate crate (`semstrait-loader` or `semstrait-cli-util`), not in `semstrait-model`, to preserve the I/O boundary.

---

## Q-MODEL-002 — Primary error-code shape

**Question.** `32 §11` documents `ParseError` variants with a numeric-code grouping (`PARSE_E_01xx`, `PARSE_E_02xx`, ...) in comments but `code()` returns kebab-case per `31 §8.3`. Which is the primary, authoritative code shape — kebab-case (`"parse.duplicate-data-kind"`) or numeric (`"PARSE_E_0201"`)?

**Refs.**
- `30 §6` — subsystem prefix allocation (`PARSE_E`, `VALID_E`, `COMP_E`, `PLAN_E`, `PLAN_W`, `ADAPT_E`).
- `31 §8.3` — `code()` returns kebab-case; numeric form preserved as a `const LEGACY_CODE: &'static str` associated value per variant.
- `31 §15` R-?? — "migration stance only, pending a ratification pass" — `31`'s position is kebab-case primary, numeric secondary.
- `14 §7` — the historical numeric form (`EXPR_E_####`) ratified there.
- `00 §9` I12 — first-class diagnostics with stable documented codes.

**Arguments for kebab-case as primary.**
- Self-describing: `parse.duplicate-data-kind` needs no lookup table.
- Matches `10 §5.1`'s `Diagnostic.code` shape and `31 §7.4`'s `code()` derivation.
- Adding variants requires no registry update — the code is derived from the variant name.
- Consistent across all subsystems (`parse.*`, `validate.*`, `compile.*`, `plan.*`, `adapt.*`) — the single string carries the stage.

**Arguments for numeric as primary.**
- Stable under variant renames (the numeric code survives a variant-name refactor; kebab form changes).
- Log-scraping pipelines prefer short identifiers.
- `30 §6`'s allocation table is already numeric; authoring the kebab form as primary makes the allocation table a secondary concern.

**Current position in `32`.** Kebab-case per `31 §8.3` is primary. Numeric is documentation-only. Open questions: (a) should the numeric form be exposed via `ParseError::legacy_code(&self) -> Option<&'static str>` per `31`'s scheme? (b) should `30 §6`'s allocation table be rewritten to use kebab-case ranges?

**Next step.** Land a `30` amendment making kebab-case the primary across all subsystems (matching `10 §5.1` / `31 §7.4` / `32 §11`). Keep `30 §6`'s numeric allocation table as documentation of the historical allocation but mark it "secondary." If the numeric form is demonstrated to be valuable for log-tooling, add `legacy_code()` accessors uniformly across subsystems; otherwise drop the comments.

---

## Q-MODEL-003 — `kind:` discriminator spelling: `simple` vs `dataset`

**Question.** `32 §5.1` accepts both `kind: simple` and `kind: dataset` as synonyms for the `DataKind::Simple` variant. Should v1 pick one canonical spelling? If so, which?

**Refs.**
- `20 §3` — uses the term "Simple" for the variant.
- `21` — document title is "Dataset"; `21 §2` refers to "`SimpleDataKind`".
- `32 §5.1` — current draft accepts both.
- Legacy code — uses `datasets:` (plural block; implicit kind).
- Peer products — dbt MetricFlow uses `semantic_models:` (neither `simple` nor `dataset`); Cube.js uses `cubes:` (neither).

**Arguments for `simple`.**
- Matches the ratified struct name (`SimpleDataKind`) directly.
- Parallel to `unionset` / `grainset` / `joinset` — every `kind:` value is the variant name in lower-case.
- Less ambiguous: "dataset" in analytics vocabulary means many things (a row set, a logical table, a physical file); "simple" is a term-of-art we own.

**Arguments for `dataset`.**
- Authors' mental model: "I'm declaring a dataset." dbt / Cube / Looker users expect the word "dataset" to appear.
- `21`'s document title is "Dataset"; the YAML surface matching the doc title is ergonomically consistent.
- Legacy code uses `datasets:` — migration pain is lower if we keep the spelling.

**Current position in `32`.** Both spellings accepted in v1 (`32 §5.1`). Deprecation of one is deferred to v2 MINOR.

**Next step.** Park. The cost of accepting both in v1 is trivial (one `match` arm in the parser). Round 2 has budget to run an author survey or migration-friction analysis; pick one then, deprecate the other via a `parse.deprecated-kind-spelling` warning for 1 minor cycle, drop in v2. If a decision is required sooner (e.g. for doc / style-guide consistency), prefer `simple` — it aligns with the struct name and the other variant names.

---

## Q-MODEL-004 — `DataKindRef::Inline` hoisting cadence

**Question.** `32 §3.3` exposes `DataKindRef::Inline(Box<DataKind>)` but states the v1 parser always rewrites `Inline` to `ByName` before returning the `SemanticModel` (hoisting inline children to top-level `data_kinds:` under structural labels per `11 §10`). Should this behaviour remain eager, or should Round 2 support deferred hoisting for LSP / incremental re-parse?

**Refs.**
- `11 §10` — structural labels for hoisted inline children.
- `12 §3.1` — `datasets:` grouping under complex parents is a parse-site convenience; children are references after parse.
- `32 §3.3` — `Inline` kept on the enum for `[TD-INLINE-HOIST-LAZY]` future.
- `32 §13.2` — internal `YamlRoot` scaffolding treated as implementation detail.

**Arguments for eager hoisting (current).**
- `SemanticModel` is always "fully normalized" — every `DataKindRef` resolves to a top-level entry. Downstream code never has to branch on `Inline` vs `ByName`.
- Diagnostic clarity: "`orders_rollups` is declared inside `paid_media` (auto-hoisted as `paid_media__orders_rollups`)" is a single well-defined story.
- Matches I4 determinism: hoisting rule is mechanical and stable.

**Arguments for deferred hoisting (future).**
- LSP re-parse on a single nested child doesn't need to re-hoist the whole tree; a deferred form would be O(changed-subtree) rather than O(full-tree).
- Round-trip serialization: serializing a `SemanticModel` with hoisted inlines doesn't round-trip back to the original YAML — the nesting is lost. A deferred mode could preserve.
- Author-facing tools (diff viewers, refactor preview) may want to show the un-hoisted shape for author ergonomics.

**Current position in `32`.** Always hoist at parse. `DataKindRef::Inline(_)` is kept on the enum as a stability-preserving placeholder for the future feature.

**Next step.** Park as `[TD-INLINE-HOIST-LAZY]`. If / when LSP performance becomes a concrete constraint, introduce an `parse_options: ParseOptions { hoist: HoistPolicy }` parameter on `parse` as an additive MINOR — the `HoistPolicy::Eager` default preserves v1 behaviour, `HoistPolicy::Deferred` returns `Inline` variants that the caller (LSP) normalizes on demand.

---

## Q-MODEL-005 — Expression-surface parse-site table completeness

**Question.** `32` refers to `ExprSource::parse_semantic` / `parse_physical` at several sites (Dimension `expr:`, Measure `expr:`, Metric `expr:`, Filter `expr:`, Joinset interface `expr:`, ColumnMapping `Computed { expr }`). Is there a canonical enumeration of every site that parses an `ExprSource`, so that every site can be covered by an integration test?

**Refs.**
- `14 §4.2` — `ExprSource` dispatch lives in `semstrait-model` parse sites.
- `14 §2.2` / `§2.3` — `SemanticExpr` vs `PhysicalExpr` admissibility rules.
- `32 §6.2` / `§6.3` / `§6.4` / `§6.5` — Dimension / Measure / Metric / Filter `expr:` sites (all `SemanticExpr`).
- `32 §8.3` — `ColumnMapping::Computed { expr }` (`PhysicalExpr`).
- `32 §5.4` — Joinset interface Dimension `expr:` (`SemanticExpr`).

**Arguments for tabulation.**
- Makes the wrapper-typing contract auditable: "every site that parses an `ExprSource` MUST dispatch to exactly one of `parse_semantic` or `parse_physical`".
- Integration-test target: enumerate every site, feed it an adversarial `expr:` (e.g. a `Column` reference on a `SemanticExpr` site), assert the expected `ValidateError`.
- Future-proofs against new parse sites silently defaulting to one wrapper without explicit decision.

**Arguments against.**
- The table is maintenance overhead; when a new site is added, the table must be updated.
- The rule is almost-mechanical ("Measures / Metrics / Dimensions / Filters → SemanticExpr; Bindings / ColumnMapping → PhysicalExpr"); author judgement is reliable.

**Current position in `32`.** Implicit across §6 / §8 / §9. Not tabulated explicitly in the doc.

**Next step.** Add a short appendix to a future revision of `32` enumerating the exhaustive parse-site table. Until then, an integration test in `crates/semstrait-model/tests/` maintains the check programmatically (the test enumerates every `ExprSource` field via reflection-style match). Record as `[TD-EXPR-PARSE-SITE-AUDIT]`.

---

## Q-MODEL-006 — `AggregationConstraints.allowed` / `.prohibited` ordering

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

**Next step.** Park. The question resolves itself if (a) `31 §6.3` matching becomes order-sensitive (then sorting is wrong), or (b) the content-hashing use case materializes (then sort — at the hashing boundary, not at parse). Neither is a v1 concern.

---

## Q-MODEL-007 — YAML crate choice (`serde_yaml` vs `yaml-rust2` / `saphyr`)

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

**Current position in `32`.** `serde_yaml` remains the v1 choice. Tracked as `[TD-MODEL-YAML-CRATE]` in §15 — pre-v1 migration not blocking.

**Next step.** Monitor. If a concrete parse-error-quality blocker emerges (e.g. "line / column tracking on `YamlSyntax` errors is too poor"), re-open and spike a `yaml-rust2` / `saphyr` adapter. Otherwise the migration lands post-v1 as a transparent internal swap — the public surface (`ParseError`, `parse`) is stable.

---

## Q-MODEL-008 — `functions:` YAML block scope

**Question.** `32 §4.5` allows authors to declare adapter-contributed function extensions at the model scope via a `functions:` top-level block. `31 §5.8` already provides `RegistryExtension` impls at the adapter-crate level. Are both needed? If yes, how do per-model declarations interact with global `RegistryExtension` impls?

**Refs.**
- `14a §7.1` — adapter-contributed registry entries via `RegistryExtension`.
- `31 §5.8` — `RegistryExtension` trait surface; folded at `function_registry()` init.
- `32 §4.5` — `functions:` YAML block with `FunctionExtension` entries.
- `14a §7.2` — collision handling: `AdapterFunctionShadowsCore`, `AdapterFunctionCollision`.

**Arguments for keeping both.**
- Global `RegistryExtension` impls are linked-in at workspace-build time. Authors who use a pre-built `semstrait` distribution can't add a function without rebuilding. Per-model `functions:` gives them an escape hatch.
- Per-model declarations are scoped to the model; they don't pollute every other model in the workspace.
- Parallel to dbt's `macros` at package-level: some code lives globally, some lives per-project.

**Arguments for dropping per-model.**
- Two registries = two collision rules = doubled complexity at `compile`.
- Function identity should be process-global per `14a §2.2`; per-model overrides create ambiguity ("does this name mean the clickhouse `quantile_bfloat16` or the per-model one?").
- The v1 use case is speculative — no author has asked for per-model overrides.
- Per-model `functions:` are a security surface ("load arbitrary function definitions from user YAML"); global impls are link-time and auditable.

**Current position in `32`.** Both supported in v1. Collision handling at `compile` per `31 §5.8`.

**Next step.** Defer a concrete decision until a real use-case surfaces. If none by mid-v1, deprecate and remove the `functions:` YAML block in a v2 MINOR; only `RegistryExtension` impls remain. Tracked as `[TD-MODEL-FUNCTIONS-BLOCK]`.

---

*Cross-references in this document are by section (e.g. `32 §10.3`, `31 §8.3`). No code-path references are used, per `00 §8`.*
