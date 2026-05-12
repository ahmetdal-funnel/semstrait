# semstrait-model

Author surface for `semstrait` semantic models — owns the in-memory
[`SemanticModel`] tree, the parse/validate stages, the fluent
loader, and the per-struct fluent builders.

The structural source of truth is the spec under
[`docs/design/`](../../docs/design/). This crate's Rust types
implement [`docs/design/apis/32_semstrait_model.md`](../../docs/design/apis/32_semstrait_model.md);
the YAML projection is documented there and mirrored by
[`schemas/semantic_model.schema.json`](schemas/semantic_model.schema.json).

---

## Surface

| Stage          | Entry point                              | Spec       |
|----------------|------------------------------------------|------------|
| Parse YAML     | [`parse`] / [`parse_with_source`]        | `32 §9.1`  |
| Validate model | [`validate`]                             | `32 §9.4`  |
| Parse catalogs | [`parse_catalogs`]                       | `32b §5.1` |
| Fluent load    | [`SemanticModel::loader()`]              | `32 §9.6`  |
| Fluent build   | [`SemanticModel::builder()`] / per-struct `::builder()` | `32 §9.7` |

All four stages are sync, accumulating, and free of `stdout`/`stderr`
side effects (invariants I9 / I11 / I3). Both `parse` and `validate`
return `Result<(T, Diagnostics<K>), Diagnostics<K>>` so warnings
travel alongside successful outputs and errors are surfaced as a
batch (`30 §4`).

---

## Construct from YAML

```rust
use semstrait_model::SemanticModel;

let (model, diagnostics) = SemanticModel::loader()
    .from_yaml_file("model.yaml")
    .from_catalogs_yaml_file("catalogs.yaml")
    .build()?;
```

Tests and tooling can swap the filesystem strategy:

```rust
use semstrait_model::{InMemoryFs, SemanticModel};

let mut fs = InMemoryFs::new();
fs.insert("model.yaml", REFERENCE_YAML);

let (model, _diagnostics) = SemanticModel::loader()
    .with_fs(fs)
    .from_yaml_file("model.yaml")
    .build()?;
```

The loader composes [`parse`] → [`parse_catalogs`] → [`validate`] and
folds every diagnostic into a single
[`ModelBuildErrorKind`](crate::ModelBuildErrorKind) accumulator.

---

## Construct from code

The builder API is per-struct and structural-faithful: method names
equal Rust field names; variant-body constructors map 1:1 onto spec
shapes (`32 §9.7.1`).

```rust
use semstrait_core::{DataType, Grain};
use semstrait_model::{
    AdditivityType, AggregationType, Cardinality, ComplexExtras,
    Dataset, Dimension, DimensionEntry, DimensionType, Grainset,
    JoinKeyExprPair, KeyDecl, Keys, LeafExtras, Measure, MeasureEntry,
    NestedDataset, Relationship, SemanticInterface, SemanticMapping,
    SemanticModel, TemporalShape,
};

// ── Shared root-pool entries ────────────────────────────────────
let order_ts = Dimension::builder("order_ts")
    .data_type(DataType::Timestamp { precision: 6 })
    .dim_type(DimensionType::temporal([Grain::Minute, Grain::Hour, Grain::Day]))
    .build();

let revenue = Measure::builder("revenue")
    .data_type(DataType::Decimal { precision: 18, scale: 2 })
    .agg(AggregationType::Sum)
    .additivity(AdditivityType::Full)
    .build();

// ── Public Dataset (body fields + Public-only fields) ───────────
let extras = LeafExtras::builder()
    .semantic_mapping(
        SemanticMapping::builder()
            .column("revenue", "amount_cents")
            .build(),
    )
    .temporal(TemporalShape::events("order_ts", Some(Grain::Minute)))
    .build();

let interface = SemanticInterface::builder()
    .dimensions(vec![DimensionEntry::r#ref("order_ts")])
    .measures(vec![MeasureEntry::r#ref("revenue")])
    .keys(
        Keys::builder()
            .primary(KeyDecl::builder().columns(vec!["order_id".into()]).build())
            .build(),
    )
    .build();

let orders = Dataset::builder("orders")
    .extras(extras.clone())
    .description("Order-line fact dataset.")
    .semantic_interface(interface.clone())
    .build();

// ── Public Grainset with two NestedDataset children (R3) ────────
let returns = NestedDataset::builder("returns").extras(extras.clone()).build();
let refunds = NestedDataset::builder("refunds").extras(extras).build();

let order_events = Grainset::builder("order_events")
    .extras(ComplexExtras::default())
    .dataset(returns)
    .dataset(refunds)
    .description("Roll-up of order-side events.")
    .semantic_interface(interface)
    .build();

// ── Cross-public Relationship ───────────────────────────────────
let orders_to_events = Relationship::builder()
    .name("orders_to_events")
    .from("orders")
    .to("order_events")
    .keys(vec![JoinKeyExprPair::columns("order_id", "order_id")])
    .cardinality(Cardinality::ManyToOne)
    .build()?;

// ── Root model — `.build()` runs `validate` ─────────────────────
let (model, _diagnostics) = SemanticModel::builder()
    .name("analytics-v1")
    .dataset(orders)
    .grainset(order_events)
    .dimension(order_ts)
    .measure(revenue)
    .relationship(orders_to_events)
    .build()?;
```

