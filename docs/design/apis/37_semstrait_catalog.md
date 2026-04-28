---
doc: design/apis/37_semstrait_catalog
status: Round-1 Draft
prereqs:
  - 15
authoritative-for:
  - "`CatalogProvider` trait surface (method set, signatures, async posture)"
  - "`FileSystem` trait surface (method set, signatures, async posture)"
  - "`CatalogError` enum (variants, stable `CAT_E_*` codes)"
  - "`FileSystemError` enum (variants, stable `FS_E_*` codes)"
  - "Built-in `CatalogProvider` implementations roster (v1)"
  - "Built-in `FileSystem` implementations roster (v1)"
  - "Shared glob-expansion utility contract (`expand_glob`)"
  - "Schema-drift gated I/O contract (I11b binding)"
  - "Caller responsibilities: construction, injection, thread-safety"
references:
  - apis/30_api_contracts.md
  - apis/31_semstrait_core.md
  - apis/35_semstrait_ir.md
  - foundations/15_mapping_and_binding.md
  - 00_overview.md
---

# `semstrait-catalog` — Public API Contract

> Round-1 draft. Ratifies the public surface of the `semstrait-catalog` crate: two open traits (`CatalogProvider`, `FileSystem`), their v1 built-in implementations, and the error hierarchies they produce. This crate owns all metadata-source and generic-I/O integration for the semstrait workspace; no other crate performs catalog or object-store I/O.

---

## 1. Purpose, scope, layering

### 1.1 Purpose

`semstrait-catalog` provides the single narrow waist through which the semstrait toolchain reaches external metadata systems and byte-level storage. It exposes two independent abstractions:

- `CatalogProvider` — structured metadata access (namespaces, tables, schemas, partitions, snapshots) against a catalog service (Iceberg REST, Unity Catalog, or a filesystem-derived catalog).
- `FileSystem` — format-agnostic byte-level I/O against an object store or local disk (list, read, write, exists).

Both traits are `async` because every real-world implementation performs network I/O. Both are **open** per `30 §10` — third parties MAY implement them to plug additional metadata sources or storage backends into the toolchain without modifying any crate in the `3x` map.

### 1.2 Scope — what this crate owns

- Trait definitions (`CatalogProvider`, `FileSystem`) and their structural value types (`CatalogId`, `Path`, `TableRef`, `NamespaceRef`, `ResolvedTable`, `Schema`, `SchemaColumn`, `Partition`, `PartitionTransform`, `FileFormat`, `SnapshotVersion`, `SnapshotMetadata`, `DriftReport`, `DriftStatus`, `DriftKind`, `FileEntry`).
- Built-in `CatalogProvider` implementations: `NoopCatalogProvider`, `IcebergRestCatalogProvider`, `UnityCatalogProvider`, `FilesystemCatalogProvider`.
- Built-in `FileSystem` implementations: `LocalFileSystem`, `S3FileSystem`, `AzureFileSystem`, `GcsFileSystem`.
- Error types (`CatalogError`, `FileSystemError`) and their stable `CAT_E_*` / `FS_E_*` code surface.
- Shared glob-expansion utility (`expand_glob`) that composes `FileSystem` with a client-supplied pattern.
- The schema-drift gated I/O contract invoked by `semstrait-manifest` under I11b, and no other query-time I/O entry point.

### 1.3 Scope — what this crate does NOT own

- **No planning.** Has no awareness of `SemanticPlan`, `PhysicalPlan`, `SemanticExpr`, `PhysicalExpr`, or any tree node type from `semstrait-ir` (`35`).
- **No SQL emission.** Does not render SQL, nor any engine-specific string. SQL generation is exclusively an adapter concern (`36`).
- **No format-header schema reading.** `FileSystem::read` returns raw bytes; it does NOT parse Parquet footers, ORC stripes, CSV headers, Avro schemas, etc. Schema knowledge comes from `CatalogProvider` (for tables) or from the manifest author (for glob-expanded file sets — see `15 §6.3`).
- **No catalog-specific branching.** Per `30 §3` / `00 §9 I3` (metadata-source-independence), canonical crates MUST route every catalog interaction through `CatalogProvider` without inspecting its concrete type.
- **No expression evaluation, no caching policy, no retry machinery, no schema inference from data.** All such concerns are callers' or adapters' responsibilities.

### 1.4 Layering

```
caller (semstrait-manifest at compile time, semstrait-manifest at I11b gate, tests)
    ↓ depends on traits only
CatalogProvider trait            FileSystem trait          (defined here)
    ↓                                ↓
IcebergRestCatalogProvider       LocalFileSystem
UnityCatalogProvider             S3FileSystem
FilesystemCatalogProvider ───── uses ─────→ FileSystem (composition)
NoopCatalogProvider              AzureFileSystem
                                 GcsFileSystem
```

`semstrait-catalog` depends only on `semstrait-core` (`31`). It does NOT depend on `semstrait-manifest`, `semstrait-ir`, `semstrait-planner`, or any adapter crate. This keeps metadata-source concerns behind a single narrow dependency edge and preserves the I3 independence axis.

---

## 2. Public crate surface

### 2.1 Roster

