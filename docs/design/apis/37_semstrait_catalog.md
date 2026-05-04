---
doc: design/apis/37_semstrait_catalog
status: Round-1 Draft
prereqs:
  - 15
authoritative-for:
  - "`CatalogProvider` trait surface (method set, signatures, async posture)"
  - "`FileSystem` trait surface (method set, signatures, async posture)"
  - "`CatalogProviderErrorKind` typed-error enum (variant identity per `30 §5`; `CAT_E_*` codes retired) and its `Diagnose` impl per `31 §3`"
  - "`FileSystemErrorKind` typed-error enum (variant identity per `30 §5`; `FS_E_*` codes retired) and its `Diagnose` impl per `31 §3`"
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
- Error types (`CatalogProviderErrorKind`, `FileSystemErrorKind`) — typed-kind enums per `30 §5` / `31 §3`; identification by variant identity. Legacy `CAT_E_*` / `FS_E_*` codes are retired.
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
| `CatalogProviderErrorKind`      | enum           | yes                  | Provisional    |
| `FileSystemErrorKind`           | enum           | yes                  | Provisional    |
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
pub mod error;              // CatalogProviderErrorKind, FileSystemErrorKind
pub mod glob;               // expand_glob
pub mod providers;          // NoopCatalogProvider, IcebergRestCatalogProvider, ...
pub mod filesystems;        // LocalFileSystem, S3FileSystem, ...

pub use traits::{CatalogProvider, FileSystem};
pub use types::*;
pub use error::{CatalogProviderErrorKind, FileSystemErrorKind};
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

`DataType` and `Diagnostic<K>` / `Diagnose` are re-exported from `semstrait-core` per `31 §3` / `§4`; no duplicate definitions live here. `Schema` / `SchemaColumn` shapes follow `15 §3.2`; the fully-qualified field list is deferred pending `Q-CAT-008`.

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
    ) -> Result<Vec<TableRef>, CatalogProviderErrorKind>;

    async fn resolve_table(
        &self,
        table: &TableRef,
    ) -> Result<ResolvedTable, CatalogProviderErrorKind>;

    async fn get_schema(
        &self,
        table: &TableRef,
    ) -> Result<Schema, CatalogProviderErrorKind>;

    async fn get_partitions(
        &self,
        table: &TableRef,
    ) -> Result<Vec<Partition>, CatalogProviderErrorKind>;

    async fn get_snapshot(
        &self,
        table: &TableRef,
        version: SnapshotVersion,
    ) -> Result<SnapshotMetadata, CatalogProviderErrorKind>;

    async fn check_schema_drift(
        &self,
        table: &TableRef,
        expected_schema: &Schema,
    ) -> Result<DriftReport, CatalogProviderErrorKind>;
}
```

Each method returns a **bare** `CatalogProviderErrorKind` per `31 §3.1`'s
construction-site convention — `CatalogProvider` is a transport trait, not a
stage entry-point. Consumers (notably `compile` in `33`) wrap into the
caller-side typed-kind via `From<CatalogProviderErrorKind>` for their own
kind enum, attaching a `Location` at the wrapping point if relevant.

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
| I10 | `CatalogProviderErrorKind`, `Partition`, `PartitionTransform`, `FileFormat`, `ResolvedTable`, `SnapshotVersion`, `SnapshotMetadata`, `DriftReport`, `DriftStatus`, `DriftKind`, `FileEntry` are `#[non_exhaustive]`. |
| I12 | `CatalogProviderErrorKind` is identified by **variant identity** per `30 §5`; numeric `CAT_E_*` codes are retired alongside the workspace-wide stable-code retirement. Severity is conveyed via `Diagnose::severity()` per `31 §3`. |

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

