---
doc: design/questions/open/33_questions
status: Living
purpose: Round-1 open items raised against `apis/33_semstrait_manifest.md`
---

# 33 — Open Questions

Unresolved items arising while drafting `docs/design/apis/33_semstrait_manifest.md`. Each entry restates the question, lists the relevant ratified references, and proposes a lean next step so a later decision pass can resolve without re-reading the whole doc.

---

## Q1 — `compile` input: by value or by reference

**Question.** Should `compile` accept `SemanticModel` by value (consuming it) or by `&SemanticModel` (sharing it)? `33 §9.2` currently ratifies by-value.

**Refs.**
- `33 §9.1`–`§9.2` — current signature and rationale.
- `32` — `SemanticModel` ownership discipline (parse returns an owned value).
- `10 §3.3` — compile-stage contract (input model + catalog; no statement about ownership).

**Arguments pro by-value.**
- Matches the stage boundary: once compile starts, the Model is no longer the caller's responsibility. Ownership move makes that explicit.
- Avoids requiring `SemanticModel` to be `Clone` at the call site.
- Simplifies internal compile passes — the driver is free to move substructures out of the model into the SemanticManifest.

**Arguments pro by-reference.**
- Callers that A/B-compile the same Model against two catalog snapshots must clone today. A `&SemanticModel` signature would let them skip the clone.
- Aligns with `validate`'s current signature (`validate(&SemanticModel, ...)`).

**Current position in `33`.** Ratified as by-value per `33 §9.1`.

**Next step.** Revisit after `semstrait-api`'s multi-snapshot compile story firms up. If A/B-compile becomes a common pattern, add an `&Arc<SemanticModel>` overload rather than changing the primary signature.

---

## Q2 — `CompileError` as unified enum vs split

**Question.** `33 §10.1`'s `CompileError` extends `semstrait-core::CompileError` by re-exporting core variants and adding SemanticManifest-layer variants. Should this remain a single enum, or should `33` expose a split `CoreCompileError` + `SemanticManifestCompileError` pair?

**Refs.**
- `31 §8.3` — `semstrait-core::CompileError` variant roster.
- `33 §10.1` — unified-enum posture.
- `30 §5` — typed-error-carrier discipline.

**Arguments pro unified.**
- Callers pattern-match on one enum. Simpler.
- Stable-code discipline is preserved regardless of enum layout.

**Arguments pro split.**
- Clearer crate-boundary semantics: core-layer concerns stay in core; SemanticManifest-layer concerns stay in `33`.
- Easier to audit what each crate contributes.
- A core-variant change is guaranteed not to touch the SemanticManifest-layer enum.

**Current position in `33`.** Unified per `33 §10.1`. Tracked as `[TD-33-ERROR-UNIFY]`.

**Next step.** Stay unified through v1; revisit if a third compile-like stage (e.g. a hypothetical `semstrait-rebase`) introduces a third error bucket that would be clearer as a separate enum.

---

## Q3 — `canonical_bytes()` encoder choice

**Question.** `33 §14.3` ratifies bincode-with-sorted-fields as the `canonical_bytes()` encoder. Should it instead lean on `serde_json` with `preserve_order`?

**Refs.**
- `33 §13.2` — determinism contract.
- `33 §14.3` — current ratification.
- `30 §10.4` — serde-feature discipline.

**Arguments pro bincode.**
- Smaller bytes; faster encode / decode.
- Closely matches the content-addressable caching use case (the bytes are hashed, not read).

**Arguments pro JSON.**
- Human-inspectable. Debugging determinism failures is materially easier when the canonical form can be diffed.
- Existing `semstrait-cli` already carries a JSON renderer; reusing it avoids shipping a second encoder for this one purpose.

**Current position in `33`.** bincode per `33 §14.3`. Tracked as `[TD-33-CANONICAL-JSON]`.

**Next step.** Decide after the first round of content-addressable caching benchmarks. If the byte-size delta is negligible at realistic SemanticManifest sizes, switch to JSON-with-`preserve_order` for debuggability.

---

## Q4 — `Repository::save` content-hash pre-condition

**Question.** Should `Repository::save` verify that the incoming SemanticManifest's derived `SemanticManifestId` matches its internal content before persisting?

**Refs.**
- `33 §11.1` — current surface (no pre-condition; id is derived inside `save`).
- `33 §11.2` — `IntegrityViolation` variant covers the case.

**Arguments pro pre-condition.**
- Catches hand-crafted / serialization-corrupted SemanticManifests at save-time rather than at later load-time.
- Enforces the SemanticManifest invariant that `SemanticManifestId::from_manifest(&m)` is stable.

**Arguments against.**
- Extra hashing per save. Production cost is non-trivial for large SemanticManifests.
- `save` is idempotent anyway; a corrupt SemanticManifest either fails to decode on load (caught by `DecodeFailed`) or yields a valid-but-wrong SemanticManifest (caught by `IntegrityViolation` when referenced).

**Current position in `33`.** No pre-condition in v1.

**Next step.** Add a `Repository::save_checked(manifest, expected_id)` as a MINOR addition if `IntegrityViolation` reports become frequent in telemetry.

---

## Q5 — `SemanticManifestEncoding::Bincode` enable-in-v1

**Question.** Should `SemanticManifestEncoding::Bincode` be exposed in v1 (alongside `MessagePack` and `Json`) or deferred?

**Refs.**
- `33 §14.2` — current ratification: deferred.
- `33 §11.4` — `SemanticManifestEncoding` enum.