| Name                            | Kind           | `#[non_exhaustive]`? | Stability (v1) |
|---------------------------------|----------------|----------------------|----------------|
| `CatalogProvider`               | trait          | n/a (trait sealing)  | **Open**       |
| `FileSystem`                    | trait          | n/a                  | **Open**       |
| `CatalogError`                  | enum           | yes                  | Provisional    |
| `FileSystemError`               | enum           | yes                  | Provisional    |
| `CatalogId`                     | newtype struct | no (field-stable)    | Stable         |
| `Path`                          | newtype struct | no (field-stable)    | Stable         |
| `TableRef`                      | struct         | no (field-stable)    | Provisional    |
| `NamespaceRef`                  | struct         | no (field-stable)    | Provisional    |
| `ResolvedTable`                 | struct         | yes                  | Provisional    |
| `Schema`                        | struct         | no                   | Provisional    |
| `SchemaColumn`                  | struct         | no                   | Provisional    |
| `Partition`                     | struct         | yes                  | Provisional    |
| `PartitionTransform`            | enum           | yes                  | Provisional    |
| `FileFormat`                    | enum           | yes                  | Provisional    |
| `SnapshotVersion`               | enum           | yes                  | Provisional    |
| `SnapshotMetadata`              | struct         | yes                  | Provisional    |
| `DriftReport`                   | struct         | yes                  | Provisional    |
| `DriftStatus`                   | enum           | yes                  | Provisional    |
| `DriftKind`                     | enum           | yes                  | Provisional    |
| `FileEntry`                     | struct         | yes                  | Provisional    |
| `NoopCatalogProvider`           | struct         | no                   | Stable         |
| `IcebergRestCatalogProvider`    | struct         | no                   | Stable         |
| `UnityCatalogProvider`          | struct         | no                   | Stable         |
| `FilesystemCatalogProvider`     | struct         | no                   | Stable         |
| `LocalFileSystem`               | struct         | no                   | Stable         |
| `S3FileSystem`                  | struct         | no                   | Stable         |
| `AzureFileSystem`               | struct         | no                   | Stable         |
| `GcsFileSystem`                 | struct         | no                   | Stable         |
| `expand_glob`                   | free function  | n/a                  | Stable         |

"Stability (v1)" follows `30 §8`. "Provisional" means field or variant additions permitted in MINOR under documented migration; breaking changes only in MAJOR.

### 2.2 Module layout (informative)

```rust
pub mod traits;             // CatalogProvider, FileSystem
pub mod types;              // CatalogId, Path, TableRef, NamespaceRef,
                            //   Schema, Partition, FileEntry, ResolvedTable, ...
pub mod error;              // CatalogError, FileSystemError
pub mod glob;               // expand_glob
pub mod providers;          // NoopCatalogProvider, IcebergRestCatalogProvider, ...
pub mod filesystems;        // LocalFileSystem, S3FileSystem, ...

pub use traits::{CatalogProvider, FileSystem};
pub use types::*;
pub use error::{CatalogError, FileSystemError};
pub use glob::expand_glob;
```

Public items are re-exported at the crate root per `30 §4.3`.

### 2.3 Structural-type sketches

Minimal shape per public value type; full field rosters live alongside the traits that consume them (`§3`, `§5`, `§9`).

```rust
/// Human-meaningful identifier for a CatalogProvider instance. Opaque newtype;
/// `CatalogRegistry` (owned by `semstrait-manifest` per Q-CAT-012) keys on this.
pub struct CatalogId(String);

/// URI-shaped path accepted by every `FileSystem` method. Validated by the
/// concrete `FileSystem` impl at call boundary — MUST parse to one of:
///   - `file:///...` or a local absolute/relative path
///   - `s3://bucket/key`
///   - `gs://bucket/key`
///   - `abfss://container@account.dfs.core.windows.net/key`
/// Custom schemes ship via third-party `FileSystem` impls (per `30 §10`).
pub struct Path(String);

/// Fully-qualified table reference: `(catalog_id?, namespace, table)`.
#[derive(Debug, Clone)]
pub struct TableRef {
    pub catalog: Option<CatalogId>,
    pub namespace: NamespaceRef,
    pub name: String,
}

/// Ordered namespace path (`["sales", "prod"]` for `sales.prod`).
#[derive(Debug, Clone)]
pub struct NamespaceRef {
    pub parts: Vec<String>,
}

/// Returned by `CatalogProvider::resolve_table`. Captures enough metadata
/// for manifest compilation to build a `PhysicalSource` per `15 §3`.
#[non_exhaustive]
pub struct ResolvedTable {
    pub table: TableRef,
    pub location: Path,
    pub format: FileFormat,
    pub schema: Schema,
    pub partitions: Vec<Partition>,
    pub snapshot: SnapshotMetadata,
}

/// Entry returned by `FileSystem::list`. Size and modified-at are hints —
/// callers MUST NOT rely on cross-provider byte-precise agreement.
#[non_exhaustive]
pub struct FileEntry {
    pub path: Path,
    pub size: u64,
    pub modified_at: Option<std::time::SystemTime>,
}

/// One partition field in a table's partition spec. Matches the shape of
/// `15 §3.4`'s `PartitionColumn` but lives on the catalog side of the
/// boundary; `semstrait-manifest` converts between the two at compile time.
#[non_exhaustive]
pub struct Partition {
    pub position: usize,                  // 1-indexed per 15 §3.4
    pub name: String,
    pub source_column: String,            // column this transform applies to
    pub data_type: DataType,              // result type after transform
    pub transform: PartitionTransform,
}

#[non_exhaustive]
pub enum PartitionTransform {
    Identity,
    Year, Month, Day, Hour,
    Bucket(u32),
    Truncate(u32),
    Void,
}
```

`DataType` and `Diagnostic` are re-exported from `semstrait-core` per `31 §4` / `§7`; no duplicate definitions live here. `Schema` / `SchemaColumn` shapes follow `15 §3.2`; the fully-qualified field list is deferred pending `Q-CAT-008`.

---

## 3. `CatalogProvider` trait

### 3.1 Purpose

`CatalogProvider` abstracts one catalog *instance*. A single compiler invocation MAY inject multiple `CatalogProvider`s (e.g. `polaris_prod` + `polaris_dev` + `unity_main`), each keyed by its `id`. The trait is the sole interface through which manifest compilation acquires structured metadata (namespaces, tables, schemas, partitions, snapshots).

### 3.2 Trait definition

```rust
use async_trait::async_trait;

#[async_trait]
pub trait CatalogProvider: Send + Sync + std::fmt::Debug {
    fn id(&self) -> CatalogId;

    async fn list_tables(
        &self,
        namespace: &NamespaceRef,
    ) -> Result<Vec<TableRef>, CatalogError>;

    async fn resolve_table(
        &self,
        table: &TableRef,
    ) -> Result<ResolvedTable, CatalogError>;

    async fn get_schema(
        &self,
        table: &TableRef,
    ) -> Result<Schema, CatalogError>;

