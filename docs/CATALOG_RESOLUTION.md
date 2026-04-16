# Catalog Resolution

**Status:** Implemented
**Scope:** Iceberg REST (via Polaris) + Unity Catalog + Null provider. DataFusion is the primary compute engine.

---

## 1. Motivation

The manifest compiler validates YAML structure and binds semantic types, but defers physical resolution to plan/execution time. This creates gaps:

| Gap | Impact |
|-----|--------|
| No column type validation | Mapping a string column to a float64 measure fails at query time, not compile time |
| No partition awareness | Temporal grains must be declared manually in YAML even when Iceberg metadata has them |
| No snapshot pinning | Compiled manifests aren't reproducible — catalog state may drift between compilation and execution |
| No source type discrimination | `resolved_sources` are raw strings; Path vs Table determined by heuristics |
| Manual table registration | DataFusion connector requires explicit `register_parquet()` calls |

Catalog resolution at compile time produces **authoritative physical bindings** that propagate through the plan into query execution.

---

## 2. Architecture

```
YAML Model
    |
[1] Parse, [2] Resolve refs
    |
    v
--- Source Resolution (step 3, requires providers for wildcards) ---
    |
[3] resolve_sources
    |  For storage.paths: expand globs via StorageProvider, read schema (best-effort)
    |  For storage.tables: lookup CatalogRegistry → CatalogProvider
    |    → list_tables() for wildcards, load_table_metadata() for metadata
    |    → Captures: columns, partition specs, snapshot ID, location, format
    |  Wildcard patterns without providers → CompileError
    |  Builds CatalogSnapshot during resolution
    |  Returns SourceResolutionResult consumed by emit()
    |
    v
[4-8] Validate, compile expressions
    |
    v
[9] Emit CompiledManifest (with CatalogSnapshot + ResolvedSources)
```

### Graceful Degradation

| Catalog State | Behavior |
|---------------|----------|
| Available + responsive | Full resolution: types validated, partitions mapped, snapshot pinned |
| Available but table missing | `CompileWarning` per missing table, continue |
| No catalog configured | Skip steps 10-13 entirely. `catalog_snapshot = None` |
| Catalog unreachable | `CompileWarning`, fall back to no-catalog path |

Acceleration structures (steps 14-21) always run — they don't depend on catalog.

---

## 3. CatalogProvider Trait Extension

The existing trait gains one new method with a default implementation:

```rust
#[async_trait]
pub trait CatalogProvider: Send + Sync {
    // Existing methods (unchanged)
    async fn list_tables(&self, namespace: &str, pattern: &GlobPattern) -> Result<Vec<TableRef>, CatalogError>;
    async fn get_schema(&self, table: &TableRef) -> Result<Vec<CatalogColumn>, CatalogError>;
    async fn table_exists(&self, table: &TableRef) -> Result<bool, CatalogError>;

    // NEW: Extended metadata including partitions, snapshots, location
    async fn load_table_metadata(&self, table: &TableRef)
        -> Result<Option<TableMetadataResponse>, CatalogError>
    {
        Ok(None) // Default: not supported by this catalog
    }
}
```

The default `Ok(None)` means existing catalog implementations (Unity, NullCatalog) work unchanged. Only `IcebergRestCatalog` overrides this.

### StorageProvider Trait

```rust
#[async_trait]
pub trait StorageProvider: Send + Sync {
    async fn expand_glob(&self, pattern: &str) -> Result<Vec<String>, CatalogError>;
    async fn read_schema(&self, path: &str, format: DataFormat) -> Result<Option<Vec<CatalogColumn>>, CatalogError>;
}
```

### CatalogRegistry

Named catalog provider map built from `catalogs.yaml`. Supports multiple catalogs of the same provider type (e.g., `polaris_prod` and `polaris_dev`).

```rust
pub struct CatalogRegistry {
    providers: HashMap<String, Arc<dyn CatalogProvider>>,
}
```

### TableMetadataResponse

```rust
pub struct TableMetadataResponse {
    pub columns: Vec<CatalogColumn>,
    pub partition_fields: Vec<CatalogPartitionField>,
    pub snapshot_id: Option<i64>,
    pub format_version: Option<u32>,
    pub location: Option<String>,
    pub format: Option<DataFormat>,
    pub properties: HashMap<String, String>,
}

pub struct CatalogPartitionField {
    pub source_column: String,
    pub transform: String,    // "identity", "year", "month", "day", "hour", "bucket[N]", "truncate[N]"
    pub name: String,
    pub field_id: i32,
}
```

---

## 4. Iceberg REST Metadata Extraction

The `IcebergRestCatalog` already calls `load_table()` internally (REST API: `GET /v1/namespaces/{ns}/tables/{table}`). The response includes full Iceberg table metadata — we currently only extract the schema. Extension captures:

| Field | Iceberg REST JSON Path | Type |
|-------|----------------------|------|
| Snapshot ID | `metadata.current-snapshot-id` | `i64` |
| Partition specs | `metadata.partition-specs[].fields[]` | Array |
| Default spec ID | `metadata.default-spec-id` | `i32` |
| Format version | `metadata.format-version` | `u32` |
| Location | `metadata.location` | String |
| Properties | `metadata.properties` | Map |

### Partition Spec Resolution

Iceberg partition fields reference columns by `source-id` (field ID), not by name. Resolution:

1. Build `HashMap<i32, String>` from current schema's `IcebergField.id → IcebergField.name`
2. For each partition field, look up `source_id` in the map
3. If field ID not found (column dropped) → emit warning, skip partition field

### Transform Parsing

Iceberg transform strings: `"identity"`, `"year"`, `"month"`, `"day"`, `"hour"`, `"bucket[N]"`, `"truncate[N]"`, `"void"`.

Mapped to `PartitionTransform` enum:

| Iceberg Transform | PartitionTransform | Inferred Grain |
|---|---|---|
| `identity` | `Identity` | None |
| `year` | `Year` | `TemporalGrain::Year` |
| `month` | `Month` | `TemporalGrain::Month` |
| `day` | `Day` | `TemporalGrain::Day` |
| `hour` | `Hour` | `TemporalGrain::Hour` |
| `bucket[N]` | `Bucket(N)` | None |
| `truncate[N]` | `Truncate(N)` | None |

---

## 5. Catalog Snapshot Types

Stored on `CompiledManifest.catalog_snapshot: Option<CatalogSnapshot>`:

```rust
pub struct CatalogSnapshot {
    pub tables: HashMap<String, TableSnapshot>,
    pub captured_at: DateTime<Utc>,
}

pub struct TableSnapshot {
    pub fqn: String,
    pub columns: Vec<ResolvedColumn>,
    pub iceberg: Option<IcebergMetadata>,
}

pub struct ResolvedColumn {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub comment: Option<String>,
    pub field_id: Option<i32>,
}

pub struct IcebergMetadata {
    pub snapshot_id: Option<i64>,
    pub partition_spec: Vec<PartitionField>,
    pub format_version: Option<u32>,
    pub location: Option<String>,
    pub properties: HashMap<String, String>,
}

pub struct PartitionField {
    pub source_column: String,
    pub transform: PartitionTransform,
    pub name: String,
    pub inferred_grain: Option<TemporalGrain>,
}

pub enum PartitionTransform {
    Identity,
    Year,
    Month,
    Day,
    Hour,
    Bucket(u32),
    Truncate(u32),
}
```

All types implement `Serialize + Deserialize` for JSON persistence.

---

## 6. Source Type Discrimination

`DatasetBinding.resolved_sources` uses `Vec<ResolvedSource>`:

```rust
pub struct ResolvedSource {
    pub reference: String,
    pub source_type: SourceType,
    pub location: Option<String>,
    pub format: Option<DataFormat>,
    pub catalog_alias: Option<String>,
    pub schema: Option<Vec<ResolvedColumn>>,
}

pub enum SourceType {
    Path,   // File path (S3, GCS, local)
    Table,  // Catalog-managed table name
}
```

Discrimination is determined at compile time from `StorageConfig`:
- `storage.paths` entries → `SourceType::Path`
- `storage.tables` entries → `SourceType::Table`

This replaces the hardcoded `SourceType::Path` in acceleration.rs.

---

## 7. Column Schema Validation

When catalog metadata is available, the compiler validates `column_mapping` entries against the physical schema:

| Mapping Key | Physical Schema | Result |
|-------------|----------------|--------|
| `"amount"` → column `"amount"` exists | `Float64` | OK |
| `"amount"` → column `"amount"` missing | — | `CompileWarning`: "column 'amount' not found in physical schema" |
| Literal mapping `"source"` → `"web"` | N/A | Skipped (not a physical column) |
| Metadata dimension | N/A | Skipped (extracted from path, not columns) |

Validation produces **warnings**, not errors. Schema may drift between compilation and execution — the warning provides early feedback without blocking compilation.

---

## 8. DataFusion Integration

The `DataFusionConnector` gains a convenience method:

```rust
pub async fn register_manifest_sources(
    &self,
    manifest: &CompiledManifest,
) -> Result<Vec<String>, ConnectorError>
```

For each resolved source:
- **Path** → `register_file(dataset_name, path)` (auto-detects .csv/.parquet)
- **Table** → look up `catalog_snapshot.tables[fqn].iceberg.location` → `register_parquet(dataset_name, location)`

Returns the list of successfully registered table names. Failures are skipped (logged but not fatal).

---

## 9. Current Scope

| Scope | Notes |
|-------|-------|
| Catalogs | Iceberg REST (via Polaris), Unity Catalog, `NullCatalogProvider` |
| Storage providers | Local filesystem (`local` feature), S3 stub (`aws` feature), `NullStorageProvider` |
| Schema drift | Detected via `CatalogSnapshot`; emitted as `PlannerWarning`, not compile error (DL-037, DL-065) |
| Partition pruning | Partition info captured in snapshot; planner-level pruning is a known future extension |
