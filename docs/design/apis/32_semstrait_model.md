---
prereqs: [11, 12, 13, 14, 14a, 15, 16, 17, 20, 21, 22, 23, 24, 30, 31]
authoritative-for:
  - the `semstrait-model` public-API surface (types, functions, re-exports)
  - the author-facing YAML surface for semantic models (top-level blocks, DataKind variant tags, Semantics blocks, `relationships:` block, `binding:` / `column_mapping:` block, `temporal_shape:` block)
  - the in-memory `SemanticModel` typed tree (fields, variant nesting, serde derivations, deterministic ordering under I4)
  - the `parse` free function signature (`&str -> Result<SemanticModel, ParseErrors>`), its contract (pure, sync, no I/O), and its multi-file-loading posture (out of scope in `32`; deferred to a caller or a thin helper)
  - the `ParseError` typed enum (variants, stable `code()` return, `IntoDiagnostic` impl)
  - `semstrait-model`'s share of `ValidateError` surface (variants raised at parse / structural-validate boundaries, per `31 §8.2`)
  - crate boundaries against `semstrait-core` below (consumes `Expr`-family / `DataType` / `Diagnostic` / `ValidateError` / `CompileError`) and `semstrait-manifest` above (no reverse deps)
  - mapping of design invariants I1, I4, I6, I10, I11, I12 to concrete crate-level guarantees
refined-by:
  - 33 (`semstrait-manifest` consumes `SemanticModel` and lowers to `Manifest`; owns `compile`)
  - 34 (`semstrait-planner` consumes `Manifest` only; does not see `SemanticModel`)
  - 36 (`semstrait-adapter` is invoked by `RegistryExtension` per `31 §5.8`; does not see `SemanticModel`)
  - 40 (`implementation/40_refactor_plan.md` — current code-vs-target YAML delta is tracked here)
---

# 32. semstrait-model

> **Status:** ratified. `32` fixes the public surface of `semstrait-model` — the YAML-parse + in-memory model-tree crate — against `11` (names), `12` (nesting), `13` (types and grain), `14` (expressions), `14a` (function catalog), `15` (mapping and binding), `16` (composition), `17` (temporal shape), `20`–`24` (DataKind taxonomy and variants), `30` (stability / diagnostics policy), and `31` (shared-types crate). All vocabulary below is already ratified upstream; `32` adds only the author-facing YAML spellings, the typed tree that carries them, the `parse` entry point, and the `ParseError` enum. Pure and synchronous per I6 / I11.

## 1. Purpose, Scope, Layering

`semstrait-model` is the **author-facing-surface crate**. It translates YAML text into a typed, in-memory representation of a Semantic Model — the `SemanticModel` tree — and nothing else. Its responsibilities are deliberately narrow:

- **Parse.** Turn a YAML string into a `SemanticModel` value (§10).
- **Structural validate.** Catch shape-level errors that don't require name resolution (duplicate names, illegal nesting per `12 §2`, missing required fields, malformed `ExprSource` blocks per `14 §4`). Resolution-dependent validation lives in `semstrait-manifest::compile` (`31 §8.3`, `14 §7.3`).
- **Own the YAML grammar.** The author-facing vocabulary — `datakinds:`, `relationships:`, `semantics:`, the `binding:` / `column_mapping:` sub-blocks, `temporal_shape:` block — is ratified here. Every other crate consumes the `SemanticModel` type; no other crate reads YAML.

### 1.1 What `semstrait-model` OWNS

- The author-facing YAML grammar: top-level blocks (§4), DataKind variant tags (§5), Semantics blocks (§6), `relationships:` block (§7), `binding:` / `column_mapping:` blocks (§8), `temporal_shape:` block (§9).
- The typed `SemanticModel` root (§3) and the variant structs it transitively holds — `DataKind`, `SimpleDataKind`, `ComplexDataKind`, `Unionset` / `Grainset` / `Joinset` specs, `Relationship`, `Binding`, `ColumnMapping`, `TemporalShape`.
- The YAML-to-typed dispatch for `ExprSource` (per `14 §4.2`) — i.e. given an `expr:` field on a parse site, decide whether the context admits an `Expr::Column` leaf (binding-layer, lowered to `PhysicalExpr`) or requires an `EntityRef` + `Aggregate`-capable tree (semantic-layer, lowered to `SemanticExpr`).
- The `parse` free function (§10) and its streaming-vs-one-shot contract.
- The `ParseError` typed enum (§11.1), its `code()` derivation, and its `IntoDiagnostic` impl.
- The `semstrait-model`-raised subset of `ValidateError` variants (§11.2), delegating the enum definition itself to `semstrait-core` per `31 §8.2`.
- Deterministic ordering guarantees on every collection the `SemanticModel` carries (§12), required by I4.

### 1.2 What `semstrait-model` does NOT own

- **Name resolution.** `EntityRef` resolution, Semantics-scope lookup, cross-DataKind reachability — all live in `semstrait-manifest::compile` (`31 §8.3`, `11 §7`).
- **Type inference / coercion.** `DataType` on a derived `Dimension` or `Measure` may be omitted in YAML; resolving it to a concrete canonical type is a `compile` concern (`14 §7.3`, `13 §6`).
- **Composition materialization.** Building `ComposedSemanticInterface` per `16 §5` — `UnifiedSemantics`, `FieldProvenance`, `CompositionCoverage` — is a `compile` concern (`16 §10`).
- **Physical source resolution.** `PhysicalSource` path patterns, catalog lookups, glob expansion — all live in `semstrait-catalog` + `semstrait-manifest`; `32` only records the author's declared `path:` / `table:` / `snapshot:` patterns (§8.2).
- **Manifest planning.** `Manifest`, `ResolvedDataKind`, `ResolvedExprTable`, `ResolvedBinding` — all `semstrait-manifest` (`33`).
- **I/O.** `parse` takes a `&str`. Reading YAML from a file, walking a directory, or resolving `$include` directives is the caller's responsibility or lives in a thin loader helper whose ratification is deferred (§10.3).

### 1.3 Design posture — thinnest-possible authoring crate

`semstrait-model` is intentionally **thin**. It exists so that every other crate above it can consume a pre-typed `SemanticModel` without touching YAML again. If a piece of logic needs access to the `Manifest` to produce a decision, it does not belong here — it belongs in `semstrait-manifest`. If a decision is author-facing (which YAML key spells which variant; what the default value of a field is; how `$VAR` substitution works), it lives here and only here.

The crate sits one level above `semstrait-core` in the workspace DAG (I7):

```
semstrait-core   (leaf: Expr / DataType / Diagnostic / ValidateError / CompileError / FunctionRegistry)
    ↑
semstrait-model  (parse + SemanticModel + ParseError + validate structural)
    ↑
semstrait-manifest, semstrait-planner, semstrait-adapter, ... (downstream)
```

**Dependencies below.** `semstrait-core` only, plus `serde`, `serde_yaml`, and `thiserror`. No `semstrait-manifest`, no `semstrait-planner`, no `semstrait-adapter` — attempting to add any upward workspace dep fails in CI (§14.1).

**Dependencies above.** None. Nothing in `semstrait-model` imports from any other `semstrait-*` crate except `semstrait-core`.

## 2. Public Crate Surface

Top-level roster. One row per exported item, grouped by module.

### 2.1 Root re-exports

| Item | Kind | Source | Purpose |
|---|---|---|---|
| `parse` | `pub fn` | §10.1 | YAML `&str` → `SemanticModel`. |
| `SemanticModel` | `pub struct` | §3 | Root of the typed tree. |
| `ParseError` | `pub enum` | §11.1 | Parse-stage typed error. `#[non_exhaustive]` per I10. |
| `ParseErrors` | `pub struct` | §11.1 | Accumulation wrapper; impls `IntoDiagnostic`. |

### 2.2 `semstrait_model::model`

| Item | Kind | Source | Purpose |
|---|---|---|---|
| `DataKind` | `pub enum` | §3.2 | Top-level entity sum type: `Simple | Complex(…)`. |
| `ComplexDataKind` | `pub enum` | §3.2 | `Unionset | Grainset | Joinset`. |
| `SimpleDataKind` | `pub struct` | §5.1 | Per `21 §2.1`. |
| `UnionsetSpec` | `pub struct` | §5.3 | Per `23 §2.1`. |
| `GrainsetSpec` | `pub struct` | §5.2 | Per `22 §2.1`. |
| `JoinsetSpec` | `pub struct` | §5.4 | Per `24 §2.2`. |
| `SemanticInterface` | `pub struct` | §6.1 | Dimensions / Measures / Metrics / Filters / Keys per `11 §2`. |
| `Dimension`, `Measure`, `Metric`, `Filter`, `Key` | `pub struct` | §6.2–§6.6 | The five Semantics carriers per `11 §2`. |
| `DimensionType` | `pub enum` | §6.2 | Temporal / Categorical / Metadata / Binary / Geo / Bucketed per `13 §4`. |
| `Additivity` | `pub enum` | §6.3 | Measure / Metric additivity per `11 §6`. |
| `Relationship` | `pub struct` | §7.1 | Per `16 §2.1`. |
| `Binding` | `pub struct` | §8.1 | Per `15 §3`. |
| `PhysicalSource` | `pub enum` | §8.2 | `File | Table | Snapshot` per `15 §3.2`. |
| `ColumnMapping` | `pub struct` | §8.3 | Per-Binding Semantics → value map per `15 §4`. |
| `ColumnMappingValue` | `pub enum` | §8.3 | `Column | Literal | Computed | Metadata` per `15 §5`. |
| `TemporalShape` | `pub enum` | §9 | Per `17 §2.1`. |
| `ScdSubtype` | `pub enum` | §9.5 | Per `17 §2.2`. |
| `DataKindName`, `SemanticsName`, `RelationshipId` | newtype `pub struct`s | §3.3 | Identifiers per `11 §3`. |
| `DataKindRef` | `pub enum` | §3.3 | Inline or by-name per `11 §5`. |