    async fn get_partitions(
        &self,
        table: &TableRef,
    ) -> Result<Vec<Partition>, CatalogError>;

    async fn get_snapshot(
        &self,
        table: &TableRef,
        version: SnapshotVersion,
    ) -> Result<SnapshotMetadata, CatalogError>;

    async fn check_schema_drift(
        &self,
        table: &TableRef,
        expected_schema: &Schema,
    ) -> Result<DriftReport, CatalogError>;
}
```

All methods are `async`. `Send + Sync` is required so `Arc<dyn CatalogProvider>` is safely shareable across the async runtime; `Debug` supports diagnostic logging without forcing `Display`.

### 3.3 Method contracts

- **`id`** — Returns a `CatalogId` by value (cheap clone of an interned string; see `§2.3`). Stable and human-meaningful (e.g. `CatalogId::from("polaris_prod")`). Used by `semstrait-manifest` to key its catalog registry and by diagnostics to cite the source of resolution. MUST NOT collide across providers within one registry — collision handling is a caller concern.

- **`list_tables`** — Returns every table directly under `namespace`. Nested namespaces are NOT traversed. Ordering is provider-defined but MUST be stable within a single snapshot of the catalog state; callers that need deterministic output (`00 §9 I4`) sort client-side. The method signature carries NO glob parameter — pattern-style listing is a client concern driven through `expand_glob` (`§7`) or via client-side filtering of a full `list_tables` result.

- **`resolve_table`** — Returns a `ResolvedTable` aggregating the table's `Path` location, format, schema, partition spec, and current snapshot in a single call. This is the canonical entry point for manifest compilation (`15 §5 Compile-Time Resolution`). Implementations MAY issue multiple network calls internally but MUST present the result as if captured at a single logical instant.

- **`get_schema`** — Returns the table's *current* schema. Called when only schema is needed (e.g. the caller already has location and partitions from a prior `resolve_table`). The returned `Schema` MUST be structurally identical to `resolve_table(table).await?.schema` for the same instant.

- **`get_partitions`** — Returns the list of `Partition` descriptors (position 1-indexed per `15 §3.4`, name, source column, result type, transform). Tables without partitioning return `Ok(vec![])`. The returned `Partition`s describe the **partition spec** of the table — not the set of concrete partition values or their row counts. Callers that need 15's `PartitionColumn` shape convert at the manifest boundary (`33`).

- **`get_snapshot`** — Returns `SnapshotMetadata` for a given version (or `SnapshotVersion::Current`). Snapshot pinning — the mechanism by which a compiled manifest captures a reproducible view of the catalog — goes through this call. See `15 §5.4` and `§9` below.

- **`check_schema_drift`** — The I11b-gated query-time entry point. Compares `expected_schema` (captured in the manifest at compile time) with the catalog's current schema and returns a `DriftReport`. Deterministic: same inputs → same output within a single catalog snapshot. See `§9` for the full contract.

### 3.4 Invariants

| Ref | Invariant |
|-----|-----------|
| I3  | No downstream crate branches on the concrete `CatalogProvider` type. Canonical crates see only `&dyn CatalogProvider`. |
| I11 | Compile-time async entries MAY be called during manifest compilation. The only query-time async entry is `check_schema_drift`. No other method is reachable during planning or execution. |
| I10 | `CatalogError`, `Partition`, `PartitionTransform`, `FileFormat`, `ResolvedTable`, `SnapshotVersion`, `SnapshotMetadata`, `DriftReport`, `DriftStatus`, `DriftKind`, `FileEntry` are `#[non_exhaustive]`. |
| I12 | Every `CatalogError` variant carries a stable `CAT_E_*` code. Error surface follows `30 §5`–`§6`. |

### 3.5 Non-goals

- No `create_table`, `drop_table`, `write_snapshot`, or any mutation. The v1 surface is strictly read-oriented. Mutation lands in a future MINOR under a separate companion trait (see `questions/open/37 Q-CAT-006`).
- No transaction scoping. Each call is independent.
- No caching inside the trait. Callers may wrap with a cache layer but the trait is not the cache surface.

---

## 4. Per-provider implementations (v1 roster)

### 4.1 `NoopCatalogProvider`

| Aspect                  | Detail                                                                 |
|-------------------------|------------------------------------------------------------------------|
| `id()`                  | User-supplied via `NoopCatalogProvider::new(id)`; defaults to `"noop"`.|
| Auth model              | None.                                                                  |
| Metadata source         | In-memory; always empty.                                               |
| Namespace support       | None (every `list_tables` returns empty).                              |
| Snapshot / partition support | None.                                                             |
| Use cases               | Unit tests, stateless compilation paths, integration harnesses that only exercise file-based bindings. |

Every method returns either `Ok(empty)` or `Err(CatalogError::TableNotFound)` (for `resolve_table` / `get_schema` / `get_partitions` / `get_snapshot` / `check_schema_drift`). This mirrors the legacy `NullCatalogProvider` behavior and preserves the "catalog-absent" graceful-degradation path from `docs/CATALOG_RESOLUTION.md §2`.

### 4.2 `IcebergRestCatalogProvider`

| Aspect                  | Detail                                                                 |
|-------------------------|------------------------------------------------------------------------|
| `id()`                  | User-supplied at construction.                                         |
| Auth model              | OAuth2 bearer token (Polaris default); configurable credential provider. |
| Metadata source         | Iceberg REST catalog API (`GET /v1/namespaces`, `GET /v1/namespaces/{ns}/tables`, `GET /v1/namespaces/{ns}/tables/{table}`). |
| Namespace support       | Multi-level (dotted); `NamespaceRef` round-trips with Iceberg's `namespace` list form. |
| Snapshot / partition support | Full — snapshot IDs, partition specs (with transforms: `identity`, `year`, `month`, `day`, `hour`, `bucket[N]`, `truncate[N]`, `void`), format version, location, properties. |
| Supported formats       | Parquet (primary), ORC, Avro (as reported by catalog metadata).        |
| Use cases               | Production deployments with Apache Polaris, Tabular, Iceberg REST Catalog service. |