Every method returns either `Ok(empty)` or `Err(CatalogProviderErrorKind::TableNotFound { .. })` (for `resolve_table` / `get_schema` / `get_partitions` / `get_snapshot` / `check_schema_drift`). This mirrors the legacy `NullCatalogProvider` behavior and preserves the "catalog-absent" graceful-degradation path from `docs/CATALOG_RESOLUTION.md §2`.

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
    ) -> Result<Vec<FileEntry>, FileSystemErrorKind>;

    async fn read(
        &self,
        path: &Path,
    ) -> Result<Bytes, FileSystemErrorKind>;

    async fn write(
        &self,
        path: &Path,
        data: Bytes,
    ) -> Result<(), FileSystemErrorKind>;

    async fn exists(
        &self,
        path: &Path,
    ) -> Result<bool, FileSystemErrorKind>;
}
```

`Path` is the URI-shaped newtype defined in `§2.3` (e.g. `s3://bucket/key`, `file:///abs/path`, `gs://bucket/key`, `abfss://container@account.dfs.core.windows.net/path`). Each concrete implementation validates scheme at call boundary and rejects non-matching URIs with `FileSystemErrorKind::UnsupportedScheme` (`§8.2`). Accepting a typed `&Path` rather than a raw `&str` keeps accidental string-path confusion out of the trait surface and gives `FileSystem` impls a stable value to pattern-match.

Each method returns a **bare** `FileSystemErrorKind` per `31 §3.1`'s
construction-site convention — `FileSystem` is a transport trait. Consumers
wrap into their own typed-kind via `From<FileSystemErrorKind>` impls (e.g.
the `expand_glob` helper in `§7.1` returns `FileSystemErrorKind` directly;
`33 §16.5` consumers wrap into `CompileErrorKind`).

### 5.3 Method contracts

- **`list`** — Returns every object whose key begins with `prefix`. Non-recursive semantics are NOT implied; implementations return all descendants. Results are NOT sorted — callers sort client-side when determinism is required.
- **`read`** — Returns the full object body as `Bytes`. Streaming reads are not in v1; large-object streaming is a MINOR extension (see `questions/open/37 Q-CAT-005`).
- **`write`** — Writes `data` as a single object at `path`. Overwrites by default. Atomicity is provider-defined — typically atomic on S3/Azure/GCS, non-atomic on local disk unless the implementation stages to a temp file.
- **`exists`** — Returns `true` iff an object exists at exactly `path`. Does NOT test prefix existence.

### 5.4 Invariants

| Ref | Invariant |
|-----|-----------|
| I11 | `FileSystem` is compile-time and artifact-output async. Adapter-hot-path I/O goes through engine-native reads (not this trait). |
| I10 | `FileSystemErrorKind`, `FileEntry` are `#[non_exhaustive]`. |
| I12 | `FileSystemErrorKind` is identified by **variant identity** per `30 §5`; numeric `FS_E_*` codes are retired alongside the workspace-wide stable-code retirement. Severity is conveyed via `Diagnose::severity()` per `31 §3`. |
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
) -> Result<Vec<Path>, FileSystemErrorKind>;
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

## 8. `CatalogProviderErrorKind` / `FileSystemErrorKind`

> **Migration note.** Prior drafts of this document used `CatalogError` and
> `FileSystemError` enums with embedded `diagnostic: Diagnostic` fields and
> stable `CAT_E_*` / `FS_E_*` numeric codes. Those shapes are **retired**
> per `30 §5` and replaced by typed-kind enums (`*ErrorKind`) per `31 §3`:
> identification is by variant identity, severity comes from
> `Diagnose::severity()`, and source location is carried in the wrapping
> `Diagnostic<K>` envelope when the consumer wraps the bare kind. Body
> prose may still cite legacy `CAT_E_NNNN` / `FS_E_NNNN` strings as
> transitional anchors; read those as shorthand for the corresponding
> `CatalogProviderErrorKind::*` / `FileSystemErrorKind::*` variants.

### 8.1 `CatalogProviderErrorKind`

