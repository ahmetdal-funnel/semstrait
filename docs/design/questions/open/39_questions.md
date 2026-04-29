---
doc: design/questions/open/39_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `apis/39_semstrait_facade.md`
depends-on:
  - apis/39_semstrait_facade.md
  - apis/30_api_contracts.md
  - apis/31_semstrait_core.md
  - apis/33_semstrait_manifest.md
  - apis/35_semstrait_ir.md
  - apis/36_semstrait_adapter.md
  - apis/37_semstrait_catalog.md
  - apis/38_semstrait_api.md
---

# Open Questions — `apis/39_semstrait_facade.md`

> Seven questions remain open: Q-FAC-001, -002, -004, -005, -006, -007, -008. Closed items moved to [`../closed/39_questions.md`](../closed/39_questions.md). Each entry restates the question, lists its ratified references, and records the Round-1 default `39` currently uses. None of these items block the headline ratifications in `39 §11`.

---

## Q-FAC-001 — Default-feature composition: `["ansi-sql"]` vs `[]`

**Question.** `39 §5.1` sets `default = ["ansi-sql"]`, which keeps `AnsiSqlAdapter` reachable from `semstrait::prelude::`* in a fresh `cargo add semstrait` build. Should the facade ship with a default-adapter bundle at all, or should `default = []` force consumers to opt into at least one adapter explicitly?

**Refs.**

- `30 §10.1` — default features = minimum viable; no core type gated behind a feature.
- `30 §10.5` — bans `default = ["every-adapter"]`, but is silent on the single-adapter-default case.
- `36 §5.1` — `AnsiSqlAdapter` is the only adapter shipped inside `semstrait-adapter` itself; no optional workspace crate needed.
- `39 §5.1` — current default composition.

**Arguments for `default = ["ansi-sql"]` (current Round-1 default).**

- A `cargo add semstrait` followed by `use semstrait::prelude::*;` compiles and runs against a working adapter without a second `--features` flag. Matches the "add a crate, don't add ceremony" posture ratified in `39 §1.5`.
- `AnsiSqlAdapter` is dialect-neutral SQL and pulls no engine crate — turning it on by default costs nothing in transitive dependency weight.
- Teaches the feature-gate pattern gently: a user who later adds `features = ["datafusion"]` sees adapters accumulate rather than replace.

**Arguments for `default = []`.**

- Forces callers to make a conscious adapter choice, preventing the "I got SQL text but I wanted Substrait" footgun.
- Matches `30 §10.1` more literally — the "minimum viable" reading of "no adapter at all" is arguably more minimal than "one baseline adapter."
- One less hidden decision when auditing a build's feature set.

**Current position in `39`.** `default = ["ansi-sql"]`. `AnsiSqlAdapter` is the sole default-enabled adapter.

**Next step.** Confirm during `38` ratification. If `38`'s `SemStrait::adapt` signature lets the builder default-register an adapter automatically, the `ansi-sql` feature may become redundant — otherwise keep the current default.

---

## Q-FAC-002 — Prelude membership of `ir::Name`

**Question.** `39 §3.2` promotes `Name` (from `semstrait-ir §5.4`) into `prelude::`*. `Name` is a plan-level identifier newtype that most first-pass callers never construct by hand — they receive it embedded in `PlanNode` variants. Should it remain in the prelude for symmetry with other re-exports, or drop to `semstrait::ir::Name`-only?

**Refs.**

- `39 §3.3` — membership principles: first-call relevance, unambiguous short name, v1-stable.
- `35 §5.4` — `Name` shape and usage (internal to `SemanticPlan`).
- `39 §3.4` — prelude omissions roster (`*Id` newtypes excluded).

**Arguments for keeping `Name` in the prelude (current Round-1 default).**

- Matches the decision to include `Name` on a per-`PlanNode` basis — if `PlanNode` is in the prelude, its carrier types should be too.
- Short, unambiguous token — no std collision.
- Callers who render a `SemanticPlan` to a debug string end up constructing `Name` values in test harnesses; making the path shorter helps.

**Arguments for dropping to `semstrait::ir::Name`.**

- `39 §3.4` already excludes `*Id` newtypes on the same grounds (rarely constructed by hand). `Name` fits that category.
- The prelude is better at being "the 20 names you actually use" than "every carrier type from every sub-crate."
- Reduces prelude-namespace pressure as the workspace grows.

**Current position in `39`.** Included in `prelude::`* per `§3.2`.

**Next step.** Revisit when `38` fleshes out `SemStrait`'s output-rendering surface. If `Name` is materialized only through `SemanticPlan` / `PlanNode` in common usage, drop to `semstrait::ir::Name`-only.

---

## Q-FAC-003 — `semstrait::run` error type — CLOSED

