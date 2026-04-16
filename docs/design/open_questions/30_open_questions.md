---
doc: design/open_questions/30_open_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `apis/30_api_contracts.md`
depends-on:
  - apis/30_api_contracts.md
  - foundations/10_resolution_pipeline.md
  - foundations/14_expressions.md
  - foundations/14a_function_catalog.md
---

# Open Questions — `apis/30_api_contracts.md`

> Items surfaced during Round-1 drafting of the cross-cutting API-contracts doc. Entries migrate out of this file as they are answered by per-crate `31`–`39` docs, by implementation contact with the Rust surface, or by an amendment pass against an upstream foundation doc.

---

## Q-API-001 — Reconcile `10 §5.1` Diagnostic sketch with `30 §5–§6`

**Context.** `00 §4.1`'s `Diagnostic` row lists `apis/30_api_contracts.md` as the authoritative doc. `30 §5` ratifies the canonical struct: `{ code: &'static str, severity: Severity, message: String, location: Option<Span>, context: Vec<ContextLine> }` with `Severity ∈ {Info, Warning, Error}` and `code` drawn from the `{SUBSYSTEM}_{SEVERITY}_{NUMBER}` convention of `30 §6`. `10 §5.1` — drafted before `30` landed — sketched a different shape: `code: String` (kebab-case `<stage>.<variant>`), `Severity ∈ {Error, Warning, Note}`, and a `source_chain: Vec<Diagnostic>` field in place of `context`.

**Question.** Amend `10 §5.1` to import `30`'s ratified shape verbatim, or keep `10`'s sketch as an alternative and document both? The `authoritative-for` row in `00 §4.1` settles the precedence in `30`'s favor; the only work item is the amendment.

**Status.** `30` is now the authoritative surface. `10 §5.1` needs a follow-up amendment to align: (a) rename `Note` → `Info`, (b) replace kebab-case `code` convention with the numeric ranges of `30 §6`, (c) replace `source_chain` with `context: Vec<ContextLine>`, (d) note that the `IntoDiagnostic` conversion still applies (typed enum → `Diagnostic` at API boundary). Track as `[TD-DIAG-ALIGN-10]`.

---

## Q-API-002 — Warning propagation across fail-fast stages

**Context.** `30 §7` matches `10 §5`'s per-stage policy (`parse`/`validate` accumulate; `compile`/`plan`/`optimize`/`adapt` fail-fast). `14 §7.4` and `30 §5` both admit `Warning` and `Info` severities (e.g. `EXPR_W_0001` `CastNarrowing`, `EXPR_W_0002` `AdditivityMismatch`). A successful fail-fast stage clearly carries warnings back to the caller alongside its primary output; an unsuccessful one is less clear.

**Question.** When `compile` fails fast on the first `Error`, does the returned failure carry any warnings accumulated up to the failure point, or are they discarded? Similarly for `plan` / `adapt`.

**Status.** Proposal: every stage returns `Result<(Output, Vec<Diagnostic>), (CompilerErrorEnum, Vec<Diagnostic>)>` where the `Vec<Diagnostic>` carries accumulated `Warning` + `Info` entries in both arms, and the error arm's enum carries the fatal error. This preserves every author-visible signal regardless of ultimate outcome. Deferred to the per-crate entry-point shapes in `33` / `34` / `36`; `30` states the principle ("warnings are never silently dropped at API boundaries") and defers signature detail.

---

## Q-API-003 — `OPT_E_*` vs `OPT_W_*`: does optimizer ever error?

**Context.** `10 §3.5` says initial-design `optimize` is a near-identity pass; most canonical rules are warnings (`OPT_W_*`). However, `10 §3.5`'s error policy is `fail-fast`, and `OptimizeError::{PassFailed, InvalidRewrite}` are named. Per `30 §6`, we reserved `OPT_W_0100-0199` for warnings and `OPT_E_0100-0199` for failures.

**Question.** Is it ever appropriate for a canonical optimizer rule to emit a hard error, given the stage's "rule-by-rule, warnings accumulate, errors fail-fast" policy from `10 §3.5`? A broken adapter-registered pass can justify `OPT_E_*` (`PassFailed`), but canonical semstrait-authored passes in v1 have no error path.

**Status.** Keep `OPT_E_*` reserved for adapter-pass failures and `OPT_W_*` for canonical rewrites; no canonical rule in v1 uses `OPT_E_*`. Revisit when any canonical pass grows a failure mode beyond "pass is trivially identity".

---

## Q-API-004 — `Span` authoritative shape: `core` vs `model`

**Context.** `30 §5` defers `Span` / `ContextLine` structural details to `31` (`semstrait-core`) and `32` (`semstrait-model`), mirroring `10 §5.1`'s treatment of `SourceId` / `ByteSpan`. `31` is expected to own primitives shared across crates; `32` owns YAML-specific location details.

**Question.** Does `semstrait-core` host the `Span` struct itself (source-id + byte range), with `32` contributing a `Model`-specific `SourceId` variant, or does `Span` live in `32` and get re-exported by `core`? The latter couples `core` to `model`, which I7 forbids.

**Status.** Default: `Span`, `SourceId`, and `ContextLine` live in `semstrait-core`; `SourceId` is non-exhaustive and `32` adds `Model`-specific variants without widening the core surface. Confirm when drafting `31`.

---

## Q-API-005 — Error-code retirement mechanics

**Context.** `30 §6.5` says "a stable code NEVER changes meaning; a code may be retired (with a migration note) but not repurposed." Retirement is documented in `implementation/42_migration_notes.md`; the retired code's `&'static str` literal is deleted from the codebase in the same MAJOR.