```rust
/// Typed error-kind for the `CatalogProvider` trait surface.
/// Identification by variant identity per `30 §5`; numeric `CAT_E_*`
/// codes are retired. Severity is conveyed via `Diagnose::severity()`;
/// every v1 variant is `Severity::Error` (no warning-severity catalog
/// kinds in v1). Source location (where applicable) lives in the
/// wrapping `Diagnostic<K>` envelope at the consumer's wrap site.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum CatalogProviderErrorKind {
    // -- Availability / resource-presence --
    /// Catalog connection / handshake established but the catalog
    /// itself is not in a usable state (e.g. service degraded).
    NotAvailable        { msg: String },

    /// The requested table does not exist in the catalog.
    TableNotFound       { fqn: String },

    /// The requested namespace does not exist in the catalog.
    NamespaceNotFound   { ns: String },

    /// The requested snapshot version of an existing table does not
    /// exist (e.g. expired snapshot, retention pruned).
    SnapshotNotFound    { table: String, version: String },

    // -- Transport / auth --
    /// Network connect / TLS handshake / DNS failure reaching the
    /// catalog endpoint.
    ConnectionFailed    { msg: String },

    /// Authentication credentials were rejected.
    AuthFailed,

    /// Authentication succeeded but the principal is not authorized to
    /// access the named resource.
    AuthDenied          { resource: String },

    /// The catalog call exceeded its deadline.
    Timeout             { millis: u64 },

    // -- Protocol / contract --
    /// `check_schema_drift` returned `DriftStatus::Breaking`.
    /// The accompanying `DriftKind` carries the structural detail.
    SchemaDrift         { kind: DriftKind },

    /// Partition-spec metadata was structurally malformed
    /// (transform unknown, source field absent, name collision).
    MalformedPartition  { msg: String },

    /// The catalog response violated its declared contract (e.g.
    /// missing required fields, type mismatch, schema version drift).
    MalformedResponse   { msg: String },

    /// Catch-all internal error from a `CatalogProvider`
    /// implementation (mapping fault, panic safety net, etc.).
    Internal            { msg: String },
}

impl semstrait_core::diagnostic::Diagnose for CatalogProviderErrorKind {
    fn message(&self) -> std::borrow::Cow<'_, str>;
    fn severity(&self) -> semstrait_core::Severity {
        semstrait_core::Severity::Error
    }
}
```

Variant identity is the stable contract. Renaming a variant is MAJOR;
adding a variant is MINOR (`#[non_exhaustive]`); refining
`Diagnose::message()` text is PATCH. Consumers wrap into their own
typed-kind via `From<CatalogProviderErrorKind>` (e.g.
`CompileErrorKind` in `33 §10.1`).

### 8.2 `FileSystemErrorKind`

```rust
/// Typed error-kind for the `FileSystem` trait surface.
/// Identification by variant identity per `30 §5`; numeric `FS_E_*`
/// codes are retired. Severity is conveyed via `Diagnose::severity()`;
/// every v1 variant is `Severity::Error`.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum FileSystemErrorKind {
    // -- Input / resource-presence --
    /// Object does not exist at the supplied path.
    NotFound            { path: Path },

    /// The supplied URI scheme is not handled by this `FileSystem`
    /// implementation (e.g. `s3://` passed to `LocalFileSystem`).
    UnsupportedScheme   { scheme: String },

    /// The supplied path is structurally invalid (e.g. malformed URI,
    /// non-UTF-8 bytes where UTF-8 is required, scheme-internal
    /// validation failure).
    InvalidPath         { path: Path, msg: String },

    /// The supplied glob pattern is structurally invalid (e.g.
    /// unmatched bracket, illegal escape sequence). Surfaces from
    /// `expand_glob` (§7.1).
    InvalidGlob         { pattern: String, msg: String },

    // -- Transport / auth --
    /// Network connect / TLS handshake / DNS failure reaching the
    /// storage endpoint.
    ConnectionFailed    { msg: String },

    /// Authentication credentials were rejected.
    AuthFailed,

    /// Authentication succeeded but the principal is not authorized to
    /// read / write the path.
    PermissionDenied    { path: Path },

    /// The transport call exceeded its deadline.
    Timeout             { millis: u64 },

    // -- Catch-all --
    /// Catch-all internal error from a `FileSystem` implementation.
    Internal            { msg: String },
}

impl semstrait_core::diagnostic::Diagnose for FileSystemErrorKind {
    fn message(&self) -> std::borrow::Cow<'_, str>;
    fn severity(&self) -> semstrait_core::Severity {
        semstrait_core::Severity::Error
    }
}
```

`FileSystemErrorKind` follows the same SemVer posture as
`CatalogProviderErrorKind` (renames MAJOR; additions MINOR; message
refinement PATCH).

### 8.3 Cross-crate wrapping

`CatalogProviderErrorKind` and `FileSystemErrorKind` are produced by the
transport traits. Stage entry-points that consume them wrap into their
own typed-kinds via `From` impls — for example `33 §10.1`'s
`CompileErrorKind::CatalogResolutionFailed { source: CatalogProviderErrorKind }`
and the `FromIoBytes` chain in `31b §5`. The wrap site attaches the
stage-relevant `Location` to the resulting `Diagnostic<K>` envelope; the
transport layer carries no `Location` field.

### 8.4 Code range registration — closed

Round-1 drafting proposed registering `CAT_E_0100`–`0399` and
`FS_E_0100`–`0199` ranges in `30 §6.2`. Both proposals are **closed without
action** as a consequence of the workspace-wide stable-code retirement
(`30 §5` typed-kind discipline). No subsystem-prefix table amendment is
required; consumers route on variant identity. See `Q-CAT-001` (closed).

---

## 9. Schema-drift gated I/O (I11b)

### 9.2 Signature

```rust
async fn check_schema_drift(
    &self,
    table: &TableRef,
    expected_schema: &Schema,
) -> Result<DriftReport, CatalogProviderErrorKind>;
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
- **No planner involvement.** `check_schema_drift` does NOT touch `semstrait-ir`, does NOT accept or return `SemanticPlan` / `PhysicalPlan`, and does NOT take a diagnostic sink. Failures surface as `CatalogProviderErrorKind`.

