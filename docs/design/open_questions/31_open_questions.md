# 31 — Open Questions

Unresolved items arising while drafting `docs/design/apis/31_semstrait_core.md`. Each entry restates the question, lists the relevant ratified references, and proposes a lean next step so a later decision pass can resolve without re-reading the whole doc.

---

## Q1 — Canonical `Aggregation` variant count

**Question.** Should the canonical `Aggregation` enum have **5 variants** (`Sum | Avg | Count | Min | Max`, with `CountDistinct` encoded via `Expr::Aggregate.distinct: bool`) as ratified in `14 §3.2`, or **6 variants** (adding `CountDistinct` as a first-class variant) as framed in the `31` drafting prompt?

**Refs.**
- `14 §3.2` — explicit: *"`Aggregation ::= Sum | Avg | Count | Min | Max`"* and *"`COUNT DISTINCT` ... is expressed as `Aggregate { aggregation: Count, distinct: true }`"*.
- `14 §3.3` — *"**`Aggregation` enum is closed** ... `CountDistinct` does **not** appear as its own variant"*.
- `14 §5.4` — aggregate typing table uses 5 variants; the `distinct: true` case is called out on the `Count` row.
- `31` drafting prompt (this session) — listed 6 variants including `CountDistinct`.

**Current position in `31`.** Ratified as 5 variants per `14 §3.2` (authoritative for this type). Decision recorded as R12 in `31 §15`.

**Next step.** If `14 §3.2` is re-opened, amend there first; `31` will follow mechanically. Until then, no further action.

---

## Q2 — `ContextLine` placement

**Question.** The `31` drafting prompt listed `ContextLine` as a `Diagnostic`-family type. `10 §5.1` does not ratify one. Should a `ContextLine { line_number: usize, text: String, caret_range: Option<Range<usize>> }` type be exposed on `semstrait-core`?

**Refs.**
- `10 §5.1` — ratifies `Diagnostic { code, severity, location, message, source_chain }`, no context-line field.
- `30` (parallel draft) — stability / diagnostic policy doc; may ratify rich rendering in a later amendment.

**Arguments pro.**
- Author-facing tooling (`semstrait-cli`, LSP) needs source-line + caret rendering. Having it pre-packaged on the `Diagnostic` avoids every frontend re-implementing it.
- `Diagnostic.location: Option<Location>` already carries a `ByteSpan`; the line+caret derivation is a pure function of `SourceId` + `ByteSpan`.

**Arguments against.**
- Rich rendering is a **presentation concern** above `Diagnostic`. `Diagnostic` is the typed-to-structured-carrier boundary; adding rendering data blurs that boundary.
- The concrete presentation may want ANSI coloring, multi-line context, or truncation — all of which are policy choices better made at the presentation layer.
- `30`'s stability policy prefers minimal API surface on shared types.

**Current position in `31`.** NOT exposed; parked here. Decision recorded as R7 in `31 §15`.

**Next step.** Defer until `30`'s diagnostic policy ratifies rendering surface explicitly. If rendering lands in a new `diagnostic::render` module, that module can consume `Diagnostic.location` and emit `ContextLine` independently.

---

## Q3 — Legacy numeric error codes (`EXPR_E_####`)

**Question.** `14 §7.1`–`§7.3` and `14a §8` use numeric error codes (`EXPR_E_0001`, `EXPR_E_0301`, etc.) on the typed error variants. `10 §5.1` ratifies a different code shape: kebab-case derivation `"<stage>.<variant>"` (e.g. `"validate.column-in-semantic-expr"`). Two code systems exist in the design tree.

**Refs.**
- `10 §5.1` — *"Stable, kebab-case identifier derived from the originating stage and enum variant by convention"*. *"No centralized code registry."*
- `14 §7` — numeric `EXPR_E_####` codes per variant.
- `14a §8` — numeric codes reused across `14 §7.3` variants.
- `00 §9` I12 — *"stable documented code (e.g. `COMP_I001`, `PLAN_E042`)"* — a third shape.

**Arguments for kebab-case (`10 §5.1`).**
- Self-describing; no lookup table needed to know what `validate.column-in-semantic-expr` means.
- Ratified ratification path is `10` (pipeline-authoritative).
- Adding a variant requires no code-registry update.

**Arguments for numeric (`14`).**
- Stable under variant renames (the numeric code survives a variant-name refactor; the kebab form changes).
- Log-scraping pipelines tend to prefer short identifiers.

**Current position in `31`.** Primary code is kebab-case per `10 §5.1`. Numeric codes preserved as a **legacy secondary code** via a `const LEGACY_CODE: Option<&'static str>` associated value on variants that appear in `14 §7` tables. Decision recorded as R?? — migration stance only, pending a ratification pass.

**Next step.** Raise against `30` (parallel draft) or against `14` directly: pick **one** code shape as primary and one as secondary; amend the mismatch in either `14 §7` or `10 §5.1`. `31` will follow mechanically.

---

## Q4 — `ExprBlock` enumeration in `semstrait-core`

**Question.** `14 §4.4.1` enumerates 21 reserved AST tags and their YAML shapes. Should `semstrait-core` expose an `ExprBlock` enum with 21 variants (plus a catch-all `FunctionCall`), mirroring that table 1:1? Or keep the enumeration inside `semstrait-model`'s parser and expose only the structural boundary (`ExprSource::Inline` / `ExprSource::Block(ExprBlock)`) where `ExprBlock` is an opaque carrier?

**Refs.**
- `14 §4` — ratifies `ExprSource` as the YAML-surface type.
- `14 §4.4.1` — 21 reserved tags with their YAML shapes.
- `31 §3.4` — current draft: `ExprBlock` is exposed, variant list is "tracked in `14 §4.4.1`", exhaustive expansion is a doctest.