**Question.** Is retirement MAJOR-only, or may a MINOR release mark a code `#[deprecated]` (via a stub `Diagnostic` code registry module) before removing at the next MAJOR? And: does a retired code get a "tombstone" entry in `30 §6.7` so log-scraping tools recognize old codes?

**Status.** Propose MINOR may introduce a `#[deprecated]` tombstone for at least one cycle before a MAJOR removes the literal; tombstones are tracked in `implementation/41_deprecations.md`. `30 §6.7` carries a "Retired codes" sub-section once any retirements exist. No retirements in v1 — every reserved code is forward-looking.

---

## Q-API-006 — `#[non_exhaustive]` struct matrix for `Resolved*` types

**Context.** `30 §4` lists the public sum types that MUST be non-exhaustive and acknowledges MAY-grow public structs should be `#[non_exhaustive]`. The `Manifest`-layer `Resolved*` family (`ResolvedDataKind`, `ResolvedSource`, `ResolvedColumnMapping`, `ResolvedExprTable`, …) are MAY-grow by construction (planner indices evolve).

**Question.** Is the blanket rule "every `Manifest`-layer public struct is `#[non_exhaustive]`" correct, or should internal-only indices (e.g. `ResolvedExprTable`) remain `pub(crate)` and therefore exhaustive? The `33` doc will settle placement; `30` states the policy and defers.

**Status.** Policy: the `Manifest` root struct, `ResolvedDataKind`, `ResolvedSource`, `ResolvedColumnMapping`, and any other MAY-grow public leaf are `#[non_exhaustive]`. Planner-internal indices (the lookup tables, the `Relationship` adjacency) live behind `pub(crate)` accessors and are therefore free to be exhaustive. Confirm per-type in `33`.

---

## Q-API-007 — Adapter / catalog crates: feature-flag-gated or separate crates?

**Context.** `30 §10` ratifies "per-engine adapter crates as SEPARATE crates, not feature flags on a single crate" and the same for catalog providers. This is the opposite of the `sqlx`-style model where drivers are features.

**Question.** Is this the final posture, given the build-time / cargo-dependency-closure cost of many tiny crates vs. the compile-time cost of optional features? Concrete downstream impact: a consumer who needs only the DuckDB adapter pulls `semstrait-facade` + `semstrait-adapter-duckdb` and gets no Spark code at all (current design) vs. pulls `semstrait-facade` with `features = ["duckdb"]` and the crate's `Cargo.toml` conditional-compiles out Spark (alternative).

**Status.** Separate-crate posture stands for v1 per `30 §10`. Rationale: matches the `RegistryExtension` layering in `14a §7` (one crate per adapter-contributed registry), keeps dependency closures surgical for catalog providers with heavy SDK transitive deps (Iceberg, Unity), and avoids the `cfg(feature = ...)` scatter across the adapter trait implementations. Revisit if the crate graph becomes unwieldy in practice.

---

## Q-API-008 — Async posture of `semstrait-manifest`: compile-time only, or sealed-then-sync?

**Context.** `30 §9`'s per-crate async table lists `semstrait-manifest` as "compile-time async; plan-time sync". The compile-time async surface is the orchestration entry point (`compile(model, catalog, fs) -> Manifest`). At plan time, the `Manifest` is consumed by reference and all access is sync.

**Question.** Does `semstrait-manifest` expose any `async fn` at plan time, e.g. for on-demand re-resolution of a specific `PhysicalSource` that the planner discovered needs freshening? Per I11 this is forbidden — only the two out-of-band I/O entry points (`Repository::load`, `CatalogProvider::check_schema_drift`) are permitted around plan time, and both are invoked **before** `plan` begins.

**Status.** Policy: `semstrait-manifest` exposes async **only** on its compile-time entry points; every other function is sync. The two out-of-band entry points live on `Repository` (in `semstrait-manifest`) and `CatalogProvider` (in `semstrait-catalog`) respectively; neither is called from inside `plan`. Confirm in `33`.

---

## Q-API-009 — `RegistryExtension` as sealed trait

**Context.** `30 §8` ratifies that public traits are either sealed-pattern (external impls prevented) or fully public. Per `14a §7.1`, `RegistryExtension` is the trait adapter crates implement to contribute functions to the sealed registry. Its impls are discovered at static-initialization time.

**Question.** Should `RegistryExtension` be a sealed trait (only workspace crates may implement) or an open trait (any third-party crate may implement and contribute to `function_registry()`)? An open trait invites adapter experiments outside the workspace; a sealed trait keeps the `&'static FunctionRegistry` initialization path under workspace control.

**Status.** Propose open trait. The collision-rejection policy of `14a §7.2` (hard-reject at init) protects registry integrity without requiring the trait itself to be sealed. Third-party adapters contributing `RegistryExtension` impls is a supported extensibility mode; `14a §7.1` already treats the trait as public. Confirm in `36`.

---

## Q-API-010 — Stability tier naming: "Stable in v1" vs "Provisional" vs "Experimental"

**Context.** `30 §13`'s stability table uses "Stable in v1" and "Provisional". No "Experimental" tier appears in v1 — every crate in the `3x` map is at least Provisional.

**Question.** Is a third tier ("Experimental" for crates that might not ship at all) needed, or does `Provisional` cover the whole "may change in MINOR under documented migration" range? Anything truly pre-design lives outside the `3x` map (not in the document catalog yet).

**Status.** Two tiers for v1 is sufficient; revisit if a future crate is added mid-cycle without enough design maturity for `Provisional`. Any such crate would be gated out of `semstrait-facade`'s public surface until promoted.
