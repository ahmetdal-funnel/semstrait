---
prereqs: [00, 10, 11, 12, 13, 14]
authoritative-for:
  - the canonical function catalog (names, categories, arities, signatures)
  - the `FunctionRegistry` API surface and its compile-time population model
  - function signature polymorphism — `FnSignature`, `ParamType`, `ReturnTypeRule`
  - function-level `Additivity` (carried as a field on `FunctionSpec`)
  - adapter registry extension API
refined-by:
  - 19 (compile-pipeline consumes the registry; effective additivity composes function-level with model-level)
  - 18 (model-level `AdditivityType` carried per-Measure / per-Metric)
  - registry/functions_mapping.md (per-engine native-name mapping + portability gaps)
  - 34 / 36 (adapter crates extend and consume the registry)
---

# 14a. Function Catalog

> Ratifies the machinery `14` defers to at every `FunctionCall` site and at `14 §6.4`'s Declarative-block tag dispatch: the `FunctionRegistry` API shape and lifecycle, the function-spec model, the canonical scalar / non-closed-aggregate catalog, function-level `Additivity`, and the adapter extension API.
>
> Crate-level placement (after the `14` second-refinement landing): `CanonicalFn`, `FunctionRegistry`, and the spec types live in `semstrait-ir` per `[14 §9.2](14_expressions.md)` and `[35 §7](../apis/35_semstrait_ir.md)`.

## 1. Purpose and Scope

`14a` ratifies how semstrait models functions. The forward references from `14` (`§3.3` `FunctionCall`, `§6.4` Declarative-block tag dispatch) resolve here.

**Ratifies:** `FunctionRegistry` API + lifecycle (§2); function-spec model + signature polymorphism + return-type rules + variadic posture (§3); canonical scalar / non-closed-aggregate catalog policy (§4); `BinaryOp` promotion posture — no canonical lattice in v1, per-engine deviation lives in `registry/functions_mapping.md` (§5); portability model (§6); adapter extension API (§7); function-resolution errors (§8).

**Does NOT ratify:** the compile-pipeline substitution/resolution algorithm — `[19 §3](19_expression_flow.md)`; per-engine native-name mapping, rewrite tiers, emulation — `registry/functions_mapping.md`; adapter rendering of resolved `FunctionCall` — `[36](../apis/36_semstrait_adapter.md)`.

**Invariants upheld:** **I1** — registry operates on canonical `Expr::FunctionCall { name, args }`; no SQL text. **I2** — signatures typed in canonical `DataType` / `TypeClass`. **I10** — every public sum carries `#[non_exhaustive]`. Per `14 §6.4`, registered names form the non-reserved Declarative-block tag vocabulary. Per `14 §5.4`, no implicit coercion at call sites — exact-type signature match or `NoMatchingSignature`.

## 2. The `FunctionRegistry` — API and Lifecycle

The `FunctionRegistry` is the compile-time catalog `compile` consults to resolve `Expr::FunctionCall.name` to a `FunctionSpec` (§3), and that the Declarative-block parser consults per `14 §6.4` to turn a non-reserved tag into a `FunctionCall` node. Surface: populate at startup, seal, look up.

### 2.1 Lifecycle

Sealed `&'static FunctionRegistry` — one shared instance per process, initialized eagerly via `pub fn function_registry() -> &'static FunctionRegistry` (`OnceLock`-backed). Immutable post-seal; no per-Model configurability in v1. Eliminates lifetime noise from every compile-stage signature. Multi-config (test-harness swappable adapter sets) tracked under `[TD-REGISTRY-MULTI-CONFIG]`.

### 2.2 Storage and entries

Two entry families share one flat `HashMap<&'static str, FunctionSpec>`: **core** entries (canonical catalog, §4) shipped with the runtime; **adapter-extended** entries (engine-specific functions like Spark `collect_set`, DuckDB `list_extract`) contributed by adapter crates per §7. No tier / overlay; collision handling per §7.2.

### 2.3 Lookup

```rust
impl FunctionRegistry {
    pub fn lookup(&self, name: &str) -> Option<&FunctionSpec>;
}
```

Strict case-sensitive exact match on the canonical name. Canonical names are lowercase ASCII per `14 §6.5`'s identifier grammar. No aliases in v1 (`[TD-REGISTRY-ALIASES]`). Namespace shared with `14 §6.4.1`'s reserved AST tags — registry entries colliding with reserved tags surface as `ReservedTagCollision` at startup (§7.2). Callers: Declarative-block parser at parse time; `compile` at type-inference time (§3); adapters at `adapt` time (which also consult `registry/functions_mapping.md` for the engine-native name).