Partition transform resolution and field-id-to-name mapping follow `docs/CATALOG_RESOLUTION.md §4`. Snapshot pinning captures `metadata.current-snapshot-id`.

Limitations: REST API only — no direct metadata-file reads. Table data files are NOT listed by this provider (that is a `FileSystem` concern via the `location` returned in `ResolvedTable`).

### 4.3 `UnityCatalogProvider`

| Aspect                  | Detail                                                                 |
|-------------------------|------------------------------------------------------------------------|
| `id()`                  | User-supplied at construction.                                         |
| Auth model              | OAuth2 personal-access-token or Databricks-issued bearer.              |
| Metadata source         | Unity Catalog REST API (`GET /api/2.1/unity-catalog/tables`, `GET /api/2.1/unity-catalog/schemas`). |
| Namespace support       | Three-level: `catalog.schema.table`. `NamespaceRef` with exactly two segments (`catalog`, `schema`). |
| Snapshot / partition support | Partition columns only (Unity does not expose snapshot IDs in v1 of this provider). `get_snapshot` returns `SnapshotMetadata` with `version: SnapshotVersion::Current` and no pin id. |
| Supported formats       | Parquet, Delta (reported as format).                                   |
| Use cases               | Databricks-hosted data; platform-agnostic Unity Catalog deployments.   |

Limitations: no snapshot pinning (versioning on Delta tables lives in the Delta log, not the Unity API). `check_schema_drift` still functions — it compares current schema to expected — but without snapshot-ID correlation. See `questions/open/37 Q-CAT-003`.

### 4.4 `FilesystemCatalogProvider`

| Aspect                  | Detail                                                                 |
|-------------------------|------------------------------------------------------------------------|
| `id()`                  | User-supplied at construction.                                         |
| Auth model              | Delegated entirely to the composed `FileSystem`.                       |
| Metadata source         | File listings under a configured root; tables are *directories*, namespaces are *parent directories*. |
| Namespace support       | Arbitrary depth, derived from directory nesting.                       |
| Snapshot / partition support | Partition columns inferred from Hive-style `key=value` subdirectories (per `15 §6.5`); no snapshot IDs. |
| Supported formats       | Parquet, CSV, JSON, ORC, Avro — inferred from file extension.          |
| Composition             | Constructed with a `Arc<dyn FileSystem>`; all metadata calls ultimately issue `FileSystem::list` / `exists`. |
| Use cases               | Data lakes without a catalog service; lake-house pre-adoption; dev/test. |

This provider is the canonical bridge: it shows that `FileSystem` alone is sufficient to stand up a minimal catalog-independent compile path, and it demonstrates the composition pattern (inject a `FileSystem` into a `CatalogProvider`) that third-party implementations may follow.

Limitations: no schema reading. `get_schema` and the `schema` field of `ResolvedTable` return `Ok(Schema::empty())` — the manifest author is expected to supply the schema explicitly or accept downstream validation against observed columns. (The legacy `StorageProvider::read_schema` path, which parsed Parquet footers, is **removed** in this design — see `§12`.)

### 4.5 Roster summary

| Provider                    | Snapshot pinning | Partition metadata | Schema source | Namespace depth |
|-----------------------------|------------------|--------------------|---------------|-----------------|
| `NoopCatalogProvider`       | —                | —                  | —             | flat (empty)    |
| `IcebergRestCatalogProvider`| yes              | full (all transforms) | API       | arbitrary       |
| `UnityCatalogProvider`      | current only     | columns only       | API           | exactly 2       |
| `FilesystemCatalogProvider` | —                | Hive-style         | manifest-declared | arbitrary |

---

## 5. `FileSystem` trait

### 5.1 Purpose

`FileSystem` abstracts byte-level I/O over a remote or local blob/object store. It is **strictly** format-agnostic and schema-unaware: it moves bytes and lists entries. Format interpretation (Parquet, CSV, JSON, …) is an adapter concern; schema knowledge is a `CatalogProvider` concern.

### 5.2 Trait definition

```rust
use async_trait::async_trait;
use bytes::Bytes;

#[async_trait]
pub trait FileSystem: Send + Sync + std::fmt::Debug {
    async fn list(
        &self,
        prefix: &Path,
    ) -> Result<Vec<FileEntry>, FileSystemError>;

    async fn read(
        &self,
        path: &Path,
    ) -> Result<Bytes, FileSystemError>;

    async fn write(
        &self,
        path: &Path,
        data: Bytes,
    ) -> Result<(), FileSystemError>;

    async fn exists(
        &self,
        path: &Path,
    ) -> Result<bool, FileSystemError>;
}
```

`Path` is the URI-shaped newtype defined in `§2.3` (e.g. `s3://bucket/key`, `file:///abs/path`, `gs://bucket/key`, `abfss://container@account.dfs.core.windows.net/path`). Each concrete implementation validates scheme at call boundary and rejects non-matching URIs with `FileSystemError::UnsupportedScheme` (`§8.2`). Accepting a typed `&Path` rather than a raw `&str` keeps accidental string-path confusion out of the trait surface and gives `FileSystem` impls a stable value to pattern-match.

### 5.3 Method contracts

- **`list`** — Returns every object whose key begins with `prefix`. Non-recursive semantics are NOT implied; implementations return all descendants. Results are NOT sorted — callers sort client-side when determinism is required.
- **`read`** — Returns the full object body as `Bytes`. Streaming reads are not in v1; large-object streaming is a MINOR extension (see `questions/open/37 Q-CAT-005`).
- **`write`** — Writes `data` as a single object at `path`. Overwrites by default. Atomicity is provider-defined — typically atomic on S3/Azure/GCS, non-atomic on local disk unless the implementation stages to a temp file.
- **`exists`** — Returns `true` iff an object exists at exactly `path`. Does NOT test prefix existence.

### 5.4 Invariants