**Arguments pro exposing 21 variants.**
- Makes the YAML-to-AST mapping typesafe — downstream consumers (docs, schema generators) can pattern-match exhaustively.
- Keeps `semstrait-core` as the single source of truth for the shape of an expression at any stage.

**Arguments against.**
- Doubles the maintenance surface — every new `Expr` variant that lands as a reserved tag triggers BOTH an `Expr` variant and an `ExprBlock` variant.
- The YAML parse is a one-time compile-time operation; after that the `Expr` tree is what travels. Exposing the YAML-surface enum beyond the parser is speculative.
- `semstrait-model` could keep `ExprBlock` as a crate-private type and expose only `ExprSource::parse_semantic()` / `::parse_physical()` as conversions.

**Current position in `31`.** Exposed as `ExprBlock` with a placeholder variant list. The full enumeration is deferred to a future amendment that reconciles against `14 §4.4.1`'s latest table.

**Next step.** Decide the boundary at `32` (semstrait-model) drafting time — if `32` keeps `ExprBlock` internal, delete the `ExprBlock` exposure from `31 §3.4` / `31 §14.1`.

---

## Q5 — Visitor trait return-type surface

**Question.** `ExprVisitor` as drafted (`§3.6`) has a single associated type `Output` and a single method `visit(&mut self, &Expr) -> Output`. This matches the shape in `14 §3.4`. Is this sufficient for the visitor use cases the design anticipates (type inference, cycle detection, column-reference collection, pushdown analysis)?

**Refs.**
- `14 §3.4` — traversal contract.
- `14b` (not yet on disk but referenced) — eager resolution's substitution algorithm.

**Concern.** Many analysis passes need pre-order / post-order separation (e.g. type inference is bottom-up), or separate enter / exit callbacks. The single-method shape forces each visitor to carry its own traversal state.

**Proposed alternatives.**
- Expand to `fn enter(&mut self, &Expr) -> Self::Output; fn exit(&mut self, &Expr, &[Self::Output]) -> Self::Output;` shape.
- Provide two blanket traits `ExprPreVisitor` / `ExprPostVisitor` over the single `visit` method.

**Current position in `31`.** Single-method per `14 §3.4`. No change pending concrete use cases in `14b` (not yet ratified) or in `semstrait-planner` visitors.

**Next step.** Revisit after `14b` lands. If eager resolution's substitution algorithm needs a post-order hook, it will motivate a shape revision.

---

## Q6 — `SourceId` opacity vs `Display` / `as_str`

**Question.** `SourceId` is ratified as opaque (`31 §7.3`), with `SourceId::unknown()` and `SourceId::as_str()` as the only public surface. Is `as_str()` the right name? Should a `Display` impl be sufficient?

**Refs.**
- `10 §5.1` — *"`SourceId` and `ByteSpan` exact shapes are ratified in `32`"* — parked on `32`.

**Current position in `31`.** Exposed as opaque newtype with `as_str()` + `Display`. Construction is crate-private to `semstrait-model` (the YAML parser is the primary producer).

**Next step.** Ratify the exact shape in `32` (semstrait-model API contract). If `32` prefers a `Display`-only surface without `as_str()`, amend `31 §7.3` accordingly.

---

## Q7 — Should `SemanticExpr` / `PhysicalExpr` be `#[non_exhaustive]`?

**Question.** `31 §3.2` / `§3.3` apply the "newtype-over-stable" exception per `30` and do NOT tag `SemanticExpr` / `PhysicalExpr` as `#[non_exhaustive]`. This matches the `30` rule. But the inner `Expr` IS `#[non_exhaustive]`, which means a `SemanticExpr` may legally hold a future `Expr` variant not yet known to external consumers. Is the newtype still "stable over the wrapped shape" in that setting?

**Refs.**
- `30` — (parallel draft) stability policy, newtype-over-stable exception.
- I10 — every public sum type non-exhaustive.

**Argument for NOT applying `#[non_exhaustive]`.**
- The wrapper's shape (fields) is stable; consumers do not pattern-match on the newtype, they call methods.
- Adding `#[non_exhaustive]` to a single-field newtype is a style noise with no concrete consumer benefit.

**Argument for applying it.**
- Signals to downstream that the wrapper's **invariants** may tighten. If a future `ValidateError::ExoticVariantInSemanticExpr` is added, the wrapper's semantic contract widens — and that widening is invisible without the marker.

**Current position in `31`.** No `#[non_exhaustive]` on the two wrappers, per `30`. Decision parked for a look-back once `30` fully ratifies.

**Next step.** Revisit with `30`'s final draft.

---

## Q8 — `is_reserved_tag` table source of truth

**Question.** `31 §9.2` exposes `is_reserved_tag(&str) -> bool` with the 21-tag list inline. `14 §4.4.1` is the authoritative table. If a new reserved tag is added to `14 §4.4.1`, how is `is_reserved_tag` kept in sync?

**Proposals.**
- **A.** Hand-maintain both; add a doctest that asserts the function returns `true` for every tag in a fixture mirroring `14 §4.4.1`. Drift is caught in CI.
- **B.** Source-of-truth is the `ExprBlock` enum (§3.4): `is_reserved_tag(s)` matches against `ExprBlock`'s discriminant names. Requires `ExprBlock` to be a 1:1 enumeration (see Q4).
- **C.** Source-of-truth is a static slice `RESERVED_TAGS: &[&str]`; `is_reserved_tag` is a binary search over it.

**Current position in `31`.** Not chosen. The §9.2 description is prose-only.

**Next step.** Decide during implementation. If Q4 resolves to "keep `ExprBlock` internal", proposal C is the natural choice.