### 9.4 Caller policy

The caller (`semstrait-manifest`'s `SemanticManifest::verify_against_catalog` or equivalent) maps `DriftReport` to one of:

| `DriftStatus` | Caller response                                                                 |
|---------------|---------------------------------------------------------------------------------|
| `Unchanged`   | Proceed with execution using the compiled plan as-is.                           |
| `Compatible`  | Proceed; optionally emit an advisory diagnostic (`Severity::Info`).             |
| `Breaking`    | Abort execution; return `CatalogProviderErrorKind::SchemaDrift { kind }` (with the `DriftKind` from `DriftReport.details`) wrapped at the consumer in their own typed-kind. |

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

Callers propagate `CatalogProviderErrorKind` and `FileSystemErrorKind` through `?` into their own typed-kind enums (typically `CompileErrorKind` at compile time per `33 §10.1`; `33 §16.5`'s manifest I/O chain at I11b) via `From` impls. The wrapping site attaches stage-relevant location and emits a `Diagnostic<CompileErrorKind>` (or whatever the consumer's outer kind is). Variant identity remains stable across the wrap.

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

1. Return an error with a dedicated typed-kind variant (e.g. `CatalogProviderErrorKind::MethodNotSupported` / `FileSystemErrorKind::MethodNotSupported` added under `#[non_exhaustive]`), OR
2. Explicitly document that the default response is semantically meaningful (e.g. "returns empty when the method is genuinely not applicable, not when the provider simply lacks support").

### 11.2 Built-in implementation stability

`NoopCatalogProvider`, `IcebergRestCatalogProvider`, `UnityCatalogProvider`, `FilesystemCatalogProvider`, `LocalFileSystem`, `S3FileSystem`, `AzureFileSystem`, `GcsFileSystem` are **Stable** in v1: their struct identities, their `new`/`builder` constructors, and their observable behavior do not break in MINOR. Field additions on their configuration types (e.g. `S3Config`) are permitted additively.

### 11.3 Error-variant stability

`CatalogProviderErrorKind` and `FileSystemErrorKind` are `#[non_exhaustive]`. MINOR releases MAY add variants. Existing variants MUST NOT change shape (variant rename or field rename / removal is MAJOR per `30 §5`'s typed-kind SemVer rules). `Diagnose::message()` text refinement is PATCH; consumers route on variant identity, not on rendered message strings.

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

## 14. Cross-references

- Overview: `00 §4 The Public Surface`, `00 §5 Layer 3 — Runtime Integration`.
- Invariants: `00 §9 I3, I10, I11 (incl. I11b), I12`.
- API contracts: `30 §3 (Open/sealed traits)`, `30 §5 (Typed-kind discipline)`, `30 §7 (Result shapes)`, `30 §8 (Stability)`, `30 §9 (Async posture)`, `30 §10 (Per-crate async table)`.
- Compile-time consumers: `15 §5 (Compile-Time Resolution)`, `15 §6 (Source resolution: paths, tables, globs)`.
- Query-time consumers: `33 (semstrait-manifest, I11b gate)` — drafted adjacent to this doc.
- Sibling crate: `31 (semstrait-core)` — shared primitives (`ColumnName`, `DataType`, `Span`, `Diagnostic<K>`, `Diagnose`, `GlobPattern`).
- Downstream: `35 (semstrait-ir)`, `34 (semstrait-planner)`, `36 (semstrait-adapter)` — do NOT import from this crate.

---