| Ref | Invariant |
|-----|-----------|
| I11 | `FileSystem` is compile-time and artifact-output async. Adapter-hot-path I/O goes through engine-native reads (not this trait). |
| I10 | `FileSystemError`, `FileEntry` are `#[non_exhaustive]`. |
| I12 | Every `FileSystemError` variant carries a stable `FS_E_*` code. |
| —   | No format-aware logic. `read` returns bytes; interpretation is not this trait's responsibility. |
| —   | No schema-aware logic. `FileSystem` MUST NOT parse Parquet footers, CSV headers, JSON objects, etc. |

### 5.5 Non-goals

- No range reads, no multipart uploads, no copy/move/rename as first-class operations in v1. (Move = read + write + delete; delete itself is also deferred — see `Q-CAT-005`.)
- No credential management surface on the trait. Credentials are internal configuration of the concrete implementation.
- No caching. Callers wrap if needed.

---

## 6. Per-filesystem implementations (v1 roster)

### 6.1 `LocalFileSystem`

| Aspect             | Detail                                                                 |
|--------------------|------------------------------------------------------------------------|
| Accepted schemes   | `file://`, and relative/absolute unadorned paths.                      |
| Auth model         | OS filesystem permissions.                                             |
| Use cases          | Tests, local development, one-box deployments.                         |

### 6.2 `S3FileSystem`

| Aspect             | Detail                                                                 |
|--------------------|------------------------------------------------------------------------|
| Accepted schemes   | `s3://`                                                                |
| Auth model         | AWS credential chain (env / profile / instance role / IMDS).           |
| Endpoint           | Configurable (AWS, R2, MinIO, Ceph-S3).                                |
| Consistency        | Read-after-write consistent per AWS S3 strong-consistency guarantees. |

### 6.3 `AzureFileSystem`

| Aspect             | Detail                                                                 |
|--------------------|------------------------------------------------------------------------|
| Accepted schemes   | `abfss://`, `abfs://`                                                  |
| Auth model         | Azure AD / managed identity / SAS token / account key.                 |
| Use cases          | Azure Data Lake Storage Gen2 (hierarchical namespace supported).       |

### 6.4 `GcsFileSystem`

| Aspect             | Detail                                                                 |
|--------------------|------------------------------------------------------------------------|
| Accepted schemes   | `gs://`                                                                |
| Auth model         | Application default credentials / service-account JSON key.            |

### 6.5 Roster summary

Implementation selection is caller-side: the caller constructs the concrete `FileSystem` (or a `Vec<Arc<dyn FileSystem>>` dispatched by scheme) and injects it into the `CatalogProvider` or into `expand_glob`. No scheme-dispatch facility is exposed by the crate root in v1; if scheme-based dispatch is needed across multiple filesystems, callers compose it themselves. See `questions/open/37 Q-CAT-004`.

---

## 7. Glob expansion

### 7.1 Shared utility

```rust
pub async fn expand_glob(
    fs: &dyn FileSystem,
    pattern: &GlobPattern,
) -> Result<Vec<Path>, FileSystemError>;
```

`GlobPattern` is re-exported from `semstrait-core` (`31 §14.4`-adjacent per `Q-CAT-002`). `expand_glob` is the single public glob-expansion entry point. It:

1. Parses `pattern` into a fixed prefix (everything up to the first glob metacharacter) and a suffix pattern.
2. Wraps the prefix in a `Path` and calls `fs.list(&prefix)` to enumerate candidates.
3. Filters candidate `FileEntry.path` values against the suffix using glob semantics defined in `31` (via `semstrait_core::glob_match` or equivalent — see `questions/open/37 Q-CAT-002`).
4. Returns the lexicographically-sorted list of matching `Path`s.

Globbing supports `*`, `**`, `?`, and character classes `[abc]` as defined by `semstrait-core`'s glob module. `**` matches any number of path segments; `*` matches within a single segment.

### 7.2 Composition with `CatalogProvider`

`FilesystemCatalogProvider` internally uses `expand_glob` when resolving a table reference of the form `root/namespace/table_glob`. Other `CatalogProvider`s do NOT use `expand_glob` — they expand glob patterns through their native catalog APIs (e.g. `GET /v1/namespaces/{ns}/tables` followed by client-side filtering).

SemanticManifest compilation also calls `expand_glob` directly for glob-bound `Source::Path { glob: ... }` bindings (`15 §6.3`), bypassing the catalog.

### 7.3 Determinism

`expand_glob` MUST return results sorted lexicographically. This aligns with `00 §9 I4` (manifest determinism) — a compiled manifest that includes glob-expanded paths must be byte-identical across compiler runs against the same filesystem state.

---

## 8. `CatalogError` / `FileSystemError`