The structural-fidelity rule (`32 §9.7.1`) constrains the builder API:

- Builder methods equal Rust field names (`.data_type(...)`,
  `.dim_type(...)`, `.agg(...)`, `.semantic_mapping(...)`) — no
  abbreviations, no synonyms.
- Variant-body constructors are 1:1 with the spec body. Example:
  `DimensionType::temporal(grains)` accepts exactly the
  [`TemporalDimensionBody`](crate::entities::TemporalDimensionBody)
  payload (`18 §4.1`); no extra parameters, no side fields.
- Helpers that conflate multiple spec fields, or that introduce
  vocabulary outside the spec (e.g. `SemanticMapping::schema_table`),
  are forbidden by construction.

---

## Type map

`semstrait_model` re-exports the public surface from the root
([`lib.rs`](src/lib.rs)). Types group along three axes:

### Root model

| Type | Spec | Notes |
|------|------|-------|
| [`SemanticModel`] | `32 §2` | Root struct. Holds four data-kind `BTreeMap`s and three shared-pool `BTreeMap`s. No `catalogs:` field — that's a sibling file. |
| [`AiContext`] | `18 §8` | LLM-facing hint surface. Authored only on Public DataKinds and root-pool entries. |

### Data kinds

The four data-kind variants come in two **forms** — Public (top-level)
and Nested (inside a complex parent). Public forms carry the full
[`SemanticInterface`]; nested forms are structurally-only per
[`26 §3`](../../docs/design/data-kinds/26_nesting_matrix.md).

| Public form | Nested form | Body | Spec |
|-------------|-------------|------|------|
| [`Dataset`]  | [`NestedDataset`]  | [`DatasetBody`]  | `32 §3.3` |
| [`Grainset`] | [`NestedGrainset`] | [`GrainsetBody`] | `32 §3.3` |
| [`Unionset`] | [`NestedUnionset`] | [`UnionsetBody`] | `32 §3.3` |
| [`Joinset`]  | [`NestedJoinset`]  | [`JoinsetBody`]  | `32 §3.3` |

Sealed traits ([`DataKind`], [`SimpleDataKind`], [`ComplexDataKind`],
[`PublicDataKind`], [`NestedDataKind`]) classify each concrete type
along structural and behavioral axes (`32 §3.4`). View enums
([`AnyDataKindRef`], [`PublicDataKindRef`], [`NestedDataKindRef`],
[`SimpleDataKindRef`], [`ComplexDataKindRef`]) provide unified
borrowing surfaces (`32 §3.6`).

### Extras

Per-DataKind physical configuration. Two flavors per `32 §4`:

| Type | Used by | Fields |
|------|---------|--------|
| [`LeafExtras`]    | `Dataset` / `NestedDataset` | `catalog`, `storage`, `semantic_mapping`, `temporal` |
| [`ComplexExtras`] | every other variant         | `temporal` only — leaf-only fields are R-6-forbidden |

Storage / mapping helpers: [`StorageConfig`], [`StorageFormat`],
[`PartitionDef`], [`CatalogRef`], [`SemanticMapping`],
[`SemanticMappingValue`], [`LiteralValue`], [`PhysicalExpr`].

### Semantic interface

[`SemanticInterface`] composes the per-DataKind authoring surface
(`18 §1`):

