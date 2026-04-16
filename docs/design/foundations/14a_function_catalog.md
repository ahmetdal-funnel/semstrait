---
prereqs: [00, 10, 11, 12, 13, 14]
authoritative-for:
  - the canonical function catalog (names, categories, arities, signatures)
  - the `FunctionRegistry` API surface and its compile-time population model
  - function signature polymorphism — `FnSignature`, `ParamType`, `ReturnTypeRule`
  - the BinaryOp promotion lattice (canonical reference table)
  - adapter registry extension API
  - function portability flags
refined-by:
  - 14b (expression resolution consumes the registry at compile time)
  - registry/functions_mapping.md (per-engine native-name mapping + portability gaps)
  - 34 / 36 (adapter crates extend and consume the registry)
---

# 14a. Function Catalog

> This document ratifies the machinery `14` defers to: the `FunctionRegistry`'s
> API shape and lifecycle, the function-spec model (`FnSignature`, `ParamType`,
> `ReturnTypeRule`), the canonical scalar / non-closed-aggregate catalog, the
> BinaryOp promotion posture, and the adapter extension API. `14` is the
> consumer: it references "resolved via `14a`" at every `FunctionCall` site and
> at §4.4.2's Declarative-block tag-dispatch rule.
>
> **Status (Round 1 ratified).** All 16 framework decisions settled per §10's
> Ratified Decisions Index. Round 2 populates §4.2–§4.6 against the Q10
> adapter-capability-intersection policy once DataFusion / Spark / DuckDB
> adapter function inventories are verified.

## 1. Purpose and Scope

`14a` ratifies **how semstrait models functions**: the registry API compile consults, the spec shape each entry declares, the canonical catalog of scalar and non-closed-aggregate functions authors can call, the promotion behavior of `BinaryOp` rendered as a reference table, and the contract adapters use to extend the catalog. Every forward reference in `14 §3.2` / `§4.4.2` / `§5.6` / `§7.3` ("resolved per `14a`", "signature machinery in `14a`", "promotion lattice in `14a`") resolves here.

**What `14a` ratifies:**

- The `FunctionRegistry` API — its lifecycle (compile-time built, read-only post-seal), its layering (core + adapter-extended), its lookup contract (§2).
- The function-spec model — `FunctionSpec`, `FnSignature`, `ParamType`, `ReturnTypeRule` — and the polymorphism strategy chosen for scalar overloads (§3).
- The **canonical** scalar and non-closed-aggregate function catalog — name, arity, signature(s), return-type rule, portability flag per entry (§4). The canonical shape lives here; per-engine native-name mapping and portability gaps live in `registry/functions_mapping.md`.
- The BinaryOp promotion lattice — documented as a **reference** table for authors and adapter implementers; NOT registry entries (`14 §3.2` keeps `BinaryOp` as a dedicated variant) and NOT compile-time validation (`14 §5.6` pass-through posture) (§5).
- The portability model — per-spec flags, adapter-time missing-function policy, the split between 14a (canonical shape) and `registry/functions_mapping.md` (per-engine reality) (§6).
- The adapter extension API — how `34` / `36` layer engine-specific functions onto the core registry, collision rules, and portability implications (§7).
- Function-resolution error variants feeding `14 §7.3` (§8).

**What `14a` does NOT ratify** (forward-refs):

- The substitution/resolution algorithm over the registry — `14b`.
- Per-engine native-name mapping, rewrite tiers, emulation strategies — `registry/functions_mapping.md`.
- Adapter rendering of resolved `FunctionCall` to engine SQL / Substrait — `34` / `36`.

**Key invariants from `00` / `10` / `13` / `14` that `14a` upholds:**

- **I1** — the registry operates on the canonical `Expr`-level `FunctionCall { name, args }`; it never sees SQL text or engine-native forms.
- **I2** — signatures are typed in canonical `DataType` / `TypeClass` terms only (per `13 §2` / `13 §4`).
- **I10** — `FunctionCategory`, `ParamType`, `ReturnTypeRule`, and any other public sum type introduced here is `#[non_exhaustive]`.
- **`14 §3.2`** — `FunctionCall` is scalar-or-(non-closed-)aggregate; never takes `distinct`. Arity and signature are registry-enforced.
- **`14 §4.4.2`** — the registry's registered names directly form the non-reserved Declarative-block tag vocabulary. A name appearing in the registry is authorable as a top-level block tag.
- **`14 §5.6`** — no implicit coercion at call sites. If no signature matches exact arg types, `CompileError::NoMatchingSignature` is raised.

## 2. The `FunctionRegistry` — API and Lifecycle

The `FunctionRegistry` is the compile-time catalog compile / `14b` consult to resolve every `Expr::FunctionCall.name` to a `FunctionSpec` (§3), and that the Declarative-block parser consults per `14 §4.4.2` to turn a non-reserved tag into a `FunctionCall` node. Its surface is intentionally narrow: populate, seal, lookup.