**Arguments pro enable.**
- Fastest + smallest of the three encodings for the size-insensitive single-machine case.
- Easy addition — no discriminator bump, just a new variant.

**Arguments against.**
- Non-self-describing. A future schema migration would need a side-channel version tag to decode correctly.
- Three encodings is enough; `MessagePack` already covers the size-sensitive case.

**Current position in `33`.** Deferred per `33 §14.2`. Tracked as `[TD-33-BINCODE]`.

**Next step.** Enable if a concrete consumer surfaces that needs bincode's characteristics. MINOR addition any time.

---

## Q6 — `FileSystemRepository` locking / atomic-write discipline

**Question.** Should `FileSystemRepository` carry a locking discipline (POSIX advisory locks, tempfile+rename) to protect against concurrent writers and partial-write-on-crash?

**Refs.**
- `33 §11.4` — current ratification: no locks; concurrent saves of the same id are idempotent no-ops.

**Arguments pro discipline.**
- A crash during write leaves a half-written file. Reading that file later with `Repository::load` yields a decode error rather than a clean missing-id error.
- The fix is well-known (`tempfile::persist`); the cost is small.

**Arguments against.**
- POSIX advisory locks vary by filesystem (NFS, FUSE); a cross-platform impl is not trivial.
- In practice, SemanticManifests are written once at compile; partial-write risk is small.

**Current position in `33`.** No locks in v1.

**Next step.** Add a tempfile-and-rename discipline in a PATCH release; MINOR only if the trait signature changes.

---

## Q7 — `check_schema_drift` trait placement

**Question.** Should `check_schema_drift` live on `CatalogProvider` (current `33 §12` ratification from `37`) or on a separate `DriftChecker` trait?

**Refs.**
- `33 §12` — current forward-ref.
- `37` (pending) — authoritative surface.

**Arguments pro single trait.**
- Keeps the I/O surface narrow (one catalog-side object, not two).
- Drift checking logically is a catalog operation.

**Arguments pro split.**
- A repository-only caller (e.g. `semstrait-facade` loading a cached SemanticManifest) could skip bringing in a full `CatalogProvider` if only drift checking were needed.
- Separates read-side (`fetch_schema`) from validate-side (`check_schema_drift`) concerns.

**Current position in `33`.** Single-trait per the `37` ratification. `33` defers.

**Next step.** Decide as part of `37`. `33` updates the §12 forward-ref to match.

---

## Q8 — `SemanticManifest::datakind` / `relationship` return shape

**Question.** Should `SemanticManifest::datakind(name)` and `SemanticManifest::relationship(id)` return `Option<&T>` or `Result<&T, SemanticManifestLookupError>`?

**Refs.**
- `33 §3.4` — current ratification: `Option`.

**Arguments pro `Option`.**
- Matches `BTreeMap::get`-style idioms.
- Callers routinely write `.get().ok_or_else(|| ...)` when they want a typed error.

**Arguments pro `Result`.**
- Makes the missing-entry case louder — impossible to ignore the error path.
- The lookup-error variant would carry code `COMP_E_...-LOOKUP-NOT-FOUND` and route through the Diagnostic pipeline uniformly.

**Current position in `33`.** `Option` per `33 §3.4`.

**Next step.** Stay `Option`. If the error-path ergonomics become a pain, add `SemanticManifest::datakind_or_err` helpers as MINOR additions.

---

## Q9 — `ResolvedRelationshipGraph` public-field vs accessor

**Question.** Should `ResolvedRelationshipGraph` be a public field on `SemanticManifest` (promoted from `33 §8.2`'s accessor-only posture) or remain behind the accessor?

**Refs.**
- `33 §8.2` — current ratification: accessor-only; `pub(crate)` field.

**Arguments pro public field.**
- Consistency with other `SemanticManifest` fields.
- One fewer indirection for planner code.

**Arguments pro accessor-only.**
- Lets `33` evolve the graph's internal representation without MAJOR churn.
- Future MINOR additions to the graph (transitive closure cache, bidirectional adjacency index) can happen behind the accessor.

**Current position in `33`.** Accessor-only per `33 §8.2`.

**Next step.** Keep accessor-only through v1. If planner hot-path profiling shows the accessor is a meaningful cost, promote to a public field as MINOR.

---

## Q10 — Compile catalog-error accumulation

**Question.** Should `compile` make a narrow exception to the fail-fast policy and accumulate **all** catalog-related errors (e.g. every `SourceNotFound` in one pass) before failing, or should it stay strictly fail-fast per `10 §5`?

**Refs.**
- `10 §5` — per-stage error-emission policy.
- `33 §10.4` — current fail-fast rationale.
- `14b §5` — cycle-detection-first pre-pass (a related pragmatic exception).

**Arguments pro narrow exception.**
- Catalog errors don't cascade into downstream-expression pseudo-errors (they're pre-expression-resolution). Accumulating them gives the author a complete picture of every missing source in one compile.
- Catalog I/O is parallelizable; batching errors from parallel I/O is cheap.

**Arguments against.**
- Adds an exception to a uniform policy. Future maintainers may extend the exception elsewhere until fail-fast is effectively gone.
- The cost of re-running `compile` on a catalog-error fix is dominated by the I/O; the extra round doesn't save much wall time.

**Current position in `33`.** Strict fail-fast per `33 §10.4`.

**Next step.** Revisit if real-world compile traces show users hitting multiple catalog errors per compile routinely; add a `compile_collecting_catalog_errors` variant as MINOR if so.
