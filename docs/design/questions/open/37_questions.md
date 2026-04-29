---
doc: design/questions/open/37_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `apis/37_semstrait_catalog.md`
depends-on:
  - apis/37_semstrait_catalog.md
  - apis/30_api_contracts.md
  - apis/31_semstrait_core.md
  - apis/33_semstrait_manifest.md
  - apis/35_semstrait_ir.md
  - foundations/15_mapping_and_binding.md
---

# Open Questions — `apis/37_semstrait_catalog.md`

> Eleven questions remain open: Q-CAT-002 through Q-CAT-012. Closed items moved to [`../closed/37_questions.md`](../closed/37_questions.md). Each entry restates the question, lists its ratified references, and records the Round-1 default currently used. Entries migrate out of this file as later docs (primarily `33`, and amendments to `30`) make decisions that either confirm or amend `37`'s defaults. None of these items block the headline ratifications in `37 §15`.

---

## Q-CAT-001 — `CAT_E_*` / `FS_E_*` subsystem-prefix registration in `30 §6.2` — CLOSED

> **Moved to [`../closed/37_questions.md`](../closed/37_questions.md#q-cat-001--cat_e_--fs_e_-subsystem-prefix-registration-in-30-62--closed--superseded-by-typed-kind-transition).** The typed-kind transition at `30 §6` retired the stable string-code subsystem; `CatalogProviderErrorKind` and `FileSystemErrorKind` are now independent typed enums with no prefix-table entry. Sub-questions (a) / (b) / (c) all dissolve.

---

## Q-CAT-002 — Glob-matching semantics: `semstrait-core` vs `semstrait-catalog`

**Question.** `37 §7.1` delegates the glob-match predicate to `semstrait-core` (via a `glob_match` free function or `GlobPattern` type), while owning the prefix-extraction and list-then-filter orchestration locally. Is `semstrait-core` the right home for glob semantics, or should the predicate live here (since `semstrait-catalog` is effectively the only caller that needs it at runtime)?

**Refs.**

- `31` — `semstrait-core` already defines `GlobPattern` (confirmed via existing `crates/semstrait-core/src/types.rs`).
- `15 §6.3` — `Source::Path { glob: ... }` bindings use glob expansion at compile time.
- `37 §7` — `expand_glob` utility orchestrates a prefix-then-filter dance on a `FileSystem`.

**Arguments for glob predicate in `semstrait-core` (current Round-1 default).**

- `GlobPattern` is already there. Moving it would churn downstream crates (manifest, model parser) that already refer to it.
- Core is the correct home for a primitive used across both catalog-bound and manifest-bound code paths.
- Keeps `semstrait-catalog`'s dependency footprint minimal — it only adds the orchestration shell.

**Arguments for glob predicate in `semstrait-catalog`.**

- The ONLY runtime consumer of `glob_match` is `expand_glob` in this crate. Core is hosting it for a single consumer.
- If glob semantics need to evolve (e.g. Windows-style `\` handling, Unicode categories), the catalog crate is closer to the I/O reality than core.

**Current position in `37`.** Predicate stays in `semstrait-core`; orchestration (`expand_glob`) lives here. `expand_glob` imports `GlobPattern` / `glob_match` from core.

**Next step.** Confirm during `31` ratification pass. If `31` decides to shed glob semantics (unlikely), `37`'s `§7` moves the predicate here with a minor signature change.

---

## Q-CAT-003 — Snapshot pinning for catalogs without snapshot IDs

**Question.** `UnityCatalogProvider` (`37 §4.3`) does not expose snapshot IDs through its API — Delta-table versioning lives in the Delta log, not the Unity metadata API. `37 §4.3` currently specifies that `get_snapshot` returns `SnapshotMetadata` with `version: SnapshotVersion::Current` and no pin ID, and `check_schema_drift` still runs but cannot be snapshot-correlated. Is this the right v1 behavior, or should Unity-bound tables either (a) refuse snapshot-pinned compilation entirely, or (b) read the Delta log directly through a composed `FileSystem` to obtain a version?

**Refs.**

- `37 §4.3` — current Unity provider contract.
- `37 §9.3` — deterministic contract for `check_schema_drift`; promises idempotency "per snapshot" but does not require a pin ID.
- `15 §5.4` — snapshot pinning as a compile-time artifact of manifest compilation.
- `docs/CATALOG_RESOLUTION.md §2` — graceful-degradation table for missing catalog capability.

**Arguments for "current-only, no pin, no Delta-log read" (current Round-1 default).**

- Keeps `UnityCatalogProvider` strictly inside the Unity REST surface. No secondary `FileSystem` dependency on the Unity provider.
- The Unity user explicitly opts out of cross-run snapshot pinning by choosing Unity; this is an informed trade-off documented in `§4.5`.
- Simpler v1 contract.

**Arguments for "refuse snapshot-pinned compilation".**

- Determinism guarantees (`00 §9 I4`) are strongest when snapshot IDs exist. Silently downgrading to "current" risks users assuming pinning when none happens.
- A loud error at compile time is better than a silent subtle drift risk at query time.

**Arguments for "read Delta log via composed `FileSystem`".**

- Would give Unity-bound tables full pinning semantics.
- Costs: `UnityCatalogProvider` now needs a `FileSystem` dependency injection, which complicates its construction. Also couples provider to Delta format specifics.

**Current position in `37`.** Option (a) — `SnapshotVersion::Current`, no pin ID, drift gate still operational. Documented as a limitation in `§4.3` and `§4.5`.

**Next step.** Revisit when `33` ratifies `SemanticManifest::verify_against_catalog` — if that method wants to promise reproducibility, the Unity path needs clearer downgrade signaling (e.g. a mandatory diagnostic when `SnapshotVersion::Current` is the best the provider can offer).

---

## Q-CAT-004 — Scheme-dispatching `FileSystem` in v1?

**Question.** `37 §6.5` leaves scheme-dispatching ("`s3://` goes to `S3FileSystem`, `gs://` goes to `GcsFileSystem`") as a caller concern in v1. Should this crate ship a `DispatchingFileSystem` utility that accepts a map of scheme → concrete filesystem, or is caller composition the right posture?

**Refs.**

- `37 §6.5` — current posture: caller-composed.
- `37 §10.1` — wiring example shows a single concrete filesystem.

**Arguments for caller-composed (current Round-1 default).**

- Avoids adding yet another public type in v1.
- Real deployments typically target one primary storage backend; mixed-scheme pipelines are the minority.
- Keeps the provider-construction surface lean.

**Arguments for shipping `DispatchingFileSystem`.**

- A non-trivial fraction of real workloads mix schemes (e.g. `s3://` for data, `file://` for dev overrides).
- Caller implementations of a dispatcher are near-identical boilerplate; shipping one reduces duplication and subtle-bug risk.
- Would align with how `FilesystemCatalogProvider` wraps a single `FileSystem` — the dispatcher pattern is already in the design.

**Current position in `37`.** Caller-composed in v1.

**Next step.** Revisit after first round of adapter integration: if 2+ adapter crates re-invent the dispatcher, promote it into `semstrait-catalog` in a MINOR.

---

## Q-CAT-005 — `FileSystem` extensions: streaming reads, deletion

**Question.** `37 §5.2` omits `read_stream` (range / streaming read) and `delete` from the v1 `FileSystem` trait. Should these be included in v1 with default-error implementations, or omitted entirely?

**Refs.**

- `37 §5.2` — current method set: `list`, `read`, `write`, `exists`.
- `37 §5.5` — non-goals lists both streaming and delete explicitly.
- `37 §11.1` — MINOR method-set growth requires a sensible default (error code, not silent empty).

**Arguments for omission (current Round-1 default).**

- Minimal surface = easier correctness review in v1.
- Streaming reads are performance-critical only for the adapter hot path, which does NOT use this trait (per I11). Compile-time reads are bounded: manifest is small, metadata files (Parquet footers if ever needed) fit in memory.
- Deletion is a mutation and overlaps with the "no mutation in v1" decision (`Q-CAT-006`).

**Arguments for including now with default-error impls.**

- Establishes the shape early so MINOR additions don't churn every third-party impl.
- `delete` is specifically useful for write-side artifact management (adapters producing intermediate files that need cleanup).

**Current position in `37`.** Omitted. Adapters that need streaming reads will request the addition with a ratified MINOR path.

**Next step.** Revisit at the first adapter ratification (`36`) — if a real adapter has an unmet streaming need, add `read_stream` with `FS_E_0199`-default.

---

## Q-CAT-006 — Catalog mutation: companion trait vs `CatalogProvider` extension

**Question.** `37 §12` states "No mutation of catalog state" as a v1 boundary. A future MINOR will add write-side operations (`create_table`, `commit_snapshot`, `register_table`). Should this land as (a) a separate `CatalogMutator` trait implemented alongside `CatalogProvider`, or (b) as default-erroring methods on `CatalogProvider` itself?

**Refs.**

- `37 §3.5` — current non-goal list.
- `37 §11.1` — method-set growth mechanism.
- `30 §3` — open/sealed trait posture.

**Arguments for separate `CatalogMutator` trait (current Round-1 default).**

- Keeps the read surface clean. Compile-time callers that only need reads never see mutation methods.
- Third-party read-only providers (common case) don't pay the default-impl tax.
- Aligns with `00 §4`'s "minimum narrow-waist" principle.

**Arguments for adding to `CatalogProvider`.**

- One fewer trait to remember.
- Default-erroring methods are a reasonable v1 migration path — existing impls don't break.

**Current position in `37`.** Separate trait in a future MINOR. Open for reconsideration when write needs are concrete.

**Next step.** Defer until the first write-side consumer emerges (likely `semstrait-writer` or similar). No action required in Round-1.

---

## Q-CAT-007 — `async fn` in traits: `async_trait` macro vs native

**Question.** `37 §3.2` / `§5.2` use `#[async_trait]` on the trait definitions. Rust 1.75+ supports native `async fn` in traits with `trait_variant::make` for object-safety. Should v1 commit to the macro or switch now?

**Refs.**

- `37 §10.5` — current posture: `async_trait` in v1.
- `30 §9` — async posture per crate.

**Arguments for `async_trait` (current Round-1 default).**

- Object-safe trait objects (`Arc<dyn CatalogProvider>`) work out of the box.
- Well-understood semantics, broad tooling support.
- Native `async fn` in traits still has ergonomic rough edges for `dyn` consumers (requires `dyn + Send` bounds that are non-trivial to spell).

**Arguments for native `async fn`.**

- Zero allocation per call vs. `async_trait`'s `Box<dyn Future>` indirection.
- No macro expansion, better rust-analyzer tooling.
- Idiomatic modern Rust.

**Current position in `37`.** `async_trait` in v1. Migration path is source-compatible for most callers (method signatures look identical) so a MINOR switch is plausible.

**Next step.** Revisit at Rust edition rollover or when `trait_variant` + `dyn`-compatibility ergonomics stabilize further.

---

## Q-CAT-008 — `Schema` / `SchemaColumn` ownership: this crate or `semstrait-core`?

**Question.** `37 §2.1` places `Schema` and `SchemaColumn` in `semstrait-catalog`. `15 §3.2` also uses `Schema` as the shape inside `PhysicalSource`. If `semstrait-manifest` consumes `Schema` from two directions (from `CatalogProvider::get_schema` and from its own `PhysicalSource` construction), should `Schema` live in `semstrait-core` as shared vocabulary?

**Refs.**

- `37 §2.1` — current ownership: this crate.
- `15 §3.2` — `Schema` used in `PhysicalSource`.
- `31` — `semstrait-core` public types list.

**Arguments for ownership here (current Round-1 default).**

- Catalog returns the type; it is *catalog-shaped* first, manifest-consumed second.
- `semstrait-manifest` already imports from this crate (for `TableRef`, `NamespaceRef`, `Partition`); adding `Schema` to that import list is zero-cost.
- Keeps `semstrait-core` narrow — core should own language primitives (`DataType`, `ColumnName`, `Span`), not domain-structural types.

**Arguments for `semstrait-core` ownership.**

- `Schema` is shared vocabulary between the catalog trait surface and the manifest's physical-source types; neutral territory is the natural home.
- Avoids the appearance that `semstrait-manifest` "reaches through" `semstrait-catalog` to get a type it needs structurally.

**Current position in `37`.** Owned here. `semstrait-manifest` imports `Schema` from `semstrait-catalog`.

**Next step.** Confirm during `31` and `33` drafting. If either insists on moving the type, `37 §2.1` updates; the trait signatures remain unchanged (import path only).

---

## Q-CAT-009 — `expand_glob` return type: `Vec<Path>` vs `Vec<FileEntry>`

**Question.** `37 §7.1` specifies `expand_glob` returns `Vec<Path>` (paths only). A richer return of `Vec<FileEntry>` (path + size + modified-at) preserves metadata that manifest compilation could use for determinism audits (has the glob expansion drifted since last compile?).

**Refs.**

- `37 §7.1` — current signature.
- `37 §7.3` — determinism requirement.
- `37 §2.3` — `Path` / `FileEntry` definitions.
- `00 §9 I4` — manifest determinism.

**Arguments for `Vec<Path>` (current Round-1 default).**

- Simpler. Every caller today wants just the paths.
- Callers that need richer metadata can follow up with targeted `fs.list()` calls.

**Arguments for `Vec<FileEntry>`.**

- Preserves information that `fs.list` already produces — no cost on the wire.
- Enables deterministic audit: "the same paths matched, but did any of them change?" answerable without extra calls.
- Aligns `expand_glob` return type with `FileSystem::list`.

**Current position in `37`.** `Vec<Path>`. Cheap to widen in a MINOR if needed.

**Next step.** Revisit at `33` ratification when manifest-determinism auditing is concretely specified.

---

## Q-CAT-010 — Partition-transform enumeration: Iceberg-exact vs portable subset

**Question.** `37 §2.3` / `§12` specify `PartitionTransform` mirrors the Iceberg v2 spec exactly (`Identity`, `Year`, `Month`, `Day`, `Hour`, `Bucket(u32)`, `Truncate(u32)`, `Void`). Unity Catalog exposes a different transform set (effectively just `Identity` on partition columns, no Iceberg-style transforms). `FilesystemCatalogProvider`'s Hive-style inference produces `Identity`-only. Is Iceberg-exact the right v1 vocabulary, or should `37` define a portable superset that covers Iceberg + Hive + whatever future catalogs emit?

**Refs.**

- `37 §12` — current enumeration (Iceberg v2 exact, `#[non_exhaustive]`).
- `15 §6.5` — Hive-style partition extraction.
- `docs/CATALOG_RESOLUTION.md §4` — legacy transform parsing (Iceberg-only).

**Arguments for Iceberg-exact (current Round-1 default).**

- Iceberg is the primary metadata source for semstrait v1. Optimizing the vocabulary for the primary consumer is correct.
- `#[non_exhaustive]` leaves room to add transforms from other catalog families as they are encountered.
- Unity and `FilesystemCatalogProvider` only produce `Identity`, which is already in the set.

**Arguments for a portable superset.**

- Reduces per-catalog variant branching in downstream code (though I3 forbids branching on *source*, downstream MAY branch on *transform kind* — that's legitimate).
- Future-proof against catalogs with novel transform semantics.

**Arguments for `PartitionTransform::Other(String)` escape hatch.**

- Lets new catalog integrations express transforms that don't map to known variants, without a crate release.
- Downside: downstream branching (ever touching `.Other(_)`) risks fragile behavior.

**Current position in `37`.** Iceberg-exact + `#[non_exhaustive]`. No `Other(String)` escape hatch in v1.

**Next step.** Revisit at the first non-Iceberg-non-Unity catalog integration request. If the first such request has a transform that doesn't fit Iceberg's model, either add a variant (MINOR) or introduce the escape hatch (ratified MINOR).

---

## Q-CAT-011 — `FilesystemCatalogProvider` schema source

**Question.** `37 §4.4` specifies `FilesystemCatalogProvider::get_schema` returns `Ok(Schema::empty())` because this provider does NOT parse format headers (that would violate `§12`). SemanticManifest authors must supply schemas explicitly. Is this the right posture, or should `FilesystemCatalogProvider` accept a schema-provider callback at construction time (e.g. "when asked for schema of `s3://bucket/path`, call *this* function") so the user can plug in whatever header-parser they like?

**Refs.**

- `37 §4.4` — current posture.
- `37 §12` — "No format-header schema reading" boundary.
- `15 §6.3` — glob-bound sources; schema is either manifest-declared or absent.

**Arguments for empty-schema (current Round-1 default).**

- Honors the crate boundary cleanly. Format parsing stays strictly in adapter territory (or in manifest if the author does it manually).
- No coupling between `semstrait-catalog` and any format-parsing crate.

**Arguments for pluggable schema callback.**

- SemanticManifest authors routinely want "just point me at a directory and figure out the schema" — removing that capability entirely is a regression from `StorageProvider::read_schema`.
- The callback is caller-supplied, so it doesn't violate the boundary — `semstrait-catalog` never imports a format-parser.

**Current position in `37`.** Empty schema. The legacy `StorageProvider::read_schema` path is deliberately removed.

**Next step.** Revisit once `15 §6.3` firms up. If glob-bound sources routinely need schema inference, the callback pattern is the minimally-invasive fix.

---

## Q-CAT-012 — `CatalogRegistry` ownership

**Question.** `37 §10.1` places `CatalogRegistry` in `semstrait-manifest`. Legacy code has `CatalogRegistry` in `semstrait-catalog` (see `crates/semstrait-catalog/src/registry.rs`). Where should the registry live for v1?

**Refs.**

- `37 §10.1` — current position: `semstrait-manifest`.
- Legacy `crates/semstrait-catalog/src/registry.rs` — existing home.
- `33` — `semstrait-manifest` draft (pending).

**Arguments for `semstrait-manifest` ownership (current Round-1 default).**

- Registry is a *composition* of providers, and composition is a manifest-compile concern (the registry is consumed by `compile(model, &registry)`).
- Keeps `semstrait-catalog`'s surface minimal: traits + impls + errors + glob utility, no collection types.
- Avoids leaking the multi-provider multiplexing pattern into the catalog crate.

**Arguments for `semstrait-catalog` ownership.**

- Matches legacy code — no migration cost.
- Registry is "about catalogs," so catalog crate is the obvious home.

**Current position in `37`.** `semstrait-manifest` owns `CatalogRegistry`. `semstrait-catalog` does NOT export a registry type.

**Next step.** Confirm during `33` ratification. If `33` decides not to own the registry, it lands back here as a `§2.1` addition.