### 2.1 Compile-time vs runtime population

The registry is populated before any parse / validate / compile stage consumes it, and is immutable thereafter. Two families of entries contribute to the sealed instance: **core** entries (§2.2) and **adapter-extended** entries (§7).

**Ratified (Round 1, Q1).** The sealed registry is handed to compile as a `&'static FunctionRegistry` — one shared instance per process, initialized eagerly at program startup via an inherently-static path (e.g. `OnceLock<FunctionRegistry>` behind a `pub fn function_registry() -> &'static FunctionRegistry`). No per-Model or per-invocation configurability in v1; a single registry configuration per process. Rationale: eliminates lifetime noise from every signature that touches compile; matches the "planner-complete Manifest + static registry" posture where registry entries are compile-time catalog data, not runtime state. Future need for multiple registry configurations in the same process (e.g. test-harness with swappable adapter sets) is `[TD-REGISTRY-MULTI-CONFIG]`.

### 2.2 Core (built-in) vs adapter-extended entries

**Core entries** — the canonical catalog of §4. These are shipped with `semstrait-core` / `semstrait-model` and are always present. **Adapter-extended entries** — functions that only a specific engine exposes (e.g. a Spark-specific `collect_set`, a DuckDB-specific `list_extract`) are contributed by adapter crates per §7.

**Ratified (Round 1, Q2).** Flat storage — a single `HashMap<&'static str, FunctionSpec>` where core and adapter-extended entries live side-by-side in one namespace. No tier / overlay structure. Collision handling follows §7.2's hard-reject policy (both adapter-vs-core and adapter-vs-adapter). Rationale: lookup is a single O(1) hash probe; no per-tier resolution order to reason about; registration-time collision checks are clean.

### 2.3 Lookup semantics (name → spec)

The lookup contract:

```rust
impl FunctionRegistry {
    pub fn lookup(&self, name: &str) -> Option<&FunctionSpec>;
}
```

Called by: (a) the Declarative-block parser at parse time per `14 §4.4.2` (to decide if a tag dispatches to a function), (b) `compile` when typing a `FunctionCall` node per §3 (to resolve signature and derive return type), (c) adapters at `adapt` time to render a resolved `FunctionCall` (consulting `registry/functions_mapping.md` for the engine-native name).