### 2.3 `semstrait_model::expr_source`

Re-exports the `ExprSource` / `ExprBlock` types from `semstrait-core` (per `31 §3.4`) plus the parse-site dispatch helpers:

| Item | Kind | Source | Purpose |
|---|---|---|---|
| `ExprSource` | re-export | `semstrait_core::expr::ExprSource` | YAML-surface expression per `14 §4`. |
| `ExprBlock` | re-export | `semstrait_core::expr::ExprBlock` | Declarative block per `14 §4.4`. |
| `ExprSource::parse_semantic` | `pub fn` | §6.2 note | Inline-or-Block → `SemanticExpr`. |
| `ExprSource::parse_physical` | `pub fn` | §8.3 note | Inline-or-Block → `PhysicalExpr` (authored, pre-compile). |

### 2.4 Crate-root free functions

| Item | Kind | Source | Purpose |
|---|---|---|---|
| `parse` | `pub fn` | §10.1 | The single entry point. |
| `substitute_env_vars` | `pub(crate) fn` | §10.2 | `${VAR}` → env lookup; called internally by `parse`. |

No other free functions are exported. Free-function proliferation is explicitly rejected (cf. `31 §9.3`); anything that would be a free function is either a method on a `SemanticModel` sub-type or belongs in `semstrait-manifest::compile`.

## 3. The `SemanticModel` Root Type

### 3.1 Shape

```rust
/// Root of the in-memory typed tree produced by `parse`. Carries every
/// top-level DataKind, every `relationships:` entry, and every shared
/// Semantics declaration.
///
/// Deterministic ordering (I4): every collection field is either a
/// `BTreeMap<Name, T>` (alphabetical) or a `Vec<T>` preserving YAML
/// author order. See §12.
#[non_exhaustive]
pub struct SemanticModel {
    /// Author-declared model name — a kebab-case identifier carried on
    /// the `semantic_model:` root per `11 §4`. Required.
    pub name: DataKindName,

    /// Optional top-level description.
    pub description: Option<String>,

    /// Optional AI-context block (synonyms, query patterns). Opaque to
    /// the planner; surfaced to downstream ML tooling unchanged.
    pub ai_context: Option<AiContext>,

    /// Optional free-form labels (kebab-case identifiers). Used by
    /// tooling to filter / group models; not consulted by the planner.
    pub labels: Vec<String>,

    /// Catalog namespace for physical-source resolution. Defaults to
    /// `"default"` when absent per `15 §3.2`'s catalog rules.
    pub namespace: Option<String>,

    /// All top-level DataKinds keyed by canonical name. Includes Simple,
    /// Unionset, Grainset, and Joinset variants merged into a single
    /// namespace per `11 §5.1`. Alphabetical by key; the `Vec<DataKind>`
    /// inside each variant's `children:` field preserves author order.
    pub data_kinds: BTreeMap<DataKindName, DataKind>,

    /// Top-level relationships per `16 §2.1`. Vec-preserves author order
    /// (relationships have no intrinsic name-identity ordering beyond
    /// `RelationshipId`, which is alphabetical-comparable but `Vec`
    /// ordering matches YAML source).
    pub relationships: Vec<Relationship>,

    /// Shared `Semantics` registry — reusable Dimensions, Measures,
    /// Metrics, Filters, Keys authored once at the model root and
    /// referenced via `ref:` from inside DataKind interfaces per `11 §5`.
    pub semantics: SharedSemanticsRegistry,

    /// Adapter-contributed function extensions declared at the model
    /// scope (rare). Folded into the `FunctionRegistry` at `compile`;
    /// `32` records the declaration, `33` binds it. Per `14a §7.1`.
    pub functions: Vec<FunctionExtension>,
}
```

`#[non_exhaustive]` per I10. Every field is `pub` so that `33`'s `compile` can destructure without a getter boilerplate; construction outside `parse` is discouraged but not forbidden (test harnesses, programmatic model building for tooling may rely on it).

### 3.2 Variant typing

`data_kinds` is a flat `BTreeMap<DataKindName, DataKind>`. The `DataKind` enum dispatches on variant:

```rust
#[non_exhaustive]
pub enum DataKind {
    Simple(SimpleDataKind),
    Complex(ComplexDataKind),
}

#[non_exhaustive]
pub enum ComplexDataKind {
    Unionset(UnionsetSpec),
    Grainset(GrainsetSpec),
    Joinset(JoinsetSpec),
}
```

The nesting is intentional: `20 §3`'s taxonomy splits DataKinds into Simple (one canonical strategy) vs Complex (one strategy per variant). The two-level enum mirrors the taxonomy; `33`'s `ResolvedDataKind` mirrors the same split. Every variant struct is separately `#[non_exhaustive]`; adding a new `ComplexDataKind` variant (e.g. a ratified `Bridgeset` from Round 2) is a MINOR change per `30 §2`.

### 3.3 Identifier newtypes

Identifiers are typed for clarity and to prevent accidental cross-mixing:

```rust
/// Top-level DataKind name per `11 §5.1`. Kebab-case identifier;
/// globally unique across the model's DataKind namespace.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct DataKindName(pub String);

/// Globally unique Semantics name per `11 §3`. Shared between Dimensions,
/// Measures, Metrics, Filters, and Keys across all DataKind scopes.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct SemanticsName(pub String);

/// Top-level `Relationship` identifier per `16 §2.1`. Author-declared
/// via `name:` on each relationship entry.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct RelationshipId(pub String);

/// Reference to a DataKind from inside a composition context. Either an
/// inline definition (hoisted at parse to top-level per `11 §10`) or a
/// by-name reference resolved at `compile`. Per `16 §2.2`.
#[non_exhaustive]
pub enum DataKindRef {
    /// Unresolved by-name reference.
    ByName(DataKindName),
    /// Inline-declared child; parse hoists it to the top-level
    /// `SemanticModel.data_kinds` table under a structural label (per
    /// `11 §10`) and leaves a `ByName` reference here.
    ///
    /// In v1 the `Inline` variant is transient: it never appears in a
    /// fully-parsed `SemanticModel` — the parser rewrites every `Inline`
    /// to a `ByName` before returning. Kept on the enum for future
    /// deferred-hoisting use (`[TD-INLINE-HOIST-LAZY]`).
    Inline(Box<DataKind>),
}
```