## 3. Function Spec

### 3.1 `FunctionSpec` shape

```rust
#[non_exhaustive]
pub struct FunctionSpec {
    pub canonical_name: &'static str,
    pub category:       FunctionCategory,
    pub signatures:     NonEmpty<FnSignature>,
    pub additivity:     Option<Additivity>,  // §3.6; None for scalar / non-aggregate
    pub description:    &'static str,
}
```

No `null_handling` (NULL-in/out delegated to engine per `14 §5.4`). No `deterministic` (`[TD-REGISTRY-DETERMINISM]`). No `aliases` / `since_version` / `stability` (versioning lives in `apis/30`). Per-engine portability carriage lives entirely in `registry/functions_mapping.md` (§6).

### 3.2 `FunctionCategory`

```rust
#[non_exhaustive]
pub enum FunctionCategory {
    Scalar,
    Aggregate,
    // Window — compile-emitted only per `14 §3.3` notes; not author-registered in v1.
}
```

Discriminates scalar vs non-closed-aggregate. Consumed by `14 §7`'s shape-gate enforcement (`Aggregate`-category calls are gated against scalar-only sites) and by the planner's additivity / rollup reasoning (`19 §6.5`). No scalar sub-categorization in v1 (`[TD-REGISTRY-SUBCATEGORY]`); catalog §4 groups by domain in headings only.

### 3.3 Signature polymorphism — overload set

```rust
#[non_exhaustive]
pub struct FnSignature {
    pub args:        &'static [ParamType],
    pub variadic:    Option<ParamType>,
    pub return_type: ReturnTypeRule,
}
```

`signatures: NonEmpty<FnSignature>`; lookup attempts each signature in declaration order; first exact match (per-arg `DataType` equality) wins. No TypeClass generics, no implicit coercion (`14 §5.4`). On no match: `NoMatchingSignature { name, arg_types, tried_signatures }` (the `tried_signatures` list mirrors the full overload set so the author can compare directly). TypeClass-parameterised generics deferred under `[TD-REGISTRY-TYPECLASS]`.

**Variadic posture.** `variadic: Some(T)` accepts zero-or-more trailing args of type `T` after the fixed prefix. Mid-signature variadic (e.g. `printf`-style) is `[TD-REGISTRY-MID-VARIADIC]`; not needed in v1. Optional args (`substring(expr, start, [length])`) are expressed as multiple overloads differing in arity.

### 3.4 `ReturnTypeRule`

```rust
#[non_exhaustive]
pub enum ReturnTypeRule {
    /// Concrete canonical type.
    Fixed(DataType),
    /// Mirror arg N's type (0-indexed).
    SameAs(usize),
    /// Common-supertype promotion of the listed arg indices per `13 §2.6`.
    Promoted(&'static [usize]),
    /// Escape hatch for width-sensitive / literal-driven rules (e.g. `cast(x, T) -> T`).
    Custom(fn(&[DataType]) -> Result<DataType, CompileError>),
}
```

Nullability is pass-through per `13`'s NULL model; no `NullableOf` variant. Per-engine nullability tightening lives in `registry/types_mapping.md`.

### 3.5 `ParamType`

```rust
#[non_exhaustive]
pub enum ParamType {
    Concrete(DataType),
    AnyOf(Vec<DataType>),
    NumericFamily,
    StringFamily,
    TemporalFamily,
    Any,
}
```

Family variants ground in `[13 §4](13_types_and_grain.md)`'s `TypeClass` vocabulary. Family use is restricted to declared overloads in v1; full TypeClass generics still deferred under `[TD-REGISTRY-TYPECLASS]`.

### 3.6 `Additivity` (function-level)

Canonical mathematical additivity of an aggregate function. Carried on `FunctionSpec.additivity`; `None` for scalar functions.

```rust
#[non_exhaustive]
pub enum Additivity {
    Additive,                                  // SUM, COUNT, MIN, MAX
    SemiAdditive { axes: Vec<DimensionAxis> }, // FIRST, LAST (temporal axis)
    NonAdditive,                               // AVG, COUNT_DISTINCT, MEDIAN, PERCENTILE
}

#[non_exhaustive]
pub enum DimensionAxis {
    Temporal,
}
```