**Ratified (Round 1, Q3).** Strict case-sensitive exact match on a single canonical name per spec. Canonical names are lowercase ASCII identifiers (matching `14`'s identifier grammar). No aliases in v1 — if a function has multiple community names (`char_length` vs `length`, `to_upper` vs `upper`), one form is canonical and the other is simply absent from the registry. Namespace with `14 §4.4.1`'s 21 reserved AST tags is shared: a registry entry whose name collides with a reserved tag is `CompileError::ReservedTagCollision` at registration. Adapter-vs-adapter and adapter-vs-core overlap is `CompileError::AdapterFunctionCollision` / `AdapterFunctionShadowsCore` per §7.2. Rationale: zero ambiguity at lookup; aliases add maintenance burden with limited author value; authors who want `char_length` can use `length` (the canonical) or — for engines where `char_length` renders differently — rely on the adapter's per-engine rewrite in `registry/functions_mapping.md`. Alias support is `[TD-REGISTRY-ALIASES]`.

## 3. Function Spec — `FunctionSpec`, `FnSignature`, `ParamType`, `ReturnTypeRule`

Each registry entry is a `FunctionSpec` (§3.1). The spec couples a canonical name to one or more `FnSignature`s (§3.3) plus category (§3.2) and portability metadata (§6). Signatures express what argument types are admissible and how the return type is derived from them (§3.4).

### 3.1 `FunctionSpec` record shape

**Ratified (Round 1, Q4).** `FunctionSpec` carries a **minimum-plus-description** field set:

```rust
#[non_exhaustive]
pub struct FunctionSpec {
    pub canonical_name: &'static str,
    pub category: FunctionCategory,
    pub signatures: NonEmpty<FnSignature>,
    pub description: &'static str,
}
```

No `null_handling` flag (engine-delegated per `14 §5.6` pass-through posture — NULL-in / NULL-out behavior belongs to the engine's evaluation model, not the registry). No `deterministic` flag in v1 (see `[TD-REGISTRY-DETERMINISM]` — may land when optimizer starts caring, e.g. constant-folding rules). No `aliases`, `since_version`, or `stability` (Q3 rules out aliases; versioning belongs to semstrait's semver posture per `apis/30`). Portability carriage is offloaded entirely to `registry/functions_mapping.md` per Q12.

### 3.2 Categories (`FunctionCategory`)

The category discriminates scalar from non-closed-aggregate (and eventually window) functions. It is consulted by validate-stage wrapper invariants (`14 §2.3` — `Aggregate` and by extension `FunctionCategory::Aggregate` calls are forbidden in `PhysicalExpr` just as `Expr::Aggregate` is) and by the planner's additivity / rollup reasoning (forward to `20–25`).

**Ratified (Round 1, Q5).** Flat `FunctionCategory`:

```rust
#[non_exhaustive]
pub enum FunctionCategory {
    Scalar,
    Aggregate,
    // Window — deferred per `14 TD-EXPR-WINDOW`
}
```

No scalar sub-categorization (`Scalar::String` / `Scalar::Math` / …) in v1. Catalog presentation (§4.2–§4.6) groups by domain in **table headings only**; the enum stays flat. Rationale: sub-categorization is presentation metadata, not type-driven dispatch; keeps enum size stable and `[TD-REGISTRY-SUBCATEGORY]` open for future optimizer-rule keying if that need concretizes.

### 3.3 Signature polymorphism model

**Ratified (Round 1, Q6).** Pure **overload-set**: `FunctionSpec.signatures: NonEmpty<FnSignature>`, each signature carrying fully-concrete `ParamType`s and a `ReturnTypeRule`. Lookup attempts each signature in declaration order; the first exact-match (per-arg `DataType` equality after §3.4 rule application) wins. No TypeClass generics, no type-variable binding, no implicit coercion (`14 §5.6`). When no signature matches, `CompileError::NoMatchingSignature { name, arg_types, tried_signatures }` — the `tried_signatures` list is the full overload set, rendered as concrete `ParamType` vectors per entry.

Rationale for overload-set over TypeClass generics:

- **Implementation simplicity** — no type-variable solver; match is tuple-equality.
- **Error-reporting fidelity** — `tried_signatures` is a literal list of overload shapes the author can compare against; under TypeClass generics the report would need to include TypeClass bounds + failed-unification detail.
- **Explicit catalog** — the catalog (§4) enumerates every admissible shape; nothing hides behind a generic bound.
- **Acceptable verbosity** — broad-polymorphism functions (`greatest`, `least`, `abs`) enumerate `(Int, Int)`, `(Long, Long)`, `(Double, Double)`, … as separate signatures. The verbosity is linear; the catalog still reads cleanly.

When future adapter-extended functions (§7) register with a closed-but-large overload set, this is still tractable; the registration API (§7.1) exposes a `signatures: &[FnSignature]` constant.

TypeClass-parameterized generics remain an `[TD-REGISTRY-TYPECLASS]` deferred extension. If adopted, they compose cleanly with the overload-set model (a single spec could carry a mix of concrete and generic signatures).

### 3.4 Return-type rules

Return type depends on argument types. A `ReturnTypeRule` enum expresses the common derivations:

**Ratified (Round 1, Q7).** Minimal-4 variant set:

```rust
#[non_exhaustive]
pub enum ReturnTypeRule {
    /// Result is a concrete canonical type.
    Fixed(DataType),
    /// Result mirrors arg N's type (0-indexed).
    SameAs(usize),
    /// Result is the common-supertype promotion of the listed arg indices per
    /// `13 §4` / `14 §5.4`'s promotion rules (e.g. `(Int, Long) -> Long`).
    Promoted(&'static [usize]),
    /// Arbitrary rule — used sparingly (e.g. `cast(x, T) -> T` where T is
    /// a `DataType` literal arg, or a width-dependent decimal rule).
    Custom(fn(&[DataType]) -> Result<DataType, CompileError>),
}
```

No `SameAsTypeVar` (Q6 rules out TypeClass generics). No `NullableOf(inner)` — nullability is pass-through per `13`'s NULL model (the canonical `DataType` is nullable-by-default; per-engine nullability tightening lives in `registry/types_mapping.md`). Rationale: four variants cover every function `14a` catalogs; `Custom` is the escape hatch for width-sensitive rules without polluting the enum.

### 3.5 Variadic / optional args

**Ratified (Round 1, Q8).** `FnSignature` carries `args: &'static [ParamType]` plus an optional **trailing variadic** tail:

```rust
#[non_exhaustive]
pub struct FnSignature {
    pub args: &'static [ParamType],
    pub variadic: Option<ParamType>,
    pub return_type: ReturnTypeRule,
}
```

`variadic: Some(T)` means "after the fixed prefix `args`, accept zero-or-more additional arguments, each of type `T`". Not mid-signature; not per-position. Optional args (`substring(expr, start, [length])`) are expressed as **multiple overloads differing in arity** — one signature with `args.len() = 2`, another with `args.len() = 3`. Rationale: trailing-variadic + arity-overloading covers every catalog need (`concat`, `greatest`, `least`, `coalesce` equivalents, `substring`), keeps `FnSignature` simple, and maps directly to the overload-set lookup of §3.3.

Mid-signature repeated params (e.g. `printf`-style where variadic sits between fixed args) are `[TD-REGISTRY-MID-VARIADIC]`; no catalog entry needs it in v1.

## 4. Canonical Function Catalog

The canonical catalog enumerates every scalar and non-closed-aggregate function semstrait authors can call out of the box. Per-engine native-name mapping and portability-gap notes live in `registry/functions_mapping.md`; §4 is the **canonical upstream** shape (name, arity, signature(s), return-type rule, portability flag).

### 4.1 Catalog presentation format

**Ratified (Round 1, Q9).** **One row per `FunctionSpec`**; overloads are collapsed into a single `signature(s)` cell (shown as `(Int, Int) -> Int | (Long, Long) -> Long | ...`) when multiple overloads exist. Columns: `canonical_name`, `arity` (or `variadic` marker), `signature(s)`, `return-type rule`, `notes`. **No per-engine coverage strip** (Q13 — `14a` is engine-agnostic); per-engine detail lives in `registry/functions_mapping.md`. No "since version / status" column in v1 — function entries don't churn at v1 density.

**Ratified (Round 1, Q10).** Catalog population scope is the **adapter-capability intersection** across DataFusion ∩ Spark ∩ DuckDB. A function enters the canonical catalog (§4.2–§4.6) only when all three first-class targets natively support it — or support it via a trivial name-only / name-remap rewrite per `registry/functions_mapping.md`'s rewrite tiers. Functions that are supported by only a subset are **adapter-extended** entries per §7, not canonical. Rationale:

- Keeps the canonical catalog portable-by-construction: every canonical function renders on every first-class adapter without emulation.
- The "universe minus intersection" (functions popular but not universal, e.g. `percentile_cont`) becomes the canonical-catalog candidate-list for each new engine that lands — the intersection grows as adapter coverage matures.
- Aspirational-union functions (popular but not universal in the first-class trio) live in adapter-extended registries per §7, with `registry/functions_mapping.md` documenting their per-engine presence.

Round 2 populates §4.2–§4.6 against this policy. Entries below are **candidate lists pending Round 2 intersection-verification** against the three adapter function inventories; the final catalog MAY be a proper subset.

### 4.2 Scalar — String functions

*Round-2 population pending intersection verification across DataFusion / Spark / DuckDB.*

Candidate entries: `upper`, `lower`, `length`, `substring`, `trim`, `ltrim`, `rtrim`, `concat`, `replace`, `lpad`, `rpad`, `reverse`. Omitted by Q3 / Q10: `char_length` (redundant with `length`), `split_part` (requires intersection check), `position` (verify DF name — may need rename to `strpos` per legacy mapping). Reserved predicates per `14 §3.2` — `Like` / `ILike` / `RegexpMatch` / `RegexpExtract` — are NOT registry entries.

### 4.3 Scalar — Math functions

*Round-2 population pending intersection verification.*

Candidate entries: `abs`, `round`, `ceil`, `floor`, `sqrt`, `power`, `exp`, `ln`, `log`, `log10`, `sign`. `mod(x, y)` NOT in canonical catalog — `BinaryOpKind::Mod` (`%`) is the canonical form per `14 §3.2`; `mod(x, y)` as a function call is an adapter-extended convenience in engines that prefer it.

### 4.4 Scalar — Temporal functions

*Round-2 population pending intersection verification.*

Candidate entries: `date_add`, `date_sub`, `date_diff`, `extract`, `year`, `month`, `day`, `hour`, `minute`, `second`, `current_date`, `current_timestamp`, `to_date`, `to_timestamp`. `DateTrunc` is a dedicated `Expr` variant per `14 §3.2` — NOT a registry entry. Per-engine name remaps (`date_add` vs `dateadd`, etc.) live in `registry/functions_mapping.md`.

### 4.5 Scalar — Logical / Conditional helpers

*Round-2 population pending intersection verification.*

Candidate entries: `greatest`, `least`, `if` (ternary — verify intersection; Spark's `if(cond, then, else)` vs standard `CASE`), `ifnull`, `nvl`. Reserved AST variants — `Case`, `Coalesce`, `NullIf`, `IsNull`, `IsNotNull` — are NOT registry entries.

### 4.6 Aggregate — non-closed (PERCENTILE_CONT, STDDEV, APPROX_COUNT_DISTINCT, …)

*Round-2 population pending intersection verification.* Per Q10, only aggregates in the DataFusion ∩ Spark ∩ DuckDB intersection become canonical; engine-specific forms (e.g. DuckDB's `approx_top_k`, Spark's `percentile_approx`) are adapter-extended.

Candidate entries: `stddev`, `variance`, `median`, `string_agg` (verify — name differs per engine: `listagg`, `string_agg`, `array_join`). `percentile_cont` / `percentile_disc` — verify all-engine native presence; may end up adapter-extended. `approx_count_distinct` — intersection check (all three support it under different names; remap via `registry/functions_mapping.md`).

Registry entries in this group carry `FunctionCategory::Aggregate` and are subject to the same wrapper invariants as `Expr::Aggregate` (forbidden in `PhysicalExpr`, per `14 §2.3`).

### 4.7 The closed five (Sum / Avg / Count / Min / Max) — link back to 14 §5.4

The five universal aggregates are expressed in the AST via `Expr::Aggregate { aggregation: Aggregation, .. }` with the closed `Aggregation` enum — they are **not** registry entries. Their return-type rules live in `14 §5.4`'s SQL:2016 promotion table. `14a` carries no duplicate specification; this subsection exists solely to anchor the cross-ref for readers scanning the catalog.

## 5. BinaryOp Promotion Lattice

`BinaryOp` is a dedicated `Expr` variant per `14 §3.2`, not a registry entry. But authors and adapter implementers need a **canonical reference** for how semstrait expects the result type of `Integer + Long`, `Decimal(10,2) * Decimal(12,4)`, etc. to behave. §5 carries that reference table. It is **documentation**, not compile-time validation (`14 §5.6` pass-through posture — the engine computes the actual result type; the lattice here just anchors author expectations).

### 5.1 Operator kinds covered by the lattice

The 14 `BinaryOpKind` values of `14 §3.2` fall into three groups:

- **Arithmetic** — `Add`, `Subtract`, `Multiply`, `Divide`, `SafeDivide`, `Mod`. Result type is derived from operand types via a width-promotion rule.
- **Comparison** — `Eq`, `NotEq`, `Lt`, `LtEq`, `Gt`, `GtEq`. Result type is always `Boolean`; operand type compatibility (same TypeClass) is engine-enforced.
- **Logical** — `And`, `Or`. Result type is always `Boolean`; operand type is always `Boolean` (engine-enforced).

Only the Arithmetic group has a non-trivial result-type table; Comparison and Logical are flat `Boolean`.

### 5.2 No canonical lattice — per-engine reference only

**Ratified (Round 1, Q11).** `14a` does NOT publish a canonical BinaryOp promotion lattice. Rationale:

- `14 §5.6`'s pass-through posture explicitly delegates cross-operand type admissibility and result-type derivation to the engine. Claiming a canonical lattice contradicts that posture.
- SQL:2003 and engine implementations disagree on details (rounding / scale rules for `Decimal`, behavior of mixed `Integer + Float` under different dialects, `Integer / Integer` producing `Integer` vs `Decimal`).
- A canonical lattice would become a per-engine deviation catalog anyway.

Authors predicting `data_type:` on a Semantics boundary that consumes a `BinaryOp` expression SHOULD consult `registry/functions_mapping.md` (the `BinaryOp` section there will enumerate per-engine observed rules). Adapter implementers authoring boundary reconciliation Casts per `14 §6.4` rule 2 target the engine's natural result-type — which is documented in the per-engine column of `registry/functions_mapping.md` — not a canonical lattice here.

Future `[TD-REGISTRY-BINOP-LATTICE]`: if author experience shows that per-engine divergence is narrow enough to paper over with a canonical rule + small deviation table, §5.2 MAY be promoted from "no canonical" to "canonical with deviations"; not yet ratified.

### 5.3 Interaction with pass-through posture (14 §5.6)

The lattice table in §5.2 is **reference material**, not a validation rule. Per `14 §5.6`:

- Semstrait does **not** validate that BinaryOp operand types satisfy a lattice row — the engine decides operand admissibility at execution time.
- Semstrait emits **no implicit widening Cast** at BinaryOp sites — the engine widens internally.
- The only places `14 §6.4` allows `Expr::Cast` to appear are (a) author-written casts and (b) Semantics-boundary reconciliation. Neither is driven by the §5.2 lattice.

The lattice therefore serves two audiences: **authors** (predicting what their expressions will likely produce, to inform `data_type:` declarations at the Semantics boundary) and **adapter implementers** (anticipating which engine's natural result type will need a boundary reconciliation Cast — `14 §6.4` rule 2). It never gates a compile-time error.

## 6. Portability Model

Not every function in §4 is supported by every target engine. `14a` is the canonical layer's record of that reality; `registry/functions_mapping.md` is the authoritative per-engine catalog.

### 6.1 Portability flag taxonomy — none on `FunctionSpec`

**Ratified (Round 1, Q12).** `FunctionSpec` carries **no portability flag**. Q10's intersection-only population policy makes every canonical entry portable-by-construction across the three first-class targets — a per-spec flag would be tautologically "Universal". For adapter-extended entries (§7), portability is implicit in the registration (the adapter that registers the function IS its supported engine; other engines do not carry that entry).

Per-engine detail — native-name remap, arity quirks, cast-semantics corner cases, emulation strategies when a new engine lands — lives exclusively in `registry/functions_mapping.md`. Engine-coverage queries ("is `stddev` supported on adapter X?") consult the mapping doc; `14a` answers only the canonical question ("is `stddev` in the core catalog?").

### 6.2 Relationship to `registry/functions_mapping.md`

**Ratified (Round 1, Q13).** `14a` is **engine-agnostic**: defines canonical shape (name, category, signatures, return-type rule, catalog membership) and NOTHING per-engine. `registry/functions_mapping.md` owns all engine-facing detail (native function name in DataFusion / Spark / DuckDB, rewrite tier, emulation strategy, per-engine arity/signature quirks, per-engine cast-semantics corner cases). Adapters read both; core-layer code and authors working against the canonical catalog read only `14a`.

Consequence: `14a` alone is insufficient to write a per-engine portability test — that test reads `registry/functions_mapping.md`. But `14a` alone is sufficient to reason about the canonical signature, type-inference at compile, and the Declarative-block tag dispatch of `14 §4.4.2`.

### 6.3 Missing-function policy at `adapt` time

**Ratified (Round 1, Q14).** Hard error at `adapt` time when the target engine does not natively support a registered function and no emulation strategy applies:

```rust
#[non_exhaustive]
pub enum AdaptError {
    // ...
    UnsupportedFunction { name: String, engine: EngineId, location: Span },
}
```

Rationale: matches the `13` / `registry/types_mapping.md` precedent (`AdaptError::UnsupportedType` is a hard error, not a runtime-deferred stub). Authors targeting a specific engine MUST write against that engine's supported catalog — enforced at adapt time where the failure is detectable, not at execution time where it surfaces as an opaque engine error. The `registry/functions_mapping.md` document is the authoritative reference for "what does engine X support"; authors consult it before declaring a `FunctionCall`.

Stub emission and runtime-deferred gaps (Option (b) from the original framing) are rejected — they violate `00 §9`'s I6 (synchronous hot path) / I12 (first-class diagnostics) design posture by pushing a known-at-compile failure to runtime.

## 7. Adapter Extension API

Engine-specific functions — e.g. Spark's `collect_set`, DuckDB's `list_extract`, DataFusion's `array_to_string` — are not canonical, but authors targeting those engines may legitimately want to use them. Adapters register these as extended entries that layer onto the core registry.

### 7.1 Registering an adapter-specific function

**Ratified (Round 1, Q15).** Compile-time **trait-impl** registration:

```rust
pub trait RegistryExtension {
    const ADAPTER_ID: &'static str;
    const FUNCTIONS: &'static [FunctionSpec];
}
```

Adapter crates implement `RegistryExtension` on a zero-size marker type. The canonical registry constructor (`function_registry()` per §2.1) enumerates all linked extensions at process startup and folds their `FUNCTIONS` into the single flat `HashMap` of §2.2. Registration is deterministic and compile-time-verifiable; no runtime mutation API, no macro-collected `inventory` magic, no YAML manifest. Rationale: pairs cleanly with §2.1's `&'static` registry (no lifetime or lock complexity); extension set is visible in the crate dependency graph; collision checks (§7.2) run at registry initialization.

Concrete adapter registration flow:

1. Adapter crate defines `struct MyAdapterFns;` and `impl RegistryExtension for MyAdapterFns { ... }`.
2. The `function_registry()` initializer's seed list includes `MyAdapterFns::FUNCTIONS` — either via a feature-gated array or via a build.rs-generated aggregation (mechanism detail: `[TD-REGISTRY-EXTENSION-WIRING]`).
3. At initialization, each spec is inserted into the flat map; collisions (§7.2) panic — these are fatal configuration errors, not recoverable diagnostics (registry is `&'static`; there is no legal way to proceed).

### 7.2 Collision with reserved AST tags and core entries

**Ratified (Round 1, Q16).** Hard reject at registry initialization — both adapter-vs-core and adapter-vs-adapter. Collisions produce:

- `CompileError::ReservedTagCollision { tag, source }` — name collides with one of `14 §4.4.1`'s 21 reserved AST tags (the only pre-existing variant; covers all three of core-vs-reserved, adapter-vs-reserved).
- `CompileError::AdapterFunctionShadowsCore { name, adapter }` — adapter tried to register a name already in the §4 core catalog.
- `CompileError::AdapterFunctionCollision { name, adapters }` — two adapters registered the same name.

Because the registry is `&'static` (§2.1) and initialization is fatal-on-collision, these errors surface as panics during `function_registry()` initialization rather than as `CompileError` returned from an API — adapter misconfiguration is a build-time problem, not a per-compile user-facing diagnostic. The `CompileError` variants above are the canonical **codes** under which the panic messages are framed (stable panic text, stable code for log-scraping).

No tier precedence (no "core always wins", no "last registered wins"), no signature-compatible shadowing. Engine-specific overrides of canonical entries — e.g. an adapter that wants `upper` to render with different semantics — are expressed via the **rewrite-tier** machinery of `registry/functions_mapping.md`, NOT via re-registration. Rationale: shadowing turns `14a`'s single-source-of-truth invariant into a multi-source-of-ambiguity invariant.

### 7.3 Portability implications when an adapter "owns" a function

A function registered solely by one adapter is, by definition, engine-specific — its `Portability` is at best `EngineSpecific(vec![that_engine])` (whichever shape §6.1 settles on). The core catalog in §4 should NOT include such entries; they live in adapter-contributed tiers. If an engine later adopts a peer-engine's function, the corresponding entry may be migrated core-ward (with a migration note in `registry/functions_mapping.md`). No new Round 1 question here — the rule follows mechanically from §6.1 / §7.1 / §7.2 once those settle.

## 8. Error Model (function-resolution errors feeding 14 §7.3)

The variants below extend / refine the function-resolution slice of `14 §7.3`'s compile-stage error table. Each variant is surfaced as a `Diagnostic` per `10 §5`.

| Variant | Code | When |
|---|---|---|
| `CompileError::UnknownFunction { name, location }` | `EXPR_E_0301` (reused from `14 §7.3`) | `FunctionCall.name` not found in the sealed `FunctionRegistry`. |
| `CompileError::FunctionArityMismatch { name, expected, got, location }` | `EXPR_E_0302` (reused) | Call-site arity is outside the spec's declared range. |
| `CompileError::NoMatchingSignature { name, arg_types, tried_signatures, location }` | `EXPR_E_0303` (reused) | No overload / no TypeClass-binding matches the actual arg types; per `14 §5.6` semstrait does NOT coerce, so the author must `CAST` explicitly. |
| `CompileError::ReservedTagCollision { tag, source, location }` | `EXPR_E_0304` (reused) | Adapter registration attempted to shadow a §4.4.1 reserved AST tag. |
| `CompileError::AdapterFunctionShadowsCore { name, adapter, location }` | `EXPR_E_0305` | Adapter registered a name already in the core catalog of §4. Surfaces as a panic at `function_registry()` initialization (see §7.2 — `&'static` registry cannot return recoverable errors from seal). |
| `CompileError::AdapterFunctionCollision { name, adapters, location }` | `EXPR_E_0306` | Two adapters registered the same name. Same surfacing as the row above. |
| `AdaptError::UnsupportedFunction { name, engine, location }` | `ADAPT_E_0301` | Adapter cannot render a registered function for its target engine and no emulation strategy applies. Hard error per §6.3 — matches the `13` / types_mapping `UnsupportedType` precedent. |

All variants are ratified per §10's Round-1 decisions. Concrete per-engine error messages and the full panic-message format for the two registration-time variants are an adapter-implementation concern (`34` / `36`), not a canonical-spec concern.

## 9. Interaction with Other Documents

- **`14` (expressions)** — upstream consumer. Every `FunctionCall` in the AST flows through this registry at compile time. `14 §4.4.2`'s Declarative-block tag-dispatch consults `lookup` at parse time. `14 §5.6`'s pass-through posture is why §5 of this doc is "reference" and not validation.
- **`14b` (expression resolution)** — consumes the sealed registry during the compile-time substitution algorithm; its cross-DataKind resolution does not alter a `FunctionCall`'s name or args, but it does look up each call once per `(Semantics, Binding)` pair during the type-inference pass.
- **`13` (types and grain)** — every `ParamType` and `ReturnTypeRule` is typed in canonical `DataType` / `TypeClass` vocabulary.
- **`15` (binding)** — unaffected by function resolution directly; physical-column typing is already Semantics-side and the `PhysicalExpr` variant contract forbids `Aggregate` (and by extension `FunctionCategory::Aggregate`) at binding sites.
- **`34` / `36` (adapters)** — extend the registry via §7 and consume resolved `FunctionCall`s at `adapt` time, consulting `registry/functions_mapping.md` for engine-native rendering.
- **`registry/functions_mapping.md`** — per-engine catalog of native names, rewrite tiers, emulation strategies, portability gaps. `14a` carries canonical shape; this doc carries per-engine reality.

## 10. Ratified Decisions Index (Round 1)

| Q | Decision | § |
|---|---|---|
| Q1 | `&'static FunctionRegistry` (process-global static, one configuration per process). `[TD-REGISTRY-MULTI-CONFIG]` tracks future per-invocation configurability. | §2.1 |
| Q2 | Flat `HashMap<&'static str, FunctionSpec>` storage — core and adapter-extended entries share one namespace. | §2.2 |
| Q3 | Strict case-sensitive exact match on a single canonical name per spec. No aliases in v1 (`[TD-REGISTRY-ALIASES]`). Names share namespace with `14 §4.4.1`'s reserved AST tags — collision is `CompileError::ReservedTagCollision`. | §2.3 |
| Q4 | `FunctionSpec` carries `canonical_name`, `category`, `signatures: NonEmpty<FnSignature>`, `description`. No `null_handling`, `deterministic`, `aliases`, `since_version`, `stability`. Determinism flag is `[TD-REGISTRY-DETERMINISM]`. | §3.1 |
| Q5 | Flat `FunctionCategory` — `Scalar \| Aggregate \| Window(deferred)`. Sub-categorization lives in catalog-presentation headings only. `[TD-REGISTRY-SUBCATEGORY]` for future enum splits. | §3.2 |
| Q6 | Pure overload-set polymorphism — `signatures: NonEmpty<FnSignature>`, first exact-match wins. `NoMatchingSignature` carries full overload list as `tried_signatures`. `[TD-REGISTRY-TYPECLASS]` for future TypeClass generics. | §3.3 |
| Q7 | Minimal-4 `ReturnTypeRule` — `Fixed(DataType)`, `SameAs(usize)`, `Promoted(&[usize])`, `Custom(fn)`. No `SameAsTypeVar`, no `NullableOf`. | §3.4 |
| Q8 | `FnSignature { args, variadic: Option<ParamType>, return_type }` — trailing-variadic only. Optional args expressed as multi-overload arity. `[TD-REGISTRY-MID-VARIADIC]` for mid-signature repeated params. | §3.5 |
| Q9 | One row per `FunctionSpec` in catalog tables; overloads collapsed into a single `signature(s)` cell. Columns: name, arity, signatures, return-type rule, notes. No per-engine coverage strip, no stability column. | §4.1 |
| Q10 | Adapter-capability-intersection catalog population — DataFusion ∩ Spark ∩ DuckDB. Non-intersection functions are adapter-extended per §7, not canonical. | §4.1 |
| Q11 | No canonical BinaryOp promotion lattice in v1 — `14 §5.6` pass-through stands; per-engine behavior documented in `registry/functions_mapping.md`. `[TD-REGISTRY-BINOP-LATTICE]` for future canonicalization. | §5.2 |
| Q12 | No `portability` field on `FunctionSpec` — intersection-only population makes every canonical entry portable-by-construction. Per-engine detail lives in `registry/functions_mapping.md`. | §6.1 |
| Q13 | `14a` is engine-agnostic — canonical shape only. `registry/functions_mapping.md` owns all engine-facing detail. | §6.2 |
| Q14 | Hard error at `adapt` time on unsupported function — `AdaptError::UnsupportedFunction`. No runtime-deferred stubs. Matches `13` / types_mapping `UnsupportedType` precedent. | §6.3 |
| Q15 | Compile-time `RegistryExtension` trait-impl registration. `const FUNCTIONS: &'static [FunctionSpec]`. No runtime mutation, no proc-macro collection. Initializer-wiring mechanism: `[TD-REGISTRY-EXTENSION-WIRING]`. | §7.1 |
| Q16 | Hard reject on adapter-vs-core and adapter-vs-adapter collisions — panics during `function_registry()` initialization, carrying `CompileError::AdapterFunctionShadowsCore` / `AdapterFunctionCollision` codes. Engine-specific overrides use rewrite tiers in `registry/functions_mapping.md`, not re-registration. | §7.2 |

### 10.1 Tech-debt / deferred extensions referenced above

- **`[TD-REGISTRY-MULTI-CONFIG]`** — per-invocation or per-Model registry configurations in the same process.
- **`[TD-REGISTRY-ALIASES]`** — spec-level alias support.
- **`[TD-REGISTRY-DETERMINISM]`** — `deterministic: bool` flag (drives constant-folding / equivalence rewrites in the optimizer).
- **`[TD-REGISTRY-SUBCATEGORY]`** — scalar sub-categorization (`Scalar::String` / `Scalar::Math` / …).
- **`[TD-REGISTRY-TYPECLASS]`** — TypeClass-parameterized generic signatures.
- **`[TD-REGISTRY-MID-VARIADIC]`** — mid-signature repeated params.
- **`[TD-REGISTRY-BINOP-LATTICE]`** — canonical BinaryOp promotion lattice with per-engine deviation table.
- **`[TD-REGISTRY-EXTENSION-WIRING]`** — concrete mechanism for aggregating adapter `FUNCTIONS` arrays into the `&'static` initializer (build.rs vs feature-gated array vs proc-macro).

### 10.2 Round 2 scope

Round 2 populates §4.2–§4.6 against the Q10 intersection policy, verifying each candidate against actual DataFusion / Spark / DuckDB function inventories. The error-model table in §8 finalizes the *pending* variants from §6.3 / §7.2 (now ratified). No further framework decisions expected.