### 8.1 `CatalogError`

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CatalogError {
    #[error("{CAT_E_0100}: catalog not available: {msg}")]
    NotAvailable { msg: String, diagnostic: Diagnostic },

    #[error("{CAT_E_0101}: table not found: {fqn}")]
    TableNotFound { fqn: String, diagnostic: Diagnostic },

    #[error("{CAT_E_0102}: namespace not found: {ns}")]
    NamespaceNotFound { ns: String, diagnostic: Diagnostic },

    #[error("{CAT_E_0103}: snapshot not found: {table} version={version}")]
    SnapshotNotFound { table: String, version: String, diagnostic: Diagnostic },

    #[error("{CAT_E_0200}: connection failed: {msg}")]
    ConnectionFailed { msg: String, diagnostic: Diagnostic },

    #[error("{CAT_E_0201}: authentication failed")]
    AuthFailed { diagnostic: Diagnostic },

    #[error("{CAT_E_0202}: authorization denied: {resource}")]
    AuthDenied { resource: String, diagnostic: Diagnostic },

    #[error("{CAT_E_0203}: request timed out after {millis}ms")]
    Timeout { millis: u64, diagnostic: Diagnostic },

    #[error("{CAT_E_0300}: schema drift: {kind:?}")]
    SchemaDrift { kind: DriftKind, diagnostic: Diagnostic },

    #[error("{CAT_E_0301}: partition metadata malformed: {msg}")]
    MalformedPartition { msg: String, diagnostic: Diagnostic },

    #[error("{CAT_E_0302}: catalog response violated contract: {msg}")]
    MalformedResponse { msg: String, diagnostic: Diagnostic },

    #[error("{CAT_E_0399}: provider internal error: {msg}")]
    Internal { msg: String, diagnostic: Diagnostic },
}
```

Each variant carries a `Diagnostic` (per `30 §5`) so callers may route catalog errors through the same reporting pipeline as other subsystems.

**Proposed `CAT_E_*` range: `0100`–`0399`.** Three sub-ranges:

- `0100`–`0199` — availability / resource-presence errors (not-available, not-found).
- `0200`–`0299` — transport / auth errors (connection, auth, timeout).
- `0300`–`0399` — protocol / contract errors (drift, malformed responses, internal).

### 8.2 `FileSystemError`

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FileSystemError {
    #[error("{FS_E_0100}: object not found: {path}")]
    NotFound { path: Path, diagnostic: Diagnostic },

    #[error("{FS_E_0101}: unsupported URI scheme: {scheme}")]
    UnsupportedScheme { scheme: String, diagnostic: Diagnostic },

    #[error("{FS_E_0102}: invalid path: {path}: {msg}")]
    InvalidPath { path: Path, msg: String, diagnostic: Diagnostic },

    #[error("{FS_E_0103}: invalid glob pattern: {pattern}: {msg}")]
    InvalidGlob { pattern: String, msg: String, diagnostic: Diagnostic },

    #[error("{FS_E_0110}: connection failed: {msg}")]
    ConnectionFailed { msg: String, diagnostic: Diagnostic },

    #[error("{FS_E_0111}: authentication failed")]
    AuthFailed { diagnostic: Diagnostic },

    #[error("{FS_E_0112}: permission denied: {path}")]
    PermissionDenied { path: Path, diagnostic: Diagnostic },

    #[error("{FS_E_0113}: request timed out after {millis}ms")]
    Timeout { millis: u64, diagnostic: Diagnostic },

    #[error("{FS_E_0199}: filesystem internal error: {msg}")]
    Internal { msg: String, diagnostic: Diagnostic },
}
```

**Proposed `FS_E_*` range: `0100`–`0199`.** Sub-ranges:

- `0100`–`0109` — input / resource-presence errors.
- `0110`–`0198` — transport / auth / timeout.
- `0199` — catch-all internal.

### 8.3 Registration with `30 §6.2`

`30 §6.2`'s reserved-ranges table currently lists `PARSE`, `VALID`, `COMP`, `EXPR`, `PLAN`, `OPT`, `ADAPT`, and `REG / IO / ENG` as reserved. `IR` has an open item against `30` (see `questions/open/35 Q-IR-001`). Neither `CAT` nor `FS` is present today.

This doc proposes adding two new rows under `30 §6.2`:

| Subsystem | Prefix | Range         | Authoritative doc |
|-----------|--------|---------------|-------------------|
| Catalog   | `CAT`  | `0100`–`0399` | `37_semstrait_catalog` |
| FileSystem| `FS`   | `0100`–`0199` | `37_semstrait_catalog` |

Tracked as amendment item `[TD-CAT-CODE-TABLE-AMEND]` pending `30`'s next amendment pass. See `questions/open/37 Q-CAT-001`.

---

## 9. Schema-drift gated I/O (I11b)

### 9.1 Context

Per `00 §9 I11` and `30 §9`, `semstrait-catalog` is **compile-time async**. The one exception is the I11b schema-drift gate: a single `async fn` call permitted immediately before query execution begins, used to confirm that the physical schema captured at compile time still matches reality.

This gate is invoked by `semstrait-manifest` (not by `semstrait-planner`, not by `semstrait-ir`, not by any adapter). The planner and the adapters remain fully synchronous and format-aware-only.

### 9.2 Signature

```rust
async fn check_schema_drift(
    &self,
    table: &TableRef,
    expected_schema: &Schema,
) -> Result<DriftReport, CatalogError>;
```

Where:

```rust
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct DriftReport {
    pub table: TableRef,
    pub status: DriftStatus,
    pub details: Vec<DriftKind>,
    pub checked_at: std::time::SystemTime,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftStatus {
    Unchanged,
    Compatible,
    Breaking,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum DriftKind {
    ColumnAdded { name: String },
    ColumnRemoved { name: String },
    ColumnRetyped { name: String, expected: DataType, actual: DataType },
    ColumnRenamed { expected: String, actual: String },
    NullabilityTightened { name: String },
    NullabilityRelaxed { name: String },
    PartitionChanged { details: String },
}
```

### 9.3 Deterministic contract

- **Idempotent per snapshot.** Two calls with identical `(table, expected_schema)` against the same underlying catalog snapshot MUST return identical `DriftReport` values except for `checked_at`. `status` and `details` (excluding ordering of additive fields) are deterministic.
- **Pure over inputs.** The method performs catalog I/O but MUST NOT mutate external state.
- **No planner involvement.** `check_schema_drift` does NOT touch `semstrait-ir`, does NOT accept or return `SemanticPlan` / `PhysicalPlan`, and does NOT take a diagnostic sink. Failures surface as `CatalogError`.

### 9.4 Caller policy