**Two-source SoC** (per `[19 §6.5.1](19_expression_flow.md)`'s composition table): function-level here vs model-level `AdditivityType` on a Measure / Metric per `[18 §5.2](18_entities.md)`. Phase B Strategy reads both independently; the **effective** additivity (intersection of axes, narrower wins) is what drives lossy-reaggregation advisories per `[19 §6.5.2](19_expression_flow.md)`.

Not author-extensible in v1 — hardcoded per built-in aggregate; UDF additivity is `[TD-REGISTRY-DETERMINISM]`'s sibling deferred item.

## 4. Canonical Function Catalog

### 4.1 Presentation + population policy

Presentation: one row per `FunctionSpec`; overloads collapsed into a `signature(s)` cell (e.g. `(Int, Int) -> Int | (Long, Long) -> Long`). Columns: `canonical_name`, `arity` (or `variadic` marker), `signature(s)`, `return-type rule`, `notes`. Per-engine coverage strip is intentionally absent — `14a` is engine-agnostic; per-engine detail lives in `registry/functions_mapping.md`.

Population policy: **adapter-capability intersection** across DataFusion ∩ Spark ∩ DuckDB. A function enters the canonical catalog only when all three first-class targets support it natively or via trivial name-remap per `registry/functions_mapping.md`'s rewrite tiers. Subset-supported functions are **adapter-extended** entries per §7. This makes every canonical entry portable-by-construction; the intersection grows as adapter coverage matures.

§4.2–§4.6 entries are candidate lists pending intersection verification; the final catalog MAY be a proper subset.

### 4.2 Scalar — String

Candidates: `upper`, `lower`, `length`, `substring`, `trim`, `ltrim`, `rtrim`, `concat`, `replace`, `lpad`, `rpad`, `reverse`. Reserved predicates (`Like` / `ILike` / `RegexpMatch` / `RegexpExtract`) are NOT registry entries — see `[14 §3.3](14_expressions.md)`.

### 4.3 Scalar — Math

Candidates: `abs`, `round`, `ceil`, `floor`, `sqrt`, `power`, `exp`, `ln`, `log`, `log10`, `sign`. `mod(x, y)` is NOT in the catalog — `BinaryOpKind::Mod` (`%`) is the canonical form.

### 4.4 Scalar — Temporal

Candidates: `date_add`, `date_sub`, `date_diff`, `extract`, `year`, `month`, `day`, `hour`, `minute`, `second`, `current_date`, `current_timestamp`, `to_date`, `to_timestamp`. Per-engine name remaps live in `registry/functions_mapping.md`.

### 4.5 Scalar — Logical / Conditional helpers

Candidates: `greatest`, `least`, `if`, `ifnull`, `nvl`. Reserved AST variants (`Case`, `Coalesce`, `NullIf`, `IsNull`) are NOT registry entries.

### 4.6 Aggregate — non-closed

Candidates: `stddev`, `variance`, `median`, `string_agg`, `percentile_cont`, `percentile_disc`, `approx_count_distinct`. Engine-specific forms (DuckDB's `approx_top_k`, Spark's `percentile_approx`) are adapter-extended per §7. Entries carry `FunctionCategory::Aggregate` and `Additivity` per §3.6.

### 4.7 The closed five aggregates

`Sum` / `Avg` / `Count` / `Min` / `Max` are carried by `Expr::Aggregate { op: AggregationOp, … }` (the closed `AggregationOp` enum in `[31 §5.2](../apis/31_semstrait_core.md)`) — **not** registry entries. Their return-type rules follow SQL:2016 promotion; effective additivity is `Additive` for Sum / Count / Min / Max, `NonAdditive` for Avg.

## 5. BinaryOp Promotion — No Canonical Lattice

`BinaryOpKind` (per `[14 §3.3](14_expressions.md)`) is a dedicated `Expr` variant, not a registry entry. `14a` **does not** publish a canonical promotion lattice because:

1. `14 §5.4`'s pass-through posture delegates result-type derivation to the engine.
2. SQL dialects disagree on details (`Decimal` rounding/scale, `Integer / Integer` precision, mixed `Integer + Float`).
3. Per-engine observed rules live in `registry/functions_mapping.md`'s `BinaryOp` section; authors and adapter implementers consult it there.

Semstrait emits no implicit widening at `BinaryOp` sites and gates no compile-time error on lattice mismatch. `Expr::Cast` appears only via (a) author-written casts and (b) Semantics-boundary reconciliation per `[19 §3.7](19_expression_flow.md)`. Future canonical-with-deviations promotion (if per-engine divergence proves narrow enough) tracked under `[TD-REGISTRY-BINOP-LATTICE]`.

## 6. Portability Model

`FunctionSpec` carries **no portability flag**. The §4.1 intersection policy makes every canonical entry portable-by-construction; for adapter-extended entries (§7), portability is implicit in the registration (the adapter that registers the function IS its supported engine).

Per-engine detail (native-name remap, arity quirks, cast-semantics corner cases, emulation strategies) lives in `registry/functions_mapping.md`. Adapters read both; authors working against the canonical catalog read only `14a`.

**Missing-function policy.** Hard error at `adapt` time when the target engine does not natively support a registered function and no emulation strategy applies — `AdaptErrorKind::UnsupportedFunction { name, engine }`. Mirrors `13` / `registry/types_mapping.md`'s `UnsupportedType` precedent; aligns with `00 §9` **I6** / **I12** (no runtime-deferred stubs).

## 7. Adapter Extension API

Engine-specific functions (Spark `collect_set`, DuckDB `list_extract`, DataFusion `array_to_string`, …) layer onto the core registry via compile-time trait-impl registration:

```rust
pub trait RegistryExtension {
    const ADAPTER_ID: &'static str;
    const FUNCTIONS: &'static [FunctionSpec];
}
```

Adapter crates impl `RegistryExtension` on a zero-size marker; `function_registry()` enumerates linked extensions at startup and folds them into the §2.2 flat map. Extension wiring detail (feature gate vs build.rs aggregation): `[TD-REGISTRY-EXTENSION-WIRING]`.

**Collision policy.** Hard reject at registry initialization — both adapter-vs-core and adapter-vs-adapter. Three error variants per §8: `ReservedTagCollision` (name shadows a `14 §6.4.1` reserved AST tag), `AdapterFunctionShadowsCore` (name already in §4), `AdapterFunctionCollision` (two adapters registered same name). Because the registry is `&'static`, these surface as panics at `function_registry()` initialization — adapter misconfiguration is a build-time problem. No tier precedence, no signature-compatible shadowing. Engine-specific semantics overrides for canonical entries are expressed via `registry/functions_mapping.md`'s rewrite tiers, not re-registration.

## 8. Error Model

Function-resolution errors feed `[19 §8.2](19_expression_flow.md)`'s error roster.

| Variant | When |
|---|---|
| `CompileError::UnknownFunction { name }` | `FunctionCall.name` not in the sealed registry. |
| `CompileError::FunctionArityMismatch { name, expected, got }` | Call-site arity outside the spec's declared range. |
| `CompileError::NoMatchingSignature { name, arg_types, tried_signatures }` | No overload matches; per `14 §5.4` no implicit coercion — author must `CAST` explicitly. |
| `CompileError::ReservedTagCollision { tag, source }` | Adapter registration shadows a `14 §6.4.1` reserved AST tag. |
| `CompileError::AdapterFunctionShadowsCore { name, adapter }` | Adapter registered a name already in §4. Surfaces as panic at `function_registry()` initialization. |
| `CompileError::AdapterFunctionCollision { name, adapters }` | Two adapters registered the same name. Same surfacing. |
| `AdaptErrorKind::UnsupportedFunction { name, engine }` | Adapter cannot render a registered function for its target engine. |

## 9. Cross-References

- `[14_expressions.md](14_expressions.md)` — `Expr::FunctionCall { name, args }` consumer; `§6.4` Declarative-block tag dispatch.
- `[18_entities.md](18_entities.md)` — model-level `AdditivityType` carried per-Measure / per-Metric (composed with function-level `Additivity` from §3.6 per `19 §6.5`).
- `[19_expression_flow.md](19_expression_flow.md)` — compile-pipeline consumer; effective-additivity composition (§6.5).
- `[../apis/35_semstrait_ir.md](../apis/35_semstrait_ir.md)` — crate home of `CanonicalFn` + `FunctionRegistry`.
- `[../apis/36_semstrait_adapter.md](../apis/36_semstrait_adapter.md)` — adapter rendering of resolved `FunctionCall`s.
- `registry/functions_mapping.md` — per-engine catalog of native names, rewrite tiers, emulation strategies, portability gaps.