> **Moved to [`../closed/39_questions.md`](../closed/39_questions.md#q-fac-003--semstraitrun-error-type--closed--superseded-by-typed-kind-transition).** Superseded by typed-kind transition.

---

## Q-FAC-004 — `semstrait::run` catalog wiring: hard-coded `NoopCatalogProvider` vs caller-supplied

**Question.** `39 §4.1`'s signature omits a catalog parameter; the implementation hard-codes `NoopCatalogProvider` (`37 §4.1`). This rules out catalog-bound one-shots — a caller whose YAML references an Iceberg table cannot use `run`. Should `run` accept an optional `&dyn CatalogProvider`, or should catalog-bound flows always drop to `SemStraitBuilder`?

**Refs.**

- `37 §4.1` — `NoopCatalogProvider`: compile succeeds only for manifest-internal references.
- `33 §9` — `compile` signature takes a catalog registry.
- `39 §4.3` — explicit "not a loader" stance.
- `38` (pending) — `SemStraitBuilder` surface presumed to carry catalog injection.

**Arguments for hard-coded `NoopCatalogProvider` (current Round-1 default).**

- Keeps `run`'s surface minimal and one-line-callable — three params, no builder.
- Clear "scripts, tests, demos" framing in `§4.1`'s doc comment: if you need catalog resolution, you need the full builder.
- Avoids the async catalog trait (`37 §3.2`) leaking into `run`'s signature in a way that confuses the one-shot ergonomics.

**Arguments for `run(yaml, request, adapter, catalog: &dyn CatalogProvider)`.**

- One extra parameter preserves the one-shot shape while lifting the catalog-less restriction.
- Catalog-bound models are the majority case in production; excluding them from `run` is a pedagogical trap.
- `NoopCatalogProvider` is still trivially constructable as a fallback.

**Arguments for `run_with(yaml, request, adapter, builder_fn)` variadic overload.**

- Keeps the three-param `run` for demos and adds a builder-callback variant for catalog-bound flows.
- Introduces a second free function, which `39 §4.4` explicitly stops at one.

**Current position in `39`.** Hard-coded `NoopCatalogProvider`. Catalog-bound flows use `SemStraitBuilder` from `38`.

**Next step.** Resolve alongside `38`'s `SemStraitBuilder` ratification. If the builder turns out to be a one-liner (`SemStrait::builder().with_catalog_provider(c).with_file_system(fs).build()`), keeping `run` catalog-less is defensible; if the builder is verbose, lift the restriction.

---

## Q-FAC-005 — Exact-version pinning of sub-crates: `=1.0.0` vs `~1.0` vs `^1.0`

**Question.** `39 §6.1` pins every `semstrait-`* sub-crate at `=1.0.0` (exact version). This upholds `30 §2.1`'s coordinated-release posture and preserves type-identity across the facade boundary. Is exact-pinning the right policy, or is `~1.0` (patch-compatible) / `^1.0` (minor-compatible) better?

**Refs.**

- `30 §2.1` — coordinated workspace release; every sub-crate ships in lockstep.
- `30 §11.3` — cross-crate breaks propagate coordinated-release-style.
- `39 §6.1` — exact-version pin rationale (type-identity argument).
- `cargo-semver-checks` — `30 §11.2`'s green-light requirement.

**Arguments for `=1.0.0` exact pin (current Round-1 default).**

- Type-identity: if the facade re-exports `semstrait-manifest::SemanticManifest` at `semstrait::manifest::SemanticManifest` and a caller also depends on `semstrait-manifest` directly at `1.0.1`, cargo's MINOR-compatibility semantics silently allow two different type instances to coexist — the facade's re-export becomes non-identical to the direct import. Exact-pin forbids this.
- Coordinated release (`30 §2.1`): the facade has no lifecycle independent of the sub-crates; pinning exactly matches.
- Makes `cargo update` on `semstrait` a single coordinated bump; no hidden drift across the workspace.

**Arguments for `~1.0` (patch-compatible).**

- Lets patch-level fixes in a sub-crate reach consumers without a facade release.
- Matches cargo-idiomatic dependency specs.
- Still preserves type-identity if the caller also uses `~1.0` (which is the default `cargo add` behavior).

**Arguments for `^1.0` (minor-compatible, cargo default).**

- Normal cargo behavior; least surprising to most authors.
- Workspace-wide `cargo update` rolls forward to the newest compatible set.
- Type-identity is preserved by cargo's unification as long as every consumer stays on the same `^1.0`.

**Current position in `39`.** `=1.0.0`. Type-identity argument drives the decision.

**Next step.** Verify with a `cargo-semver-checks` audit before v1.0 cut. If the type-identity argument turns out to be overblown (e.g. every realistic workspace ends up on the same patch anyway), relax to `~1.0`.

---

## Q-FAC-006 — Reserved feature names: enumerated vs open-namespace

**Question.** `39 §5.6` reserves feature names for future per-engine (`clickhouse`, `trino`, `snowflake`, `bigquery`, `postgres`) and per-catalog (`polaris`, `glue`, `hms`, `tabular`) integrations. Is pre-emptive reservation the right approach, or should the namespace stay open (first-come-first-served)?

**Refs.**

- `30 §10.2` — adapter support as separate crates.
- `30 §10.3` — catalog support as separate crates.
- `39 §5.6` — current reserved list.

**Arguments for reservation (current Round-1 default).**

- Prevents collision if an early community adapter crate claims a name we later need (e.g. `clickhouse` used as a feature alias for a hypothetical `semstrait-adapter-clickhouse`).
- Documents the intended extension surface — signals "these engines are on the roadmap" without committing to ship.
- Matches `30 §6.6`'s reserved-prefix discipline for error codes, applied to feature names.

**Arguments for open namespace.**

- Reservation implies commitment; if `snowflake` is reserved for three years and never shipped, callers may be blocked from a workaround crate that would have used it.
- Cargo features are scoped per-crate — a third-party `semstrait-adapter-clickhouse` crate can name its own features whatever it likes; facade-level reservation only affects the facade.
- Less maintenance: no reserved list to update as the integration roster grows.

**Current position in `39`.** Reserved list per `§5.6`, with explicit caveat that reservation does not imply a ship commitment.

**Next step.** Resolve during first non-core adapter integration — if a community crate needs a reserved name earlier than the workspace ships it, unblock by releasing the reservation; otherwise keep.

---

## Q-FAC-007 — Prelude growth budget: scannable cap vs organic growth

**Question.** `39 §3.2`'s prelude is currently ~25 names. As sub-crates grow new first-touch types, prelude membership may balloon — at some point it stops being "the 20 names you actually use" and starts being "half the workspace surface." Should `prelude::`* cap its size, or grow organically?

**Refs.**

- `39 §3.3` — membership principles (first-call relevance, unambiguous short name, v1-stable).
- `39 §3.4` — what the prelude deliberately omits.
- `39 §7.4` — prelude growth follows deprecation lifecycle.

**Arguments for no hard cap (current Round-1 default).**

- Membership principles in `§3.3` already act as a natural throttle — items enter only if they satisfy all three criteria.
- Arbitrary caps (e.g. 25 names) force later additions to eject earlier ones, which is worse API churn than growing organically.
- Callers who want a smaller surface import individual names via `use semstrait::prelude::{parse, compile, plan};`.

**Arguments for a soft cap of ~25 names.**

- Scannable by humans at a glance — a 50-name prelude is a haystack.
- Forces discipline when adding new members: "what would I evict to make room?"
- Mirrors Rust stdlib's own `std::prelude::v1` philosophy — small, stable, rarely grown.

**Arguments for a two-tier prelude (`prelude::basic` + `prelude::full`).**

- Lets power users pull everything while newcomers see a minimal set.
- Downside: three modules to remember (`prelude`, `prelude::basic`, `prelude::full`) — arguably worse than none.

**Current position in `39`.** No hard cap; membership principles apply.

**Next step.** Revisit at v1.2 or v1.3 — if the prelude has grown past ~35 names without feeling bloated, leave open; if it feels unwieldy, impose a soft cap via membership-principle tightening.

---

## Q-FAC-008 — `semstrait::VERSION` constant: useful vs redundant

**Question.** `39 §2.1` / `§2.2` ship a `pub const VERSION: &str = env!("CARGO_PKG_VERSION");`. Is this meaningfully distinct from a consumer writing `env!("CARGO_PKG_VERSION")` at their call site, or is it redundant surface?

**Refs.**

- `33 §3.2` — `SemanticManifestMetadata.semstrait_version` records the compile-time workspace version.
- `39 §6` — version alignment posture (coordinated release).
- `39 §2.1` — `VERSION` constant row.

**Arguments for shipping `VERSION` (current Round-1 default).**

- Diagnostic / bug-report friendliness: `println!("{}", semstrait::VERSION)` is a one-liner; `env!("CARGO_PKG_VERSION")` at the *consumer's* call site reports the consumer's version, not semstrait's.
- Matches how many ecosystem crates expose a `VERSION` constant (serde, tokio, etc.).
- Anchors the coordinated-release story (`§6`) — one constant, one source of truth for the workspace version.

**Arguments against.**

- Adds a `pub const` to the facade; `39 §8` explicitly aims for zero new surface.
- `cargo tree` and `cargo metadata` already expose the version to tooling that needs it.
- Consumers who need the version at runtime can depend on `semstrait-core::VERSION` if we expose one there — keeps the facade pure re-export.

**Current position in `39`.** `pub const VERSION: &str = env!("CARGO_PKG_VERSION");` on the facade.

**Next step.** Consider moving to `semstrait-core` and re-exporting from the facade if `§8`'s "zero new surface" goal is tightened; otherwise keep.

---

*Each entry lists `Current position` — the default `39` adopts in Round-1 drafting — and `Next step` — the trigger that would reopen the decision. Items migrate out of this file as the trigger conditions resolve.*