| Type | Spec | Variants |
|------|------|----------|
| [`Dimension`] / [`DimensionEntry`] / [`DimensionRef`] | `18 §4` | [`DimensionType`] = Temporal, Categorical, Binary, Geo, Bucketed, Metadata |
| [`Measure`] / [`MeasureEntry`] / [`MeasureRef`] | `18 §5` | [`AggregationType`] roster, [`AdditivityType`] (Full, Semi via [`SemiAdditivity`], Non) |
| [`Metric`] / [`MetricEntry`] / [`MetricRef`] | `18 §6` | Required `expr:` |
| [`Keys`] / [`KeyDecl`] / [`ForeignKeyDecl`] | `18 §9` | Primary, unique, foreign |
| [`DataKindFilter`] / [`AggregationFilter`] | `18 §7` | DataKind-scoped vs. Measure/Metric-scoped |

### Temporal shape

[`TemporalShape`] (struct) + [`TemporalShapeKind`] (enum) per
[`17_temporal_shape.md`](../../docs/design/foundations/17_temporal_shape.md):

| Variant | Body | When to use |
|---------|------|-------------|
| `Timeseries` | [`TimeseriesBody`] | periodic series, semi-additive |
| `Events`     | [`EventsBody`]     | independent occurrences |
| `Snapshot`   | [`SnapshotBody`]   | point-in-time facts |
| `Scd`        | [`ScdBody`] (+ [`ScdType`] Type1 / Type2) | slowly-changing dimensions |

### Relationships

[`Relationship`] (cross-public or per-`Joinset` internal) per `18 §2`,
with derived [`JoinType`]. Shape: [`Cardinality`], [`Integrity`],
[`Optional`], [`CrossFilter`], [`JoinKeyExprPair`].

---

## Structural-rule taxonomy

`parse` and `validate` together implement the rules from
[`32 §6`](../../docs/design/apis/32_semstrait_model.md) and
[`18 §11`](../../docs/design/foundations/18_entities.md). Rule IDs map
to variants of [`ParseErrorKind`] and [`ValidateErrorKind`]:

### Root-shape (`SR-*`)

| ID | Stage    | Variant                                         |
|----|----------|-------------------------------------------------|
| SR-1, SR-2 | parse    | `MalformedRoot`, `UnknownTopLevelBlock`         |
| SR-3       | parse    | `DuplicateDataKindName`, `DuplicateSharedSemanticsName` |
| SR-5 (R2)  | parse    | `IllegalSelfNesting`                            |
| SR-5 (R3)  | parse    | `NestedDataKindCarriesInterface`                |
| SR-6       | validate | `MissingRequiredExtras`                         |
| SR-10      | validate | `ComplexDataKindInsufficientChildren`           |

### Entity-level (`SR-E-*`)

| ID | Stage    | Variant                                                  |
|----|----------|----------------------------------------------------------|
| SR-E-1     | validate | `InvalidReferenceOverride`                       |
| SR-E-2     | validate | `SemanticsRefMissingExpr`                        |
| SR-E-3     | validate | `OrphanSharedSemantics`                          |
| SR-E-4     | parse    | `RelationshipMissingCardinality`                 |
| SR-E-5     | validate | `RelationshipDanglingEndpoint`                   |
| SR-E-6     | validate | `TemporalLeafMissingGrain`                       |
| SR-E-7     | validate | `TemporalGrainOnComplex`                         |
| SR-E-8     | validate | `GrainsetChildMissingGrain`                      |
| SR-E-9     | parse    | `MeasureMissingAgg`                              |
| SR-E-10    | parse    | `SemanticsMissingDataType`                       |
| SR-E-11    | validate | `FilterWrongKind`                                |
| SR-E-12    | validate | `SemanticsDataTypeMismatch`                      |
| SR-E-13    | validate | `RelationshipSymmetricCardinalityIncomplete`     |
| SR-E-14    | validate | `RelationshipManyToManyCrossFilterDirectional`   |

`SR-E-*` rules also fire from the builder: `Relationship::builder()`
runs SR-E-13 and SR-E-14 at `.build()` time, and
`SemanticModel::builder().build()` re-runs the full `validate` pass
so YAML-loaded and code-built models share the same diagnostics.

`SemanticsShadowRootPool` is the sole `Severity::Warning` variant —
emitted when a Public DataKind's inline declaration shadows a shared
root-pool entry of the same name (`18 §1.5`).

---

## Catalogs

`catalogs.yaml` is a sibling file, never embedded
([`32 §1.3`](../../docs/design/apis/32_semstrait_model.md),
[`32b`](../../docs/design/apis/32b_catalogs_yaml.md)).