The caller (`semstrait-manifest`'s `SemanticManifest::verify_against_catalog` or equivalent) maps `DriftReport` to one of:

| `DriftStatus` | Caller response                                                                 |
|---------------|---------------------------------------------------------------------------------|
| `Unchanged`   | Proceed with execution using the compiled plan as-is.                           |
| `Compatible`  | Proceed; optionally emit an advisory diagnostic (`Severity::Info`).             |
| `Breaking`    | Abort execution; return `CatalogError::SchemaDrift` with the `DriftReport` embedded. |

The caller MAY skip the gate entirely (e.g. dev/test mode) — this is a *caller* policy, not a trait requirement. The trait only promises to answer the question when asked.

### 9.5 I11b boundary

`check_schema_drift` is the ONLY `CatalogProvider` method that a well-behaved query-time path invokes. All other methods (`resolve_table`, `get_schema`, `get_partitions`, `get_snapshot`, `list_tables`) are compile-time only. A runtime path that invokes any non-drift method is a layering violation and SHOULD be caught in review.

---

## 10. Caller responsibilities

### 10.1 Construction

Catalog and filesystem providers are constructed at *program startup* (or at tool-invocation time for a CLI), not lazily inside the compile/plan pipeline. Typical wiring:

```rust
let fs: Arc<dyn FileSystem> = Arc::new(S3FileSystem::new(s3_config)?);
let catalog: Arc<dyn CatalogProvider> = Arc::new(
    IcebergRestCatalogProvider::builder()
        .id("polaris_prod")
        .base_url("https://polaris.example.com")
        .auth(token)
        .build()?,
);

let registry = CatalogRegistry::new()
    .with_catalog("polaris_prod", catalog.clone())
    .with_filesystem("s3", fs.clone());

// Compile-time:
let manifest = semstrait_manifest::compile(model, &registry).await?;
```

`CatalogRegistry` is owned by `semstrait-manifest` (see `33`), not by this crate. `semstrait-catalog` exposes only the *pieces* from which a registry is built.

### 10.2 Dependency injection pattern

Every consumer takes `&dyn CatalogProvider` / `&dyn FileSystem` (or `Arc<dyn …>`), never a concrete type. This preserves I3 and lets tests substitute `NoopCatalogProvider` or an in-memory `FileSystem` without conditional compilation.

### 10.3 Thread safety

`CatalogProvider: Send + Sync` and `FileSystem: Send + Sync` are required by the trait bounds. Implementations MUST be safe to share across async tasks behind an `Arc`. Internal mutability (e.g. connection-pool state) MUST use interior-mutable synchronization (`Mutex`, `RwLock`, `tokio::sync::*`, atomic); exterior `&self` on every method is a hard requirement.

### 10.4 Error propagation

Callers propagate `CatalogError` and `FileSystemError` through `?` into their own error types (typically `SemanticManifestCompileError` at compile time, `ExecutionError` at I11b). Stable codes (`CAT_E_*`, `FS_E_*`) flow through the outer `Diagnostic` stream (`30 §5`).

### 10.5 Async runtime

The trait is `async fn` via `async_trait::async_trait` in v1. Callers MUST drive futures on a `tokio`-compatible runtime (per `30 §9`). A move to native `async fn` in traits (stable as of Rust 1.75+) is a possible future simplification — see `questions/open/37 Q-CAT-007`.

---

## 11. Stability

### 11.1 Trait stability posture

Both `CatalogProvider` and `FileSystem` are **open** traits per `30 §10`:

- Third-party crates MAY implement them to plug new catalog services or storage backends.
- Method-set growth is permitted only in MAJOR releases, OR in MINOR via a default-implemented method (which does NOT break existing impls).
- Method signature changes are MAJOR-only.

To avoid the "default method lie" — where a default silently returns `None` or an empty value and a caller mistakes absence-of-support for absence-of-data — MINOR-added methods MUST have defaults that either:

1. Return an error with a dedicated `CAT_E_*` / `FS_E_*` code (e.g. `MethodNotSupported`), OR
2. Explicitly document that the default response is semantically meaningful (e.g. "returns empty when the method is genuinely not applicable, not when the provider simply lacks support").

### 11.2 Built-in implementation stability

`NoopCatalogProvider`, `IcebergRestCatalogProvider`, `UnityCatalogProvider`, `FilesystemCatalogProvider`, `LocalFileSystem`, `S3FileSystem`, `AzureFileSystem`, `GcsFileSystem` are **Stable** in v1: their struct identities, their `new`/`builder` constructors, and their observable behavior do not break in MINOR. Field additions on their configuration types (e.g. `S3Config`) are permitted additively.

### 11.3 Error-variant stability

`CatalogError` and `FileSystemError` are `#[non_exhaustive]`. MINOR releases MAY add variants with new `CAT_E_*` / `FS_E_*` codes. Existing variants MUST NOT change shape, and existing codes MUST NOT be reused or retired within a MAJOR cycle (retirement policy follows `30 §6.7` once `questions/open/30 Q-API-006` resolves).

### 11.4 Value-type stability

`CatalogId`, `Path`, `TableRef`, `NamespaceRef`, `Schema`, `SchemaColumn` are field-stable (not `#[non_exhaustive]`) — these are the shared vocabulary between this crate and its callers, and shielding them behind non-exhaustive marking would require builder boilerplate that outweighs the benefit. `ResolvedTable`, `Partition`, `PartitionTransform`, `FileFormat`, `SnapshotVersion`, `SnapshotMetadata`, `DriftReport`, `DriftStatus`, `DriftKind`, `FileEntry` are `#[non_exhaustive]` — these are more likely to grow as catalog ecosystems expose new metadata shapes.

### 11.5 Method-set growth mechanism

Method-set growth on `CatalogProvider` or `FileSystem` follows this ordered procedure:

1. Propose in an `questions/open/37_*.md` entry citing the need.
2. Draft signature + default behavior.
3. Ratify in a MINOR release with a default impl that errors on absence-of-support.
4. Built-in impls override the default in the same MINOR; third-party impls migrate at their own pace.

This procedure is the MINOR-safe path; any growth that cannot be fit into it becomes a MAJOR candidate.

---

## 12. Crate boundaries

| Boundary                                     | Status                                                                 |
|----------------------------------------------|------------------------------------------------------------------------|
| Planning (`SemanticPlan`, `PhysicalPlan`)    | **NO.** `semstrait-catalog` has zero `semstrait-ir` / `semstrait-planner` imports. |
| SQL emission / engine-specific string gen    | **NO.** Every string produced is a URI, an identifier, or a diagnostic message — never an SQL fragment. |
| Format-header schema reading                 | **NO.** `FileSystem::read` returns raw `Bytes`. The legacy `StorageProvider::read_schema(path, DataFormat)` path is removed. Schemas come from `CatalogProvider` or from the manifest author. |
| Expression evaluation                        | **NO.** No `Expr`, no `ExprBlock`, no evaluator. |
| Caching policy                               | **NO.** Callers wrap. |
| Retry / backoff machinery                    | **NO.** Individual implementations MAY retry internally as a quality-of-implementation detail, but the trait surface exposes no retry knobs. |
| Transaction scoping                          | **NO.** Each call is independent. |
| Mutation of catalog state                    | **NO in v1.** Read-only trait surface. Future: see `Q-CAT-006`. |
| Credential management                        | **NO public surface.** Credentials are internal config of each concrete impl. |
| Scheme-dispatch across FileSystems           | **NO.** Callers compose. See `Q-CAT-004`. |
| Format enumeration (`FileFormat`)            | **YES.** Owned here because `FileFormat` is a metadata attribute of resolved tables and files. Adapters and the manifest consume it as opaque metadata. |
| Partition-transform enumeration (`PartitionTransform`) | **YES (v1 limited).** `Identity`, `Year`, `Month`, `Day`, `Hour`, `Bucket(u32)`, `Truncate(u32)`, `Void` (per `§2.3`). Matches Iceberg REST v2 spec. New transforms land in MINOR via `#[non_exhaustive]`. See `Q-CAT-010`. |
| Glob semantics                               | **Delegated.** `expand_glob` uses `semstrait-core`'s glob predicate; this crate owns only the `FileSystem::list`-driven prefix-then-filter dance. |

---

## 13. Round-1 open items

The following drafting decisions are **defaulted** in this document but MUST be confirmed before ratification. All are captured in `docs/design/questions/open/37_questions.md`:

- **Q-CAT-001** — Register `CAT` and `FS` subsystem prefixes in `30 §6.2`, with ranges `CAT_E_0100`–`0399` and `FS_E_0100`–`0199`.
- **Q-CAT-002** — Ownership of glob-matching semantics: `semstrait-core` vs `semstrait-catalog`. Current default: core owns the predicate; catalog owns the prefix-and-filter orchestration.
- **Q-CAT-003** — Snapshot-pinning contract for catalogs that do not expose snapshot IDs (Unity Catalog). Current default: `SnapshotMetadata` uses `SnapshotVersion::Current` without a pin ID; `check_schema_drift` still runs but cannot be snapshot-correlated.
- **Q-CAT-004** — Should a scheme-dispatching `FileSystem` (e.g. `DispatchingFileSystem`) ship in v1 or stay caller-composed? Current default: caller-composed.
- **Q-CAT-005** — Streaming reads (`read_stream`) and deletion (`delete`) on `FileSystem`: v1 omission or v1 inclusion with default-error impls? Current default: omitted from v1.
- **Q-CAT-006** — Catalog mutation surface (`create_table`, `commit_snapshot`): separate companion trait in a future MINOR, or attached to `CatalogProvider` via default-error methods. Current default: separate companion trait.
- **Q-CAT-007** — `async fn` in traits: keep `async_trait` macro in v1 for object-safety ergonomics, or move to native `async fn` + `trait_variant::make`? Current default: `async_trait` in v1.
- **Q-CAT-008** — `Schema` / `SchemaColumn` ownership: define here (provider returns catalog-local types; manifest converts) or in `semstrait-core` (shared vocabulary). Current default: define here, since the types are catalog-shaped first.
- **Q-CAT-009** — `expand_glob` return type: `Vec<Path>` (current) vs `Vec<FileEntry>`. The former is lighter; the latter preserves size/modified-at for caller determinism audits.
- **Q-CAT-010** — Partition-transform enumeration: mirror Iceberg v2 exactly, or expose a portable subset plus a `PartitionTransform::Other(String)` fallback. Current default: mirror Iceberg exactly, `#[non_exhaustive]`.
- **Q-CAT-011** — `FilesystemCatalogProvider` schema source: empty schema + manifest-declared (current default) vs user-supplied schema-callback plug-in.
- **Q-CAT-012** — `CatalogRegistry` ownership: `semstrait-manifest` (current default) vs `semstrait-catalog` (legacy code location).

Each item is parked with arguments-for, arguments-against, and a next-step in `questions/open/37`.

---

## 14. Cross-references

- Overview: `00 §4 The Public Surface`, `00 §5 Layer 3 — Runtime Integration`.
- Invariants: `00 §9 I3, I10, I11 (incl. I11b), I12`.
- API contracts: `30 §3 (Open/sealed traits)`, `30 §5–§6 (Diagnostic + error codes)`, `30 §8 (Stability)`, `30 §9 (Async posture)`, `30 §10 (Per-crate async table)`.
- Compile-time consumers: `15 §5 (Compile-Time Resolution)`, `15 §6 (Source resolution: paths, tables, globs)`.
- Query-time consumers: `33 (semstrait-manifest, I11b gate)` — drafted adjacent to this doc.
- Sibling crate: `31 (semstrait-core)` — shared primitives (`ColumnName`, `DataType`, `Span`, `Diagnostic`, `GlobPattern`).
- Downstream: `35 (semstrait-ir)`, `34 (semstrait-planner)`, `36 (semstrait-adapter)` — do NOT import from this crate.

---

## 15. Round-1 ratifications

- §2.1 roster of public items and their stability tier.
- §3.2 `CatalogProvider` method set and signatures.
- §5.2 `FileSystem` method set and signatures.
- §4 four-provider roster and capability matrix (§4.5).
- §6 four-filesystem roster.
- §7.1 `expand_glob` signature and contract.
- §8.1–§8.2 `CatalogError` / `FileSystemError` variant set, carrying `Diagnostic`, under proposed `CAT_E_0100`–`0399` and `FS_E_0100`–`0199` ranges.
- §9 I11b schema-drift gate as the sole query-time `CatalogProvider` entry point.
- §10 caller-wiring pattern and thread-safety requirements.
- §11 stability posture: open traits, stable built-ins, non-exhaustive error/metadata value-types.
- §12 crate-boundary negatives: no planning, no SQL, no format-header parsing.

Numeric error code values in §8 are placeholders pending `30`'s amendment pass (`Q-CAT-001`) — the shape, variant names, severity, and sub-range assignment are ratified; only the literal digit offsets may change during reconciliation with `30 §6.2`.