Construction of `DataKindName` / `SemanticsName` / `RelationshipId` is validated at `parse` (per `11 §3`'s identifier grammar — kebab-case, non-reserved). The newtype constructors are `pub(crate)` (tuple-struct access via `pub String` field is `pub`, but `from_str` / `new` constructors validate and are exposed only at parse sites). Read access via `.0` is unrestricted — consumers treat the `String` as an identifier token.

### 3.4 Serde derivations

Every public type in §3 derives `serde::Serialize` and `serde::Deserialize` under the `serde` feature (ON by default for `semstrait-model`; contrast with `31`, where it's OFF by default). The YAML shape ratified in §4–§9 is the primary author-facing surface; the JSON round-trip via `serde_json` is a side-effect of the same derives and is consumed by tooling (CLI `--format json` emission, LSP payloads).

**Non-roundtrip surface.** Certain parse-site fields carry post-parse hoisting metadata (§3.3 `DataKindRef::Inline`, the ephemeral `YamlRoot` scaffolding in `parse`) that is either rewritten pre-return or never serialized. `SemanticModel` itself is always roundtrip-safe: `parse(serialize(m)?)` produces a tree equal-under-ordering to `m` for every `m` produced by `parse`.

### 3.5 Deterministic ordering

See §12. Summary:

- `data_kinds: BTreeMap<DataKindName, DataKind>` — alphabetical by key.
- `relationships: Vec<Relationship>` — YAML author order.
- `semantics: SharedSemanticsRegistry` — each inner sub-map (`dimensions`, `measures`, ...) is a `BTreeMap<SemanticsName, _>`.
- Inside each DataKind variant, `SemanticInterface` fields (`dimensions`, `measures`, ...) are `BTreeMap<SemanticsName, _>`.
- Complex-variant `children: Vec<DataKindRef>` — YAML author order (per `22 §3.4` / `23 §3.4` / `24 §3.2`).

This is the exact ordering I4 requires to make `SemanticModel` byte-deterministic under a canonical serializer.

## 4. YAML Surface — Top-Level Blocks

### 4.1 Root shape

A model YAML file has exactly one root key: `semantic_model:`. Everything else is nested under it.

```yaml
semantic_model:
  name: analytics-v1
  description: "Primary analytics model for the order pipeline."
  namespace: warehouse-prod
  labels: [analytics, prod]

  data_kinds:
    # DataKind entries — see §5.
    - kind: simple
      name: orders
      # ...
    - kind: grainset
      name: revenue_by_day
      # ...

  relationships:
    # Relationship entries — see §7.
    - name: orders_to_customers
      from: orders
      to: customers
      # ...

  semantics:
    # Shared Semantics registry — see §6.7.
    dimensions:
      - name: region
        # ...

  functions:
    # Adapter-contributed extensions — see §4.5.
    - adapter: clickhouse
      # ...
```

Every top-level block is optional except `name:`. An empty model (no `data_kinds:`, no `relationships:`) parses successfully; a non-empty model with no DataKinds is a `ValidateError::EmptyModel` at `compile` per `31 §8.2`.

### 4.2 `data_kinds:` block

A flat list of `DataKind` entries, each tagged with a `kind:` discriminator:

| `kind:` value | Lowers to | Spec |
|---|---|---|
| `simple` (alias: `dataset`) | `DataKind::Simple(SimpleDataKind)` | `21` |
| `unionset` | `DataKind::Complex(ComplexDataKind::Unionset(...))` | `23` |
| `grainset` | `DataKind::Complex(ComplexDataKind::Grainset(...))` | `22` |
| `joinset` | `DataKind::Complex(ComplexDataKind::Joinset(...))` | `24` |

The `kind:` key is an **explicit tag** on every entry — no implicit kind detection from surrounding block shape. This is a deliberate divergence from the legacy code, which groups entries by top-level block name (`datasets:` / `unionsets:` / `grainsets:` / `joinsets:`) and infers kind from the containing list. See `[CODE-DIVERGES-FROM-SPEC]` at §15.

Each entry carries a `name:` field (a `DataKindName` per §3.3). Names are unique across the whole `data_kinds:` table per `11 §5.1`; duplicates raise `PARSE_E_0201 DuplicateDataKind` at parse.

### 4.3 `relationships:` block

A flat list of `Relationship` entries per `16 §2.1`. See §7 for the full shape.

### 4.4 `semantics:` block (shared registry)

A registry of reusable `Dimension` / `Measure` / `Metric` / `Filter` / `Key` definitions authored once at the model root and referenced via `ref:` from inside DataKind interfaces. Per `11 §5`.

```yaml
semantic_model:
  semantics:
    dimensions:
      - name: region
        data_type: string
        type:
          categorical:
            enum_values: [na, eu, apac]
    measures:
      - name: amount
        data_type: decimal(18, 2)
        agg: sum
```

Shape: `SharedSemanticsRegistry` holds one `BTreeMap<SemanticsName, _>` per carrier:

```rust
#[non_exhaustive]
pub struct SharedSemanticsRegistry {
    pub dimensions: BTreeMap<SemanticsName, Dimension>,
    pub measures:   BTreeMap<SemanticsName, Measure>,
    pub metrics:    BTreeMap<SemanticsName, Metric>,
    pub filters:    BTreeMap<SemanticsName, Filter>,
    pub keys:       BTreeMap<SemanticsName, Key>,
}
```

From inside a DataKind's `dimensions:` / `measures:` / ... block, a `ref:` entry names a member of this registry:

```yaml
data_kinds:
  - kind: simple
    name: orders
    dimensions:
      - ref: region           # resolves to semantics.dimensions.region at compile
      - name: order_id        # inline, not shared
        data_type: string
```

Refs resolve at `compile` (per `11 §7`); `32` only records the unresolved `Ref` entry on the DataKind. Unresolved refs raise `CompileError::UnresolvedSharedSemanticsRef` at compile time, not here.

### 4.5 `functions:` block (adapter extensions)

Rarely used at model scope. Authors declare adapter-contributed function specs:

```yaml
semantic_model:
  functions:
    - adapter: clickhouse
      name: quantile_bfloat16
      category: aggregate
      signatures:
        - args: [double]
          return: double
```

Shape: `Vec<FunctionExtension>`. Folded into the global `FunctionRegistry` at `compile` per `14a §7.1`; `32` just records the declaration. Collisions with core names raise `CompileError::AdapterFunctionShadowsCore` at compile, per `31 §8.3`.

```rust
#[non_exhaustive]
pub struct FunctionExtension {
    pub adapter: String,               // adapter identifier; matches `RegistryExtension::ADAPTER_ID`.
    pub name: String,                  // function name; validated against `is_reserved_tag`.
    pub category: FunctionCategory,    // re-exported from `semstrait-core` per `31 §5.7`.
    pub signatures: Vec<FnSignature>,  // re-exported from `semstrait-core` per `31 §5.4`.
    pub description: Option<String>,
}
```

Most authors leave `functions:` empty; the registry is populated through adapter-crate `RegistryExtension` impls at link time (per `31 §5.8`), not through model YAML. The `functions:` block exists for model-local overrides and author-facing documentation.

## 5. DataKind YAML by Variant

Each `DataKind` entry in `data_kinds:` carries a `kind:` discriminator and a payload. The parser dispatches on `kind:`:

```text
PARSE_DATA_KIND(entry):
    match entry["kind"]:
        "simple" | "dataset" → parse as SimpleDataKind     (§5.1)
        "grainset"           → parse as GrainsetSpec       (§5.2)
        "unionset"           → parse as UnionsetSpec       (§5.3)
        "joinset"            → parse as JoinsetSpec        (§5.4)
        other                → PARSE_E_0202 UnknownDataKind
```

### 5.1 `simple:` / `dataset:`

The flagship variant, per `21 §2.1`. An inline declaration of a queryable unit backed by one `Binding`.

```yaml
- kind: simple
  name: orders
  description: "Canonical orders fact."

  # SemanticInterface — §6.
  dimensions:
    - name: order_id
      data_type: string
    - name: ordered_at
      data_type: timestamp
    - name: region
      data_type: string
  measures:
    - name: amount
      data_type: decimal(18, 2)
      agg: sum
      expr: amount_cents / 100
  keys:
    - order_id

  # Binding — §8.
  binding:
    sources:
      - path: "s3://bucket/orders/year=*/month=*/*.parquet"
        format: parquet
    column_mapping:
      order_id:   order_id
      ordered_at: { column: ordered_at, grain: minute }
      region:     { metadata: { path: { token: 1 } } }
      amount_cents: amount_cents

  # TemporalShape — §9.
  temporal_shape:
    kind: timeseries
    occurred_at_dim: ordered_at
    grain: day

  # Structural declarations.
  grain: day                 # Optional; overrides temporal_shape.grain.
```

Lowers to:

```rust
#[non_exhaustive]
pub struct SimpleDataKind {
    pub name: DataKindName,
    pub description: Option<String>,
    pub ai_context: Option<AiContext>,
    pub interface: SemanticInterface,
    pub binding: Binding,
    pub temporal_shape: Option<TemporalShape>,
    pub grain: Option<Grain>,
}
```

Per `21 §2.1`. `kind: dataset` is accepted as a synonym for `kind: simple` (legacy ergonomics); both produce `DataKind::Simple(SimpleDataKind)`.

### 5.2 `grainset:`

Per `22 §3.1`. A set of alternate-grain children sharing a composed semantic surface.

```yaml
- kind: grainset
  name: paid_media_rollups
  grain_axis: report_date
  rollup_policy: shape_default    # one of: shape_default | pin_only | prefer_finest

  # SemanticInterface of the composed surface.
  dimensions:
    - name: report_date
      data_type: date
      type:
        temporal:
          grains: [day, week, month, quarter, year]
    - name: campaign_id
      data_type: string
  measures:
    - name: cost
      data_type: decimal(18, 4)
      agg: sum
    - name: clicks
      data_type: long
      agg: sum

  # Children — ordered; tie-break axis per `22 §4.5`.
  children:
    - ref: paid_media_daily_events
      grain: day
    - ref: paid_media_monthly_snapshot
      grain: month
      rollup_override: pin_only
```

Lowers to:

```rust
#[non_exhaustive]
pub struct GrainsetSpec {
    pub name: DataKindName,
    pub description: Option<String>,
    pub ai_context: Option<AiContext>,
    pub interface: SemanticInterface,
    pub grain_axis: SemanticsName,
    pub rollup_policy: RollupPolicy,
    pub children: Vec<GrainsetChild>,
}

#[non_exhaustive]
pub struct GrainsetChild {
    pub constituent: DataKindRef,
    pub grain: Option<Grain>,                  // None → inherit per `22 §3.2`.
    pub rollup_override: Option<RollupPolicy>,
}

#[non_exhaustive]
pub enum RollupPolicy { ShapeDefault, PinOnly, PreferFinest }
```

Per `22 §2.1` / `§3.1`. The per-child `coverage:` is **not** authored — it is compile-time-derived per `22 §3.3`.

### 5.3 `unionset:`

Per `23 §3.1`. A set of children UNION-ALLed into one composed surface.

```yaml
- kind: unionset
  name: paid_media
  mode: all                      # one of: all | distinct

  # SemanticInterface — authoritative; children do NOT declare Semantics (per `12 §3.3`).
  dimensions:
    - name: date
      data_type: date
    - name: source_platform
      data_type: string
    - name: campaign_id
      data_type: string
  measures:
    - name: cost
      data_type: decimal(18, 4)
      agg: sum

  children:
    - ref: adwords_daily
      coverage:
        provides: [date, campaign_id, cost]   # optional explicit override; `23 §3.2`.
    - ref: facebook_daily
    - ref: tiktok_rollup                       # a Grainset child (legal per `12 §2`).
```

Lowers to:

```rust
#[non_exhaustive]
pub struct UnionsetSpec {
    pub name: DataKindName,
    pub description: Option<String>,
    pub ai_context: Option<AiContext>,
    pub interface: SemanticInterface,
    pub mode: UnionMode,
    pub children: Vec<UnionsetChild>,
}

#[non_exhaustive]
pub enum UnionMode { All, Distinct }

#[non_exhaustive]
pub struct UnionsetChild {
    pub constituent: DataKindRef,
    pub coverage: Option<ChildCoverageOverride>,
}

#[non_exhaustive]
pub struct ChildCoverageOverride {
    pub provides: BTreeSet<SemanticsName>,
}
```

Per `23 §2.1`. Minimum-children (≥ 2) is a structural-validate check at parse per §11.2 `PARSE_E_0203 UnionsetTooFewChildren`; the canonical `VALID_E_2301` lives at the compile layer.

### 5.4 `joinset:`

Per `24 §2.2`. A named composition of two (v1-binary per `12 §5.2`) DataKinds joined along a declared path.

```yaml
- kind: joinset
  name: orders_with_customers
  anchor: orders                 # MUST be in `members:`; §3 of `24`.
  members:
    - orders
    - customers

  # SemanticInterface — Joinset-level additions only; per `24 §2.6`.
  dimensions:
    - name: order_country
      data_type: string
      expr: customers.country
  measures: []
  keys:
    - name: order_id
      owner: orders

  # Implicit path (`path:` absent) or explicit-path pinning:
  path:
    explicit:
      hops:
        - relationship: orders_to_customers
          direction: forward            # forward | reverse
          to: customers
  overrides:
    - position: 0
      join_type: left                  # per `16 §4` / `24 §5.3`.
```

Lowers to:

```rust
#[non_exhaustive]
pub struct JoinsetSpec {
    pub name: DataKindName,
    pub description: Option<String>,
    pub ai_context: Option<AiContext>,
    pub interface: SemanticInterface,
    pub anchor: DataKindRef,
    pub members: Vec<DataKindRef>,
    pub path: JoinPath,
    pub overrides: Vec<JoinTypeOverride>,
}

#[non_exhaustive]
pub enum JoinPath {
    Implicit,
    Explicit(ExplicitPath),
}

#[non_exhaustive]
pub struct ExplicitPath {
    pub hops: Vec<JoinHop>,
}

#[non_exhaustive]
pub struct JoinHop {
    pub relationship: RelationshipId,
    pub direction: HopDirection,
    pub to: DataKindRef,
}

#[non_exhaustive]
pub enum HopDirection { Forward, Reverse }

#[non_exhaustive]
pub struct JoinTypeOverride {
    pub position: u16,                       // index into `ExplicitPath.hops`.
    pub join_type: JoinType,                 // re-exported from §7 / `16 §4`.
}
```

Per `24 §2.2` / `§2.3`. The anchor-must-be-member check is a parse-stage structural validation per §11.2 `PARSE_E_0204 JoinsetAnchorNotMember`; the corresponding compile-stage invariant `COMP_E_2402` catches cases where the member list is modified post-parse (not author-reachable in v1).

## 6. Semantics YAML

Every DataKind carries a `SemanticInterface` block — the five `Semantics` carriers (`11 §2`). The shape is uniform across Simple and Complex kinds.

### 6.1 `SemanticInterface` layout

```rust
#[non_exhaustive]
pub struct SemanticInterface {
    pub description: Option<String>,
    pub ai_context: Option<AiContext>,
    pub dimensions: BTreeMap<SemanticsName, Dimension>,
    pub measures: BTreeMap<SemanticsName, Measure>,
    pub metrics: BTreeMap<SemanticsName, Metric>,
    pub filters: BTreeMap<SemanticsName, Filter>,
    pub keys: BTreeMap<SemanticsName, Key>,
}
```

Every carrier map is keyed by `SemanticsName` for alphabetical ordering (I4). Author order within each carrier is **not** preserved — semantic equivalence is name-based per `11 §3`.

YAML author declares each carrier as a list; the parser converts to a `BTreeMap`, raising `PARSE_E_0205 DuplicateSemanticsName` on duplicate keys within the same carrier.

### 6.2 `dimensions:`

Per `11 §2.1` / `13 §4`. A dimension carries a name, a logical `DataType`, and a `DimensionType` discriminator:

```yaml
dimensions:
  - name: ordered_at
    data_type: timestamp
    type:
      temporal:
        grains: [minute, hour, day, week, month, quarter, year]

  - name: region
    data_type: string
    type:
      categorical:
        enum_values: [na, eu, apac]

  - name: is_active
    data_type: bool
    type:
      binary: {}

  - name: source_kind
    type:
      metadata:
        path: { token: 2 }             # metadata dimension — `15 §5.3` / `13 §4.5`.

  - name: country_hash
    type:
      bucketed:
        buckets:
          - { name: "0-9",   low: 0, high: 9 }
          - { name: "10-99", low: 10, high: 99 }

  - name: market_share
    data_type: double
    expr: shipped_orders / total_orders       # derived dimension per `11 §2.1`.
```

Lowers to:

```rust
#[non_exhaustive]
pub struct Dimension {
    pub name: SemanticsName,
    pub description: Option<String>,
    pub data_type: Option<DataType>,      // None → inferred at compile.
    pub ai_context: Option<AiContext>,
    pub dim_type: DimensionType,
    pub expr: Option<ExprSource>,         // None → binding-backed via ColumnMapping.
}

#[non_exhaustive]
pub enum DimensionType {
    Temporal(TemporalDimension),
    Categorical(CategoricalDimension),
    Binary(BinaryDimension),
    Geo(GeoDimension),
    Bucketed(BucketedDimension),
    Metadata(MetadataDimension),
}
```

Per `13 §4.1`–`§4.6`. `expr:` is an `ExprSource` — either an inline DSL string or a Declarative block (per `14 §4`). At parse sites on `Dimension`, `expr:` lowers to a `SemanticExpr` (via `ExprSource::parse_semantic`) because derived Dimensions may reference other DataKinds (`EntityRef`) per `14 §2.2`.

### 6.3 `measures:`

Per `11 §2.2`. Always aggregatable; carries `agg:` and optional `constraints:`:

```yaml
measures:
  - name: amount_sum
    data_type: decimal(18, 2)
    agg: sum
    expr: amount_cents / 100                # optional; default is identity on `name`.

  - name: order_count
    agg: count
    expr: "*"                                # Count-all literal per `14a §4`.

  - name: distinct_customers
    agg: count
    distinct: true                            # lowers to Aggregate { Count, distinct: true }.
    expr: customer_id

  - name: revenue
    data_type: decimal(18, 2)
    agg: sum
    additivity: additive                      # additive | semi_additive | non_additive — `11 §6`.
    constraints:
      dimensions:
        all: [region]                         # must group by region to be valid — `11 §8.4.1`.
      aggregations:
        allowed: [SUM, COUNT]
        prohibited: [MIN, MAX]
```

Lowers to:

```rust
#[non_exhaustive]
pub struct Measure {
    pub name: SemanticsName,
    pub description: Option<String>,
    pub data_type: Option<DataType>,
    pub ai_context: Option<AiContext>,
    pub agg: Aggregation,                     // re-exported from `semstrait-core` per `31 §3.5`.
    pub distinct: bool,                       // default false.
    pub expr: Option<ExprSource>,             // default: identity on name.
    pub additivity: Option<Additivity>,       // default: Additive (`11 §6`).
    pub constraints: MeasureConstraints,      // re-exported from `semstrait-core` per `31 §6.1`.
}

#[non_exhaustive]
pub enum Additivity {
    Additive,
    SemiAdditive { across: Vec<SemanticsName> },   // semi-additive axes per `11 §6.2`.
    NonAdditive,
}
```

`expr:` on a Measure lowers to `SemanticExpr` (it may reference another DataKind via `EntityRef` for a metric-like cross-kind formula). `constraints:` is re-exported from `semstrait-core`'s `MeasureConstraints` per `31 §6.1` — same type, same YAML shape, `32` is the parse site.

### 6.4 `metrics:`

Per `11 §2.3`. A Metric is a computed expression over other Semantics (Measures, Dimensions, other Metrics). Distinct from a Measure in that it does not carry its own `agg:` — aggregation is expressed inside its `expr:`.

```yaml
metrics:
  - name: avg_order_value
    data_type: decimal(18, 2)
    expr: sum(amount) / count(orders)
    additivity: non_additive

  - name: conversion_rate
    expr:
      binary_op:
        op: divide
        left:
          aggregate:
            function: sum
            expr: converted_flag
        right:
          aggregate:
            function: count
            expr: "*"
    constraints:
      dimensions:
        none_of: [time_of_day]                # explicitly forbid time-of-day groupings.
```

Lowers to:

```rust
#[non_exhaustive]
pub struct Metric {
    pub name: SemanticsName,
    pub description: Option<String>,
    pub data_type: Option<DataType>,
    pub ai_context: Option<AiContext>,
    pub expr: ExprSource,                           // REQUIRED for a Metric.
    pub additivity: Option<Additivity>,
    pub constraints: MeasureConstraints,
}
```

`expr:` lowers to `SemanticExpr`. Per `14 §2.2`, a Metric's `expr:` MAY contain `Aggregate` nodes at any depth; a Measure's `expr:` MUST NOT (the outer `agg:` is the unique aggregation site on a Measure).

### 6.5 `filters:`

Per `11 §2.4`. A named predicate over the DataKind's Semantics:

```yaml
filters:
  - name: active_only
    expr: is_active = true

  - name: recent_30d
    expr: ordered_at >= date_sub(current_date(), interval 30 day)
```

Lowers to:

```rust
#[non_exhaustive]
pub struct Filter {
    pub name: SemanticsName,
    pub description: Option<String>,
    pub expr: ExprSource,
    pub ai_context: Option<AiContext>,
}
```

`expr:` lowers to `SemanticExpr`. Filters are Boolean-valued; the type check lives at `compile` (`14 §7.3` `CaseConditionNotBoolean` analogue: `CompileError::FilterExprNotBoolean`).

### 6.6 `keys:`

Per `11 §2.5` / `16 §6.5`. A key is a named grouping of Semantics that uniquely identifies a row on the DataKind (or composition):

```yaml
keys:
  # Shorthand form — single Semantics name treated as a single-column key.
  - order_id

  # Long form — multi-column key.
  - name: order_line_pk
    columns: [order_id, line_number]

  # Composition-anchored key on a Joinset.
  - name: order_id
    owner: orders                              # `16 §6.5` — echoes anchor's key.
```

Lowers to:

```rust
#[non_exhaustive]
pub struct Key {
    pub name: SemanticsName,
    pub columns: Vec<SemanticsName>,            // length ≥ 1.
    pub owner: Option<DataKindRef>,             // Some(...) on composed surfaces per `16 §6.5`.
}
```

The shorthand form (`- order_id`) is equivalent to `{ name: order_id, columns: [order_id] }`. Duplicate key names within a DataKind raise `PARSE_E_0205 DuplicateSemanticsName`; empty `columns:` is `PARSE_E_0206 KeyColumnsEmpty`.

### 6.7 Shared `semantics:` registry — `ref:` form

Inside any DataKind's `dimensions:` / `measures:` / `metrics:` / `filters:` / `keys:` block, an entry may be a `ref:` rather than an inline definition:

```yaml
data_kinds:
  - kind: simple
    name: orders
    dimensions:
      - ref: region             # resolves to semantic_model.semantics.dimensions.region.
      - name: order_id          # inline.
        data_type: string
```

Lowers to a tagged entry in the carrier:

```rust
#[non_exhaustive]
pub enum DimensionEntry {
    Inline(Dimension),
    Ref(SemanticsName),
}
// Equivalent carrier-enum wrappers exist for Measure / Metric / Filter / Key.
```

Unresolved refs are a `compile` concern (`CompileError::UnresolvedSharedSemanticsRef`), not parse. `32`'s job is to record the `Ref` and leave resolution for `33`.

## 7. `Relationship` YAML

Per `16 §2–§5`. Top-level `relationships:` block; each entry is a declarative pairwise connector between two DataKinds.

### 7.1 Entry shape

```yaml
relationships:
  - name: orders_to_customers
    from: orders
    to: customers
    keys:
      - { left: customer_id, right: customer_id }
    cardinality: many_to_one
    join_type: left
    directionality: bidirectional
```

Lowers to:

```rust
#[non_exhaustive]
pub struct Relationship {
    pub id: RelationshipId,
    pub from: DataKindRef,
    pub to: DataKindRef,
    pub keys: Vec<KeyPair>,
    pub cardinality: Cardinality,
    pub join_type: JoinType,
    pub directionality: Directionality,
}

#[non_exhaustive]
pub struct KeyPair {
    pub left: SemanticsName,
    pub right: SemanticsName,
}
```

Per `16 §2.1`. `name:` on the YAML entry lowers to `id: RelationshipId`.

### 7.2 `cardinality:`

```rust
#[non_exhaustive]
pub enum Cardinality {
    OneToOne,
    OneToMany,
    ManyToOne,
    ManyToMany,
}
```

YAML spellings (snake_case): `one_to_one`, `one_to_many`, `many_to_one`, `many_to_many`. Per `16 §3.1`.

### 7.3 `join_type:`

```rust
#[non_exhaustive]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    // `AsOf` ratified at vocabulary level in `17 §4`; implementation deferred.
    // Adding the variant would be source-breaking on exhaustive matches;
    // because the enum is #[non_exhaustive] it is MINOR per `30 §2`.
}
```

YAML spellings (snake_case): `inner`, `left`, `right`, `full`. Per `16 §4`. The `as_of` spelling is **reserved** per `17 §4`; parsing it in v1 raises `PARSE_E_0207 ReservedJoinTypeAsOf`.

### 7.4 `directionality:`

```rust
#[non_exhaustive]
pub enum Directionality { Bidirectional, Forward }
```

YAML spellings: `bidirectional`, `forward`. Default `bidirectional`. Per `16 §2.4`.

### 7.5 `keys:`

A non-empty `Vec<KeyPair>`. Each pair names a Semantics from the `from:` kind's interface and a Semantics from the `to:` kind's interface. At parse, `32` only validates non-emptiness (`PARSE_E_0208 RelationshipKeysEmpty`); resolution against the referenced DataKinds' interfaces is `compile` (`CompileError::UnresolvedRelationshipKey`).

### 7.6 Round-1 cross-reference — `16 §5`'s `ComposedSemanticInterface`

`ComposedSemanticInterface` / `UnifiedSemantics` / `FieldProvenance` / `CompositionCoverage` are **not** authored. They are compile-time outputs of `16 §6`'s merge algorithm. `32` exposes them only transitively through the re-export of `33`'s types at a later stage; `32` itself does not mention them in its YAML surface.

## 8. `ColumnMapping` / `Binding` YAML

Per `15 §3–§8`. Authored on every `SimpleDataKind` under a `binding:` block.

### 8.1 `binding:` block shape

```yaml
binding:
  sources:
    - path: "s3://bucket/orders/year=*/month=*/*.parquet"
      format: parquet
    - path: "s3://bucket/orders/archive.parquet"
      format: parquet

  column_mapping:
    order_id:    order_id
    ordered_at:  { column: ordered_at, grain: minute }
    region:      { metadata: { path: { token: 1 } } }
    amount_cents: amount_cents
    gross_amount: { computed: "amount_cents / 100" }
    is_test:     { lit: false }
```

Lowers to:

```rust
#[non_exhaustive]
pub struct Binding {
    pub sources: Vec<PhysicalSource>,
    pub column_mapping: ColumnMapping,
    // `coverage:` is compile-derived per `15 §6`; not authored.
}
```

Per `15 §3.1`. `Binding` carries no further author-facing fields beyond `sources:` and `column_mapping:` in v1.

### 8.2 `PhysicalSource` — `sources:` entries

```rust
#[non_exhaustive]
pub enum PhysicalSource {
    File     { path: String, format: FileFormat,    options: BTreeMap<String, String> },
    Table    { catalog: Option<String>, schema: Option<String>, name: String },
    Snapshot { snapshot_id: String, source_ref: Box<PhysicalSource> },
}

#[non_exhaustive]
pub enum FileFormat { Parquet, Csv, Json, Orc, Avro, Delta, Iceberg }
```

Per `15 §3.2`. YAML discriminator is key-based:

```yaml
sources:
  # File
  - path: "s3://..."
    format: parquet
    options:
      compression: snappy

  # Table
  - table: orders
    schema: public
    catalog: warehouse

  # Snapshot (wraps a File or Table)
  - snapshot_id: "2024-01-15T00:00:00Z"
    source:
      path: "s3://bucket/orders/*.parquet"
      format: parquet
```

The `path:` vs `table:` vs `snapshot_id:` top-level key selects the variant. Mutually exclusive; mixing raises `PARSE_E_0209 PhysicalSourceAmbiguous`.

### 8.3 `ColumnMapping` — per-Semantics value

```rust
#[non_exhaustive]
pub struct ColumnMapping(pub BTreeMap<SemanticsName, ColumnMappingValue>);

#[non_exhaustive]
pub enum ColumnMappingValue {
    /// Direct physical column.
    Column     { column: String, grain: Option<Grain> },
    /// Constant literal injected as a column.
    Literal    { value: LiteralValue },
    /// Computed from a physical-layer expression.
    Computed   { expr: PhysicalExpr },          // authored pre-compile via `PhysicalExpr::new_authored`.
    /// Source-metadata extraction (path tokens, partition values, etc.).
    Metadata   { spec: MetadataSpec },
}

#[non_exhaustive]
pub enum MetadataSpec {
    /// Token indexed from the file path after glob-segment split. `15 §5.3`.
    PathToken { token: usize },
    /// Partition column value from Hive-style partitioning. `15 §5.3`.
    Partition { column: String },
    /// Snapshot metadata (e.g. snapshot_id, snapshot_timestamp).
    Snapshot  { field: String },
}
```

Per `15 §5`. YAML spellings:

```yaml
column_mapping:
  # Column — string shorthand or { column: ..., grain: ... }.
  order_id: order_id
  ordered_at: { column: ordered_at, grain: minute }

  # Literal.
  is_test: { lit: false }

  # Computed.
  gross_amount: { computed: "amount_cents / 100" }

  # Metadata.
  source_region: { metadata: { path: { token: 1 } } }
  part_date:     { metadata: { partition: order_date } }
  snap_ts:       { metadata: { snapshot: snapshot_timestamp } }
```

**`Computed { expr }` lowers to `PhysicalExpr`** (not `SemanticExpr`). At this site, `Expr::Column` leaves are legal (the expression runs inside a Binding, per `14 §2.3`) but `Expr::EntityRef` / `Expr::Aggregate` are forbidden — `PhysicalExpr::new_authored` enforces the check and raises `ValidateError::EntityRefInPhysicalExpr` / `ValidateError::AggregateInPhysicalExpr` per `31 §8.2`.

### 8.4 Multi-source `coverage:` is compile-derived

Per `15 §6`, `Coverage` records per-source × per-Semantics truth-table entries (`Native` / `NullFill` / `Derived`). It is **not** authored — the author writes `sources:` (possibly with differing subsets of `column_mapping:` keys across sources via glob expansion) and `15 §6`'s fold computes the `Coverage` at `compile`. `32` does not expose a `coverage:` YAML key on `Binding`.

## 9. `TemporalShape` YAML

Per `17 §2`. Authored on a `SimpleDataKind`'s `temporal_shape:` block. Optional — a Simple with no temporal axis omits the block.

### 9.1 Block discriminator

```yaml
temporal_shape:
  kind: timeseries               # timeseries | events | snapshot | scd
  # ... variant-specific payload.
```

Lowers to:

```rust
#[non_exhaustive]
pub enum TemporalShape {
    Timeseries { occurred_at_dim: SemanticsName, grain: Grain },
    Events     { occurred_at_dim: SemanticsName },
    Snapshot   { snapshotted_at_dim: SemanticsName, cadence: Option<Grain> },
    Scd        { subtype: ScdSubtype },
}
```

Per `17 §2.1`.

### 9.2 `timeseries`

```yaml
temporal_shape:
  kind: timeseries
  occurred_at_dim: ordered_at
  grain: day
```

`occurred_at_dim` MUST name a Temporal Dimension on the SimpleDataKind's interface; `grain` MUST be present in that Dimension's `grains:` list. Both checks are compile-stage (`COMP_E_1701`); parse only validates the field shape.

### 9.3 `events`

```yaml
temporal_shape:
  kind: events
  occurred_at_dim: event_ts
```

No grain — events are the finest-grain case per `17 §2.1.2`.

### 9.4 `snapshot`

```yaml
temporal_shape:
  kind: snapshot
  snapshotted_at_dim: as_of
  cadence: day                   # optional.
```

`cadence:` is the author's assertion of how often the snapshot is refreshed; the planner consults it for rollup legality per `22 §4.3` / `17 §5`.

### 9.5 `scd`

```yaml
temporal_shape:
  kind: scd
  subtype:
    type2:
      valid_from_dim: valid_from
      valid_to_dim: valid_to
      current_flag_dim: is_current   # optional.
```

Lowers to:

```rust
#[non_exhaustive]
pub enum ScdSubtype {
    Type0,
    Type1,
    Type2 {
        valid_from_dim: SemanticsName,
        valid_to_dim: SemanticsName,
        current_flag_dim: Option<SemanticsName>,
    },
    Type3 { prior_value_dim: SemanticsName },
    Type4 { history_data_kind_ref: DataKindRef },
    Type5 {
        valid_from_dim: SemanticsName,
        valid_to_dim: SemanticsName,
        mini_dim_ref: DataKindRef,
    },
    Type6 {
        valid_from_dim: SemanticsName,
        valid_to_dim: SemanticsName,
        current_flag_dim: Option<SemanticsName>,
        current_value_dim: Option<SemanticsName>,
    },
}
```

Per `17 §2.2`. YAML spellings use the snake_case variant names: `type0`, `type1`, `type2`, `type3`, `type4`, `type5`, `type6`. The discriminator is nested under `subtype:` because variants carry per-subtype payload shapes; a flat discriminator would require a 7-way union of fields at the same level, which YAML handles poorly.

### 9.6 `AsOfAnchor`

Per `17 §3`, `AsOfAnchor` (for `JoinType::AsOf`) is ratified but NOT YAML-authoring-surface in v1. The `as_of` `join_type:` spelling raises `PARSE_E_0207 ReservedJoinTypeAsOf` per §7.3.

## 10. The `parse` API

### 10.1 Signature

```rust
/// YAML `&str` → `SemanticModel`. Pure and synchronous.
///
/// # Contract
/// - Input is a YAML document with a single `semantic_model:` root key (§4.1).
/// - `${VAR}` substitutions are expanded from the process environment
///   before YAML deserialization (§10.2).
/// - Structural validation happens inline: duplicate DataKinds / Semantics
///   / Relationships, required-field presence, `kind:` discriminator
///   completeness, `PhysicalSource` discriminator uniqueness, minimum
///   child counts for composite variants.
/// - Name resolution, type inference, Semantics-scope lookup, composition
///   materialization — all DEFERRED to `semstrait-manifest::compile`.
///
/// # Errors
/// Accumulates into `ParseErrors`. Returns `Err(ParseErrors)` on the first
/// error that prevents further structural parsing; for recoverable shape
/// errors the parser continues and collects every diagnostic before
/// returning. See §10.4 for the streaming-vs-one-shot detail.
pub fn parse(input: &str) -> Result<SemanticModel, ParseErrors>;
```

One entry point. No options, no context, no engine / catalog handle. `parse` is deliberately stateless — the same `&str` always produces the same `SemanticModel` (I4).

### 10.2 `${VAR}` substitution

Per current-code behavior (validated by `substitute_env_vars` in `parse.rs`). Before YAML deserialization, the parser rewrites every `${IDENT}` token in the input string to the value of the environment variable `IDENT`. Unset variables raise `ParseError::UnsetEnvVar`.

```rust
pub(crate) fn substitute_env_vars(input: &str) -> Result<String, ParseError>;
```

Not part of the public surface — always invoked transitively through `parse`. The function is `pub(crate)` to support unit tests within `semstrait-model` only; consumers who need env-var substitution at a different granularity should wrap `parse` themselves.

Bare `$VAR` syntax is NOT supported; only `${VAR}`. Identifier grammar follows standard Unix env-var rules (letters, digits, underscores, not starting with digit).

### 10.3 Multi-file loading — out of scope

`parse` takes a single `&str`. Reading multiple YAML files, walking a directory, resolving `$include:` directives — all are out of scope for `semstrait-model` in v1.

Rationale: I11 keeps I/O out of the shared-types path. A CLI (`semstrait-cli`) or a language server (`semstrait-lsp`) composes its own file-loading logic on top of `parse`. The composite-loader helper (`parse_dir(path: &Path) -> Result<SemanticModel, ...>`) is deferred as `[TD-MODEL-DIR-LOADER]` — see open question `Q-MODEL-001`.

### 10.4 Streaming vs one-shot

`parse` is **one-shot**: it consumes the full input string, produces a full `SemanticModel`, and returns. There is no incremental parse API in v1.

`serde_yaml` under the hood is streaming at the YAML level (it processes the document as events), but the resulting `SemanticModel` is materialized whole. The streaming event pipeline is not exposed because:

- Author-facing models are small (thousands of lines at the outer limit; most are < 500 lines).
- A partial `SemanticModel` has no well-defined semantics — the nesting rules in `12` are global, and a half-parsed tree cannot be validated meaningfully.
- Incremental re-parse (for LSP / editor use) is a `semstrait-lsp` concern; the right shape for it is a diff-aware re-run of `parse` on the changed file, not a streaming extension of `parse` itself.

### 10.5 Thread-safety and reentrancy

`parse` is `Send + Sync` at the signature level (it's a free function). It is reentrant — multiple threads may call `parse` concurrently with different inputs and receive independent `SemanticModel` values. No process-global state is consulted or mutated beyond the environment-variable read in `substitute_env_vars`, which is itself `Send + Sync` per `std::env::var`.

## 11. `ParseError` / `ValidateError`

### 11.1 `ParseError`

Owned by `semstrait-model`. `#[non_exhaustive]` per I10. Variants below aggregate §4–§9's parse-stage failures:

```rust
#[non_exhaustive]
pub enum ParseError {
    // -- YAML surface (PARSE_E_01xx) --
    YamlSyntax                 { message: String, location: Option<Location> },
    UnsetEnvVar                { var: String,     location: Option<Location> },
    MalformedRoot              { reason: String,  location: Option<Location> },
    UnknownTopLevelBlock       { block: String,   location: Option<Location> },

    // -- DataKind shape (PARSE_E_02xx) --
    DuplicateDataKind          { name: DataKindName, occurrences: Vec<Location> },
    UnknownDataKind            { kind: String,    name: Option<DataKindName>, location: Option<Location> },
    UnionsetTooFewChildren     { name: DataKindName, child_count: usize, location: Option<Location> },
    JoinsetAnchorNotMember     { name: DataKindName, anchor: DataKindRef,    location: Option<Location> },
    DuplicateSemanticsName     { carrier: String, name: SemanticsName, container: DataKindName, occurrences: Vec<Location> },
    KeyColumnsEmpty            { name: SemanticsName, location: Option<Location> },
    ReservedJoinTypeAsOf       { location: Option<Location> },
    RelationshipKeysEmpty      { name: RelationshipId, location: Option<Location> },
    PhysicalSourceAmbiguous    { binding_of: DataKindName, keys_present: Vec<String>, location: Option<Location> },

    // -- Expression surface (PARSE_E_03xx) --
    MalformedExprSource        { site: String, reason: String, location: Option<Location> },
    ReservedTagInExprBlock     { tag: String, site: String, location: Option<Location> },
    DslParseFailure            { message: String, location: Option<Location> },

    // -- Type / grain YAML (PARSE_E_04xx) --
    InvalidDataType            { raw: String,    location: Option<Location> },
    InvalidDecimalParameters   { precision: u32, scale: i32, location: Option<Location> },
    InvalidGrainLiteral        { raw: String,    location: Option<Location> },

    // -- Temporal shape (PARSE_E_05xx) --
    TemporalShapeKindUnknown   { raw: String, location: Option<Location> },
    ScdSubtypeMalformed        { subtype: String, reason: String, location: Option<Location> },
}

impl ParseError {
    pub fn code(&self) -> &'static str;         // kebab-case derivation per `31 §7.4`.
    pub fn severity(&self) -> Severity;         // Severity::Error for every v1 variant.
    pub fn location(&self) -> Option<&Location>;
}

impl IntoDiagnostic for ParseError { fn into_diagnostic(self) -> Diagnostic; }
impl std::fmt::Display for ParseError { /* per-variant messages */ }
impl std::error::Error for ParseError {}
```

**Stable codes.** Per `30 §6`, every variant maps to a kebab-case `code()` of the form `"parse.<variant-kebab>"` — e.g. `"parse.duplicate-data-kind"`, `"parse.unionset-too-few-children"`. The numeric `PARSE_E_01xx` / `PARSE_E_02xx` / ... grouping in the comments is documentation-only (matches the grouping scheme described in `30 §6` and `31 §8.3`); the authoritative code surface is kebab-case.

**Accumulation.** A single `parse` invocation may emit multiple `ParseError`s:

```rust
#[non_exhaustive]
pub struct ParseErrors {
    pub errors: Vec<ParseError>,
}

impl IntoDiagnostic for ParseErrors {
    fn into_diagnostic(self) -> Diagnostic {
        // First error is the primary `Diagnostic`; remaining errors
        // become `source_chain` entries. Rationale: `IntoDiagnostic` returns
        // a single `Diagnostic`; the surface is designed to handle one
        // typed error at a time, with chained context being supplemental
        // rather than parallel. Callers that want a per-error list use
        // `self.errors.into_iter().map(IntoDiagnostic::into_diagnostic)`.
    }
}
```

Recoverability policy:

- **Recoverable.** Per-DataKind shape errors (`DuplicateSemanticsName`, `UnionsetTooFewChildren`, `JoinsetAnchorNotMember`) — the parser records the error, skips the offending entry, and continues. The returned `SemanticModel` is never `Err`-wrapped when any entry parsed successfully **only if** no YAML-level syntax failure occurred.
- **Fatal.** YAML syntax errors, malformed root (`MalformedRoot`, `YamlSyntax`) — the parser cannot proceed; returns `Err(ParseErrors)` with the single fatal error.

The line between recoverable and fatal is drawn conservatively: any structural error that prevents downstream `compile` from running is recoverable at parse; any error that prevents `parse` from producing a structurally-valid tree is fatal.

### 11.2 `ValidateError` share raised by `semstrait-model`

`ValidateError` itself lives in `semstrait-core` per `31 §8.2`. `semstrait-model` raises a subset at parse-stage boundaries (wrapper construction on `SemanticExpr` / `PhysicalExpr`; DSL parse failures lowered through `ExprSource::parse_semantic` / `::parse_physical`):

| `ValidateError` variant | Parse-stage site | Caused by |
|---|---|---|
| `ColumnInSemanticExpr` | `Dimension.expr`, `Measure.expr`, `Metric.expr`, `Filter.expr`, `Joinset.interface.dimension.expr` | Author wrote a physical column name where a `SemanticExpr` is required. |
| `EntityRefInPhysicalExpr` | `ColumnMapping::Computed.expr` | Author wrote an `EntityRef` inside a binding-layer computed column. |
| `AggregateInPhysicalExpr` | `ColumnMapping::Computed.expr` | Author wrote an `Aggregate` inside a binding-layer computed column. |
| `NestedAggregate` | `Measure.expr`, `Metric.expr` | `Aggregate(..., Aggregate(...))` nesting in a `SemanticExpr`. |
| `ReservedIdentifier` | any Semantics `name:` field | Name collides with a reserved identifier (`14 §4.4.1` tags). |
| `CaseConditionNotBoolean` | any `Case` / `When` inside an `ExprBlock` | Syntactic shape check pre-type-inference. |
| `InvalidGrainValue` | `Dimension.type.temporal.grains`, `SimpleDataKind.grain`, `TemporalShape.grain` | Grain literal not in `13 §3.1`'s enum. |

Other `ValidateError` variants (`DimensionTypeMalformed`, `BucketsOverlap`, `ShapeMalformed`, `ShapeFieldConflict`) are also raised at parse when their structural preconditions are authorable in YAML. Type-inference-dependent variants (`ComputedTypeUnifyConflict`, `ShapeInferenceConflict`) are compile-stage only.

### 11.3 Code-range allocation

Per `30 §6`'s subsystem allocation, `semstrait-model` owns the `PARSE_E_*` range in full. Round-1 assignments:

| Range | Category | Variants (this doc) |
|---|---|---|
| `PARSE_E_01xx` | YAML surface | `YamlSyntax`, `UnsetEnvVar`, `MalformedRoot`, `UnknownTopLevelBlock` |
| `PARSE_E_02xx` | DataKind shape | `DuplicateDataKind`, `UnknownDataKind`, `UnionsetTooFewChildren`, `JoinsetAnchorNotMember`, `DuplicateSemanticsName`, `KeyColumnsEmpty`, `ReservedJoinTypeAsOf`, `RelationshipKeysEmpty`, `PhysicalSourceAmbiguous` |
| `PARSE_E_03xx` | Expression surface | `MalformedExprSource`, `ReservedTagInExprBlock`, `DslParseFailure` |
| `PARSE_E_04xx` | Type / grain | `InvalidDataType`, `InvalidDecimalParameters`, `InvalidGrainLiteral` |
| `PARSE_E_05xx` | Temporal shape | `TemporalShapeKindUnknown`, `ScdSubtypeMalformed` |

Per Q-MODEL-002 (see open questions), the numeric-code mapping is secondary; the primary code is the kebab-case derivation per `31 §8.3`. The numeric scheme is preserved for tooling that grew against the legacy `EXPR_E_####` numeric shape.

## 12. Deterministic Ordering (I4 Guarantee)

### 12.1 The invariant

Given the same input `&str`, `parse` produces a `SemanticModel` that is byte-identical under any canonical serializer (`serde_yaml::to_string`, `serde_json::to_string`). This is the concrete I4 guarantee `semstrait-model` upholds — the `Manifest` further down the pipe inherits it because `33`'s `compile` is deterministic given a deterministic `SemanticModel`.

### 12.2 Collection-ordering rules

| Collection | Type | Ordering rule |
|---|---|---|
| `SemanticModel.data_kinds` | `BTreeMap<DataKindName, DataKind>` | Alphabetical by name. |
| `SemanticModel.relationships` | `Vec<Relationship>` | YAML author order. Relationships have a `name:` but author order carries the semantic intent of "first-match wins" for ambiguous implicit paths (`16 §11`). |
| `SemanticModel.semantics.*` | `BTreeMap<SemanticsName, _>` | Alphabetical by name. |
| `SemanticModel.labels` | `Vec<String>` | YAML author order. |
| `SemanticModel.functions` | `Vec<FunctionExtension>` | YAML author order; within an adapter, sorting is `33`'s responsibility. |
| `DataKind::Simple(SimpleDataKind).interface.dimensions` (and `measures`, `metrics`, `filters`, `keys`) | `BTreeMap<SemanticsName, _>` | Alphabetical by name. |
| `GrainsetSpec.children` | `Vec<GrainsetChild>` | YAML author order (tie-break axis per `22 §4.5`). |
| `UnionsetSpec.children` | `Vec<UnionsetChild>` | YAML author order (column-reconciliation first-wins axis per `23 §3.4`). |
| `JoinsetSpec.members` | `Vec<DataKindRef>` | YAML author order (anchor-first canonical form per `24 §8.1`). |
| `JoinsetSpec.path::Explicit(ExplicitPath).hops` | `Vec<JoinHop>` | YAML author order (the path IS the ordered walk). |
| `Binding.sources` | `Vec<PhysicalSource>` | YAML author order (multi-source fold reads in this order per `15 §6.2`). |
| `ColumnMapping.0` | `BTreeMap<SemanticsName, ColumnMappingValue>` | Alphabetical by key. |
| `AggregationConstraints.allowed` / `.prohibited` | `Vec<String>` | YAML author order preserved — `31 §6.3` matches tokens against these lists; order is semantic only under tie-break scenarios, but for I4 we keep author order. |
| `DimensionConstraints.one_of` / `.none_of` / `.all` | `Vec<String>` | Same — YAML author order. |

### 12.3 Rationale — why `BTreeMap` for named entities, `Vec` for positional / declaration-ordered entities

- **`BTreeMap<Name, T>`**: entities whose identity IS their name (DataKinds, Semantics, shared registry entries). Semantic equivalence under reordering — two models with the same entities in different YAML order produce the same `BTreeMap` — making I4 effortless and making `compile`-stage diffs tractable.
- **`Vec<T>`**: entities whose identity is their position or declaration order (relationships — first-match wins on implicit paths; Grainset / Unionset / Joinset children — tie-break axis / column-reconciliation ordering / anchor-first path ordering). Reordering these changes the model's semantics, so we preserve the author's intent directly.

The rule is applied consistently: any time author order carries semantic information, the collection is a `Vec`; otherwise it is a `BTreeMap`. `32`'s in-memory representation never holds a `HashMap` on any public field — `HashMap` introduces hash-randomized iteration order, which is fatal to I4 on JSON / YAML round-trip.

### 12.4 Field ordering within structs

Serde's default field-serialization order follows struct declaration order. `32` freezes struct field order at ratification; adding a new `pub` field is a MINOR change per `30 §2` but appends at the end of the struct to preserve serialization-output ordering. Re-arranging existing fields is a MAJOR change per `30 §2` and is explicitly rejected for `SemanticModel` v1.

### 12.5 Failure mode — `HashMap` in public surface

An `integration-test` over `cargo public-api` output enforces "no `HashMap<_, _>` in any public field reachable from `SemanticModel`." Violation is a CI failure, not a code-review catch. The guard extends to transitively reachable types in `semstrait-core` (`ColumnMapping`, `ChildCoverageOverride`, etc.); `semstrait-core`'s share of the check runs inside `31`'s test suite.

## 13. Stability

### 13.1 Stable surface

Per `30 §2`, the following are **stable** under v1 semver:

- The `SemanticModel` struct and every public field on it.
- Every exported DataKind variant struct (`SimpleDataKind`, `UnionsetSpec`, `GrainsetSpec`, `JoinsetSpec`) and every public field.
- Every exported Semantics carrier (`Dimension`, `Measure`, `Metric`, `Filter`, `Key`) and every public field.
- `Relationship`, `KeyPair`, `Cardinality`, `JoinType`, `Directionality`.
- `Binding`, `PhysicalSource`, `FileFormat`, `ColumnMapping`, `ColumnMappingValue`, `MetadataSpec`.
- `TemporalShape`, `ScdSubtype`, `AsOfAnchor`.
- The `parse` free function signature.
- The `ParseError` / `ParseErrors` enum and their `code()` / `severity()` / `location()` / `IntoDiagnostic` surface.
- The **author-facing YAML grammar** — `kind:` discriminator values, block names (`data_kinds:`, `relationships:`, `semantics:`, `binding:`, `column_mapping:`, `temporal_shape:`), enum spellings (`one_to_many`, `timeseries`, etc.).

All of the above are `#[non_exhaustive]` per I10 (except newtype-over-stable cases like `DataKindName`, `SemanticsName`, `RelationshipId` per `30`'s newtype exception). MINOR additions: new variants, new `Option<T>` fields, new YAML keys with sensible defaults. MAJOR changes: removing / renaming any of the above.

### 13.2 Internal surface

Not stable; subject to change at MINOR cadence:

- `ColumnMapping` inner representation (today `BTreeMap<_, _>`; may grow a more efficient indexed form).
- The `YamlRoot` / `YamlDataset` / `YamlGrainset` / `YamlUnionset` / `YamlJoinset` intermediate structs in `parse.rs`. These are implementation details of the parse dispatch; they are NOT exported.
- `substitute_env_vars` (signature and behaviour) — `pub(crate)` only; not public API.
- The internal ordering of variant-check passes in `parse` (e.g. "check `kind:` discriminator before checking `name:` duplication"). Observable only through the first-error-wins behaviour on malformed input; not a stable contract.

### 13.3 Feature flags

`semstrait-model` has one feature flag:

| Feature | Gates | Default |
|---|---|---|
| `serde` | `Serialize` / `Deserialize` on every public type | ON |

The `serde` feature is ON by default here (contrast `31 §11` where it's OFF). The crate's entire purpose is YAML round-trip; disabling `serde` is a specialised use case (e.g. programmatic model-building harnesses) and consumers opt-out via `default-features = false`.

No `async` feature. No `arrow` feature. No engine-gated features. Per I11.

### 13.4 Dependency posture

```toml
[dependencies]
semstrait-core = { path = "../semstrait-core", features = ["serde"] }
serde          = { version = "^", features = ["derive"] }
serde_yaml     = "^"
thiserror      = "^"

[features]
default = ["serde"]
serde   = ["dep:serde", "dep:serde_yaml", "semstrait-core/serde"]
```

- **No** `semstrait-manifest`, `semstrait-planner`, `semstrait-adapter`, or `semstrait-catalog` deps. CI enforces (§14.1).
- **No** `tokio`, `futures`, `async-trait`, `reqwest`. I11.
- **No** `arrow`, `datafusion`, `spark-*`, `duckdb`. I2.
- **No** direct `std::fs` / `std::net` imports in `src/`. I11 (and `parse` takes `&str` anyway).

## 14. Crate Boundaries

### 14.1 No I/O

`parse` takes a `&str`. The crate does not open files, read environment files, connect to catalogs, or perform any syscalls other than `std::env::var` inside `substitute_env_vars`.

A CI check enumerates `std::fs`, `std::net`, `std::process` imports in the crate source and fails on any match. `std::env::var` is whitelisted (the substitution surface is documented).

### 14.2 No resolution

Name resolution is `compile`'s responsibility, not `parse`'s. In particular:

- `DataKindRef::ByName(name)` is not resolved to a `DataKindId` here.
- `SemanticsName` references inside `ExprSource::Inline` DSL are left as opaque identifiers inside the resulting `SemanticExpr`; they resolve at `compile` through `14 §7.3`.
- `RelationshipId` is a string wrapper; the referenced Relationship entry is located at `compile`.
- Shared-Semantics `ref:` entries are left as `DimensionEntry::Ref(SemanticsName)` / etc.; expansion happens at `compile`.

Any attempt to add a resolver interface to `32` must go through ratification against this section.

### 14.3 No planning

No `Request`, no `PlanNode`, no `SemanticPlan`, no `PhysicalPlan` anywhere in `semstrait-model`. These are `semstrait-ir` / `semstrait-planner` types (`34`, `35`). If `parse` produces a tree that the planner cannot eventually plan, that's a `compile` or `plan` error, not a `parse` error.

### 14.4 Divergence from legacy code (`[CODE-DIVERGES-FROM-SPEC]`)

The current `crates/semstrait-model/src/` code carries pre-ratification vocabulary that `32` replaces:

| Legacy | This document |
|---|---|
| Top-level `datasets:` / `grainsets:` / `unionsets:` / `joinsets:` blocks (implicit kind) | Single `data_kinds:` block with explicit `kind:` discriminator (§4.2). |
| `ChildEntry` / `datasets:` sub-block on complex kinds | `children:` with explicit `ref:` entries (§5.2 / §5.3). |
| `YamlJoinset.associativity: JoinAssociativity` | `JoinsetSpec.anchor: DataKindRef` + `JoinsetSpec.path: JoinPath` (§5.4) — per `24 §2.2`. |
| `ColumnMappingValue::Simple(String)` / `::WithGrain { column, grain }` / `::Anchored(HashMap)` | `ColumnMappingValue::Column { column, grain }` / `::Literal` / `::Computed` / `::Metadata` (§8.3). The `Anchored` variant is absorbed into `Computed` (anchored composition is a computed expression). |
| `DataType` enum with `I8 | I16 | I32 | I64 | F32 | F64 | Bool | String | Date | Timestamp | Decimal` | `semstrait-core::DataType` per `13 §2.1` / `31 §4.1` — 14 variants including `Byte`, `Short`, `Integer`, `Long`, `Float`, `Double`, `Time { precision }`, `Interval`, `Binary`. |
| `TemporalGrain` enum (in `temporal.rs`) | `semstrait-core::Grain` per `13 §3.1` / `31 §4.2`. |
| `Relationship` in `relationship.rs` uses `relationship_type:` / `source_set:` / `target_set:` | `Relationship` per `16 §2.1` — `from:` / `to:` / `cardinality:` / `join_type:` / `directionality:` (§7). |
| `substitute_env_vars` returning `Result<String, ModelError>` | `Result<String, ParseError>` (§10.2). `ModelError` itself is absorbed into `ParseError` + `ValidateError` per `31 §8`. |

The migration plan is tracked in `implementation/40_refactor_plan.md` under `[REFACTOR-MODEL-GRAMMAR]`. Round-1 status: design ratified here; implementation lag expected. The design IS the source of truth (I9); the code catches up.

## 15. Round-1 Open Items

See `/docs/design/open_questions/32_open_questions.md`. Cross-references in the items below.

| # | Title | Parked item |
|---|---|---|
| Q-MODEL-001 | Multi-file / directory loader helper | `[TD-MODEL-DIR-LOADER]` — `parse_dir` or `parse_files` shape is deferred; CLI / LSP build loaders today (§10.3). |
| Q-MODEL-002 | Primary error-code shape | kebab-case per `30 §6` vs legacy `PARSE_E_####` numeric. Primary = kebab-case; numeric preserved as secondary per `31 §8.3`. Ratification follow-up on `30` (§11.3). |
| Q-MODEL-003 | `kind:` discriminator spelling | `kind: dataset` vs `kind: simple` — both spellings accepted in v1 as synonyms (§5.1). Deprecation of `dataset` deferred to v2 MINOR. |
| Q-MODEL-004 | `DataKindRef::Inline` hoisting cadence | Round 1: always hoist at `parse`. `[TD-INLINE-HOIST-LAZY]` — deferred hoisting for incremental re-parse in LSP (§3.3). |
| Q-MODEL-005 | Expression-surface parse-site table | Audit pass: every `expr:` site in §6 / §8 / §9 must map to either `parse_semantic` or `parse_physical`. Table completion tracked as an integration test. |
| Q-MODEL-006 | `AggregationConstraints.allowed` / `.prohibited` ordering | §12.2 keeps YAML author order for I4; `31 §6.3` already matches token-wise so ordering is not semantic. Parked — confirms the `Vec<String>` shape is OK. |
| Q-MODEL-007 | `serde_yaml` vs `yaml-rust2` | Current code uses `serde_yaml`. The crate is in maintenance mode upstream. Migration to `yaml-rust2` or `saphyr` is tracked as `[TD-MODEL-YAML-CRATE]`; parse-error-quality-dependent. |
| Q-MODEL-008 | `functions:` YAML block scope | §4.5 — per-model function extensions vs process-global `RegistryExtension` impls. Round-1: both supported; collision handling at `compile` per `31 §5.8`. |

---

*Cross-references in this document are by section (e.g. `14 §3.2`, `16 §2.1`, `31 §8.3`). No code-path references are used, per `00 §8`.*