| Type | Notes |
|------|-------|
| [`CatalogsConfig`]    | Top-level. `catalogs: BTreeMap<String, CatalogEntry>`. |
| [`CatalogEntry`]      | `type`, `name`, `url`, optional `realm` / `default_namespace`, `auth`. |
| [`CatalogAuthMethod`] | Internally-tagged enum (`oauth2` / `bearer` / `aws_secrets`). |
| [`SecretKeyMapping`]  | Secrets-Manager JSON-key override. |

`${VAR}` substitution runs ahead of YAML decoding for both model and
catalogs files (`32 §8`, `32b §6`).

---

## Reference materials

- [`schemas/reference.yaml`](schemas/reference.yaml) — every public
  authoring concept exemplified.
- [`schemas/catalogs_reference.yaml`](schemas/catalogs_reference.yaml)
  — every `CatalogAuthMethod` variant exemplified.
- [`schemas/semantic_model.schema.json`](schemas/semantic_model.schema.json)
  — JSON Schema (draft 2020-12) for authoring tools.
- [`schemas/catalogs.schema.json`](schemas/catalogs.schema.json) —
  JSON Schema for the sibling catalogs file.
- [`tests/schema_roundtrip.rs`](tests/schema_roundtrip.rs) — asserts
  reference YAML validates against the schema, parses, and validates
  clean.

---

## Diagnostic primitives

All diagnostic types live in `semstrait-core` per `31 §6`:

```rust
use semstrait_core::diagnostic::{Diagnostic, Diagnostics, Severity, SourceId};
use semstrait_model::ParseErrorKind;

let result: Result<_, Diagnostics<ParseErrorKind>> = semstrait_model::parse(yaml);
```

Stage-specific kinds: [`ParseErrorKind`], [`ValidateErrorKind`],
[`CatalogsParseErrorKind`], [`ModelBuildErrorKind`] (loader/builder
fused kind).

---

## Spec link map

| Topic | Authoritative |
|-------|---------------|
| Root shape & contract | [`32_semstrait_model.md`](../../docs/design/apis/32_semstrait_model.md) |
| Catalogs sibling file | [`32b_catalogs_yaml.md`](../../docs/design/apis/32b_catalogs_yaml.md) |
| Cross-crate API contracts | [`30_api_contracts.md`](../../docs/design/apis/30_api_contracts.md) |
| Entity surface (`Dimension`/`Measure`/…) | [`18_entities.md`](../../docs/design/foundations/18_entities.md) |
| Temporal shape | [`17_temporal_shape.md`](../../docs/design/foundations/17_temporal_shape.md) |
| Mapping & binding | [`15_mapping_and_binding.md`](../../docs/design/foundations/15_mapping_and_binding.md) |
| Composition / nesting | [`16_composition.md`](../../docs/design/foundations/16_composition.md), [`12_nesting_policy.md`](../../docs/design/foundations/12_nesting_policy.md), [`26_nesting_matrix.md`](../../docs/design/data-kinds/26_nesting_matrix.md) |
| Per-kind chapters | [`21_dataset.md`](../../docs/design/data-kinds/21_dataset.md), [`22_grainset.md`](../../docs/design/data-kinds/22_grainset.md), [`23_unionset.md`](../../docs/design/data-kinds/23_unionset.md), [`24_joinset.md`](../../docs/design/data-kinds/24_joinset.md) |
| Applicability matrix | [`25_applicability_matrix.md`](../../docs/design/data-kinds/25_applicability_matrix.md) |
| Names & scopes | [`11_names_and_scopes.md`](../../docs/design/foundations/11_names_and_scopes.md) |
| Types & grain | [`13_types_and_grain.md`](../../docs/design/foundations/13_types_and_grain.md) |
| Expressions | [`14_expressions.md`](../../docs/design/foundations/14_expressions.md), [`19_expression_flow.md`](../../docs/design/foundations/19_expression_flow.md) |

---

## Dependencies

- `semstrait-core` — diagnostic primitives, `DataType`, `Grain`,
  `ExprSource`/`ExprBlock` carrier (`31 §6`).
- `serde`, `serde_yaml` — YAML decode.
- `bon` — typestate builders (`32 §9.7`).
- `indexmap` — author-order preservation in YAML intermediates.
- `thiserror` / `tracing` — internal plumbing.
