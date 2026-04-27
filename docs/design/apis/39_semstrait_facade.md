---
prereqs: [30, 31, 32, 33, 34, 35, 36, 37, 38]
authoritative-for:
  - the `semstrait` facade crate public-API surface (re-export roster, `prelude::*`, one-shot convenience)
  - the zero-new-logic principle (facade adds NO new types, NO new traits, NO new algorithms)
  - the `prelude::*` module — the named, curated import set callers bring in via `use semstrait::prelude::*;`
  - the `semstrait::run` one-shot convenience free function (compile + plan + adapt, single call)
  - per-adapter and per-catalog feature flags (`datafusion`, `duckdb`, `spark`, `substrait`, `iceberg-rest`, `unity`) and their default-feature composition
  - version alignment posture — facade pins exact versions of every `semstrait-*` sub-crate it re-exports
  - stability tier — facade is the MOST stable surface in the workspace (v1 promise on `prelude::*`)
  - crate-boundary negatives: nothing but re-exports + `prelude::*` + thin conveniences
refined-by:
  - per-adapter-crate appendices (`semstrait-adapter-datafusion`, `-duckdb`, `-spark`, `-substrait`) — their stability tiers carry through the feature gates declared here
  - per-catalog-provider-crate appendices (`semstrait-catalog-iceberg`, `semstrait-catalog-unity`) — same
  - 40 (`implementation/40_refactor_plan.md` — current-vs-target delta for `crates/semstrait/src/`)
---

# 39. semstrait (facade)

> **Status:** Round-1 draft. `39` nails down the public surface of `semstrait` — the **facade crate**, same name as the workspace — as a re-export + `prelude::*` + one-shot-convenience veneer over `semstrait-api` (`38`). Every type exposed here is already ratified in `30`–`38`; `39` adds no new vocabulary, no new types, no new traits, and no new algorithms. It ratifies only **which** subset of the lower-layer surface is promoted to the top-level convenience namespace, **how** feature flags compose against the per-adapter and per-catalog crates, and **what** the `v1` stability promise on `prelude::*` means. Round-1 open items are parked in `questions/open/39_questions.md`.

## Table of Contents

1. [Purpose, Scope, Layering](#1-purpose-scope-layering)
2. [Public Crate Surface](#2-public-crate-surface)
3. [The `prelude::*` Module](#3-the-prelude-module)
4. [One-Shot Convenience Functions](#4-one-shot-convenience-functions)
5. [Feature Flags](#5-feature-flags)
6. [Version Alignment](#6-version-alignment)
7. [Stability](#7-stability)
8. [Crate Boundaries](#8-crate-boundaries)
9. [Round-1 Open Items](#9-round-1-open-items)
10. [Cross-References](#10-cross-references)

---

## 1. Purpose, Scope, Layering

### 1.1 Purpose

`semstrait` is the top-level **facade crate** — the single crate a consumer normally adds to their `Cargo.toml`. It does one job: make the `parse → validate → compile → plan → optimize → adapt` pipeline reachable from a single `use semstrait::prelude::*;` line and, for one-shot callers, from a single `semstrait::run(...)` call.

It ships **no new logic**. Every type, trait, and free function it exposes is re-exported from a lower-layer `semstrait-*` crate (`31`–`38`). The facade's sole contribution is **curation**: picking the minimum set of names a new caller needs to type, arranging them into ergonomic modules, and pinning the feature gates for the optional per-adapter / per-catalog crates.

### 1.2 Scope — what this crate owns

- The **roster of re-exports** from every `semstrait-*` sub-crate (`§2.2`) — one `pub use` entry per promoted item, module-grouped.
- The **`prelude::*` module** (`§3`) — a named, curated subset of the re-exports intended for `use semstrait::prelude::*;` onboarding.
- The **`semstrait::run` one-shot convenience** (`§4`) — a thin free function that stitches `compile → plan → optimize → adapt` for simple callers (scripts, tests, demos).
- The **feature-flag policy** (`§5`) — which per-adapter and per-catalog optional dependencies are gated behind which feature, and what the `default` features compose.
- The **workspace-version pin** (`§6`) — the facade depends on each sub-crate at an exact workspace version.
- The **v1 stability promise** on `prelude::*` (`§7`).

### 1.3 Scope — what this crate does NOT own

- **No new types.** If `semstrait::Foo` compiles, `Foo` is re-exported from `semstrait-core`, `-model`, `-manifest`, `-planner`, `-ir`, `-adapter`, `-catalog`, or `-api`. Zero `pub struct`, `pub enum`, `pub trait`, `pub fn` are declared inside `crates/semstrait/src/` except the single `semstrait::run` convenience in `§4`.
- **No algorithm logic.** `semstrait::run` is the entirety of the algorithmic surface in this crate, and its body is a straight chain of sub-crate calls with no branching beyond error propagation.
- **No parsing, no planning, no adaptation, no I/O.** Every one of those lives in a lower crate; the facade merely re-exports their entry points.
- **No prelude aliases or short names.** If `semstrait-core` names a type `SemanticExpr`, the facade re-exports it as `semstrait::SemanticExpr` (and `semstrait::prelude::SemanticExpr`). No `semstrait::SemExpr` shortening, no `use … as …` rebranding.

### 1.4 Layering

```
caller (end-user application, script, test harness)
    ↓ depends on
semstrait            (facade — re-exports + prelude + run)
    ↓ depends on
semstrait-api        (38 — unified entry: SemStrait, SemStraitBuilder, SemStraitError)
    ↓ depends on
semstrait-manifest   semstrait-planner   semstrait-adapter   semstrait-catalog
(33)                 (34)                (36)                (37)
    ↓
semstrait-ir  (35)   semstrait-model (32)
    ↓
semstrait-core (31)
```

`semstrait` sits at the apex of the workspace DAG (I7). Every other `semstrait-*` crate is a direct or transitive dependency. No workspace crate imports from `semstrait` — the facade is strictly terminal.

### 1.5 Design posture — "add a crate, don't add ceremony"

The facade's reason-to-exist is ergonomic. A consumer who writes

```rust
use semstrait::prelude::*;

let model = parse(&yaml)?;
let manifest = compile(model, &registry).await?;
let plan = plan(&manifest, &request)?;
let artifact = AnsiSqlAdapter::default().adapt(&plan, &manifest)?;
```

should not need to know that `parse` lives in `semstrait-model`, `compile` in `semstrait-manifest`, `plan` in `semstrait-planner`, and `AnsiSqlAdapter` in `semstrait-adapter`. The facade collapses five `use` lines into one and pins the sub-crate versions so a `cargo update` on `semstrait` is a single coordinated bump (`30 §2.1`).

Consumers who DO need sub-crate-level control (pick a different adapter registry, supply a custom `Repository`, pin a different `semstrait-catalog` minor) depend on `semstrait-api` directly and bypass this crate. `39` is a convenience, not a chokepoint.

---

## 2. Public Crate Surface

### 2.1 Roster

| Name / module | Kind | Source crate | Purpose |
|---|---|---|---|
| `core`        | module (re-export) | `semstrait-core` (`31`)         | Shared primitives: `Expr` family, `DataType`, `Diagnostic`, `FunctionRegistry`, `CanonicalFn`. |
| `model`       | module (re-export) | `semstrait-model` (`32`)        | `SemanticModel`, `parse`, `ParseError`. |
| `manifest`    | module (re-export) | `semstrait-manifest` (`33`)     | `Manifest`, `compile`, `CompileError`, `Repository` + bundled impls. |
| `planner`     | module (re-export) | `semstrait-planner` (`34`)      | `Request`, `SessionContext`, `plan`, `optimize`, `PlanError`, `OptimizeError`. |
| `ir`          | module (re-export) | `semstrait-ir` (`35`)           | `SemanticPlan`, `PlanNode`, `EngineArtifact`, `DialectId`. |
| `adapter`     | module (re-export) | `semstrait-adapter` (`36`)      | `EngineAdapter`, `AdaptError`, `AdapterCapabilities`, `AnsiSqlAdapter`, `SubstraitAdapter`. |
| `catalog`     | module (re-export) | `semstrait-catalog` (`37`)      | `CatalogProvider`, `FileSystem`, `NoopCatalogProvider`, `LocalFileSystem`, built-in provider set. |
| `api`         | module (re-export) | `semstrait-api` (`38`)          | `SemStrait`, `SemStraitBuilder`, `SemStraitError`. |
| `prelude`     | module             | (curated subset)                | See `§3`. |
| `run`         | `pub fn`           | (this crate — §4)               | One-shot `compile → plan → optimize → adapt`. |
| `VERSION`     | `pub const`        | (this crate)                    | Workspace version string (`env!("CARGO_PKG_VERSION")`). |

"Kind" is `module (re-export)` when the facade simply re-exports the sub-crate's public surface wholesale — i.e. `semstrait::core` is isomorphic to `semstrait_core`, `semstrait::model` to `semstrait_model`, etc. No filtering occurs at the module-level; filtering is applied only in `prelude::*` (`§3`).

### 2.2 Module-level re-exports

```rust
// crates/semstrait/src/lib.rs

pub use semstrait_core       as core;
pub use semstrait_model      as model;
pub use semstrait_manifest   as manifest;
pub use semstrait_planner    as planner;
pub use semstrait_ir         as ir;
pub use semstrait_adapter    as adapter;
pub use semstrait_catalog    as catalog;
pub use semstrait_api        as api;

pub mod prelude;                     // §3

pub use crate::run::run;             // §4

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
```

Every public symbol of every sub-crate is reachable through `semstrait::<module>::<symbol>` without a second `use` line on that sub-crate. This matches `30 §3.4`'s re-export policy — re-exports carry the same `#[non_exhaustive]` annotations as the underlying types; no annotation is relaxed at the re-export boundary.

### 2.3 What is NOT re-exported at the crate root

Two conventions:

1. **Crate-root re-exports are MODULE-level only.** `semstrait::core::DataType` works; `semstrait::DataType` does NOT. The crate root is deliberately uncluttered — consumers who want flat access use `prelude::*` (`§3`).
2. **No leaking of sub-crate-private items.** The facade re-exports sub-crate roots exactly as-is. An item that is `pub(crate)` in `semstrait-core` remains `pub(crate)` when accessed via `semstrait::core`; no `#[doc(hidden)]` items are promoted to `#[doc(visible)]`.

### 2.4 Module layout (informative)

```rust
crates/semstrait/
├── Cargo.toml          // optional deps on every semstrait-* sub-crate + adapter / catalog crates
└── src/
    ├── lib.rs          // module re-exports (§2.2), prelude mod, VERSION const
    ├── prelude.rs      // §3
    └── run.rs          // §4 — semstrait::run implementation
```

Two source files outside `lib.rs`. `prelude.rs` is a straight sequence of `pub use` lines; `run.rs` is the single-function body of `§4`. Nothing else.

---

## 3. The `prelude::*` Module

### 3.1 Purpose

`semstrait::prelude` is the **rapid-onboarding import set** — the named bundle of types a caller brings in via

```rust
use semstrait::prelude::*;
```

to immediately write against the facade without knowing the sub-crate layout. It is curated, not exhaustive: every module-level re-export in `§2.2` exposes the full sub-crate surface; the `prelude::*` set is the subset of those items a *typical* caller uses when wiring the canonical pipeline from end to end.

The prelude pattern follows `31`'s precedent — `31 §14.10` sketched a crate-root re-export for `semstrait-core`; `39 §3` is the analogous curated bundle at the workspace apex.

### 3.2 Member set

```rust
// crates/semstrait/src/prelude.rs

// -- core (31) --
pub use crate::core::{
    Diagnostic,
    Severity,
    DataType,
    CanonicalFn,
};

// -- model (32) --
pub use crate::model::{
    SemanticModel,
    parse,
};

// -- manifest (33) --
pub use crate::manifest::{
    Manifest,
    compile,
    Repository,
    InMemoryRepository,
    FileSystemRepository,
};

// -- planner (34) --
pub use crate::planner::{
    Request,
    SessionContext,
    plan,
    optimize,
};

// -- ir (35) --
pub use crate::ir::{
    SemanticPlan,
    PlanNode,
    EngineArtifact,
    DialectId,
    Name,
};

// -- adapter (36) --
pub use crate::adapter::{
    EngineAdapter,
    AnsiSqlAdapter,
};

#[cfg(feature = "datafusion")]
pub use crate::adapter::DataFusionSqlAdapter;

// (duckdb / spark / substrait follow the same pattern per §5.2)

// -- catalog (37) --
pub use crate::catalog::{
    CatalogProvider,
    FileSystem,
    NoopCatalogProvider,
};

// -- api (38) --
pub use crate::api::{
    SemStrait,
    SemStraitBuilder,
};
```

### 3.3 Membership principles

A name is in `prelude::*` only when every one of the following holds:

1. **First-call relevance.** The caller reaches for it on their first pass through the pipeline (e.g. `parse`, `compile`, `plan`, `adapt`). Helper types a caller rarely constructs by hand (e.g. `CoverageIndex`, `ResolvedExprTable`, `FnSignature`, `AdapterCapabilities`) are re-exported through `semstrait::<module>::*` but NOT through the prelude.
2. **Unambiguous short name.** The name does not collide with common std / tokio / serde / tracing names. This rules out e.g. a bare `Path` (collides with `std::path::Path`) — the catalog `Path` newtype (`37 §2.3`) is reachable through `semstrait::catalog::Path`, not the prelude.
3. **Stable in v1.** Only items with `Stable` v1 stability (`30 §13`) are in the prelude. Items with `Provisional` stability (e.g. the `semstrait-adapter` and `semstrait-catalog` error enums) are reachable through their sub-crate modules; they may enter the prelude in a MINOR once promoted.

Adding a name to `prelude::*` is additive (MINOR, `30 §2.1`). Removing a name is additive in the `#[deprecated]` sense — the name is marked deprecated for at least one MINOR cycle before removal (MAJOR, `30 §12`). The prelude's **existence** — that `use semstrait::prelude::*;` keeps compiling across v1.x — is the v1 promise ratified in `§7`.

### 3.4 What the prelude deliberately OMITS

The following are reachable through `semstrait::<module>::*` but deliberately excluded from `prelude::*`:

- Every `*Error` / `*Errors` enum (consumers prefer `Result<_, Diagnostic>` at API boundaries per `30 §5.5`).
- Every `Resolved*` type from `semstrait-manifest` (consumed at `compile` boundary, not constructed by hand).
- `PhysicalExpr` / `SemanticExpr` / `Expr` (consumers rarely construct AST nodes directly; authors consume them through `ExprSource` parse boundaries).
- `FunctionRegistry`, `FunctionSpec`, `FnSignature` (registry is process-global via `core::function_registry()`; rarely referenced by name).
- Every per-engine adapter type beyond `AnsiSqlAdapter` / the feature-gated `DataFusionSqlAdapter` (the default bundle — see `§5`).
- Every `*Id` newtype (`CatalogId`, `SourceId`, `ManifestId`) — consumers reference them by value, not by type path.

### 3.5 Glob discipline

`use semstrait::prelude::*;` is the intended one-liner; mid-file single-item imports from the prelude (e.g. `use semstrait::prelude::DataType;`) are equally supported. The prelude is not a "namespace" — it is a re-export module. Each member is the exact same type identity as the sub-crate definition.

---

## 4. One-Shot Convenience Functions

### 4.1 `semstrait::run`

```rust
/// Compile a YAML model, plan a request against it, optimize the plan,
/// and adapt the optimized plan into an `EngineArtifact` — in a single
/// call. Intended for scripts, tests, and demos where sub-crate-level
/// control is not needed.
///
/// Equivalent hand-wired form:
///
/// ```ignore
/// let model = semstrait::model::parse(yaml)?;
/// let manifest = semstrait::manifest::compile(model, &registry).await?;
/// let plan = semstrait::planner::plan(&manifest, &request)?;
/// let plan = semstrait::planner::optimize(plan)?;
/// let artifact = adapter.adapt(&plan, &manifest)?;
/// ```
///
/// The callable-facing catalog is fixed to `NoopCatalogProvider` —
/// `semstrait::run` does NO external catalog I/O. Callers who need
/// catalog resolution at compile time drop to the `SemStrait` /
/// `SemStraitBuilder` surface (`38`) directly.
pub async fn run(
    yaml: &str,
    request: Request,
    adapter: &dyn EngineAdapter,
) -> Result<EngineArtifact, SemStraitError>;
```

### 4.2 Contract

- **Pure over inputs.** `run` performs no I/O beyond what the `Repository` / `CatalogProvider` would have done (and in the default wiring, neither runs — `run` uses `InMemoryRepository` + `NoopCatalogProvider`).
- **Async.** Inherits `compile`'s async posture per `30 §9` / `33 §9`. The planner and adapter hot paths remain synchronous (I6); the sole `.await` in `run`'s body is on `compile`.
- **Fail-fast.** Propagates the first stage error as a `SemStraitError` — the unified error type ratified in `38`. Warnings accumulated by upstream stages are preserved on the `Ok` arm where the stage contract carries them (`30 §7`).
- **Deterministic.** For identical `(yaml, request)` inputs and an adapter whose `adapt` is deterministic, `run` is byte-deterministic in its `EngineArtifact` output. This falls out of I4 (`00 §9`).

### 4.3 What `run` is NOT

- **Not a router.** `run` takes exactly one adapter; dispatching across engines is a `SemStrait` / `AdapterRegistry` concern (`36 §11`, `38`).
- **Not a loader.** `run` takes a `&str`; reading YAML from disk, merging multi-file models, or resolving `$include` directives is a caller responsibility (`32 §10.3`).
- **Not a cache.** `run` compiles every invocation. Persistent `Manifest` caching goes through `Repository::save` / `Repository::load` (`33 §11`).
- **Not a harness for streaming or incremental planning.** One input, one output, one await.

### 4.4 No other free functions

The facade surface deliberately stops at `run`. Additional one-shots — `semstrait::run_sql`, `semstrait::plan_only`, `semstrait::compile_file` — were considered and rejected: each adds a new maintenance point that duplicates a sub-crate call chain with zero additional value. A caller who needs a two-stage one-shot wires it themselves in three lines; the facade does not grow to host it.

---

## 5. Feature Flags

### 5.1 Default features

`semstrait`'s `default` features compose the minimum useful bundle for most callers:

```toml
[features]
default = ["ansi-sql"]
```

`ansi-sql` pulls in `AnsiSqlAdapter` from `semstrait-adapter` (which is always present — it ships in the adapter crate itself, per `36 §5`). The intent: a consumer who writes `cargo add semstrait` gets a working `prelude::*` against a dialect-neutral SQL emitter without touching a single adapter option.

No per-engine adapter is enabled by default, because each one pulls a heavy transitive crate (`datafusion`, `duckdb`, `spark-*`, `substrait`) that most callers do not need. `30 §10.5` bans `default = ["every-adapter"]` at the policy level; `39` upholds that.

### 5.2 Per-adapter feature flags

Each per-engine adapter crate ships as a separate workspace crate (`30 §10.2`); the facade exposes each one behind a feature flag:

| Feature | Optional dep | Re-exports |
|---|---|---|
| `datafusion`  | `semstrait-adapter-datafusion`  | `adapter::DataFusionSqlAdapter` |
| `duckdb`      | `semstrait-adapter-duckdb`      | `adapter::DuckDbSqlAdapter`     |
| `spark`       | `semstrait-adapter-spark`       | `adapter::SparkSqlAdapter`      |
| `substrait`   | `semstrait-adapter-substrait`   | `adapter::SubstraitAdapter`     |
| `ansi-sql`    | (none — always in `semstrait-adapter`) | `adapter::AnsiSqlAdapter` |

Turning on `features = ["datafusion"]` pulls the `semstrait-adapter-datafusion` crate and the corresponding prelude entry (`DataFusionSqlAdapter`). Consumers compose features additively: `features = ["datafusion", "duckdb"]` gets both adapters.

### 5.3 Per-catalog feature flags

Catalog providers follow the same pattern (`30 §10.3`):

| Feature | Optional dep | Re-exports |
|---|---|---|
| `iceberg-rest` | `semstrait-catalog-iceberg` | `catalog::IcebergRestCatalogProvider` |
| `unity`        | `semstrait-catalog-unity`   | `catalog::UnityCatalogProvider`        |

`NoopCatalogProvider`, `LocalFileSystem`, and `FilesystemCatalogProvider` always ship in `semstrait-catalog` (`37 §4.1`, `§4.4`, `§6.1`) and are always reachable through `semstrait::catalog::*` without a feature. Only the remote providers (`iceberg-rest`, `unity`) — which pull HTTP / auth stacks — are feature-gated.

### 5.4 Composition and independence

Features compose additively per `30 §10.5` — there are no mutually exclusive features within the facade. A consumer enabling

```toml
semstrait = { version = "1.0", features = ["datafusion", "iceberg-rest"] }
```

gets DataFusion adapter + Iceberg REST catalog + the default `ansi-sql` bundle. Adapter features and catalog features are independent axes — a consumer may enable any catalog with any adapter (`00 §3` "Where metadata sources fit").

### 5.5 Optional serde

Following `30 §10.4`: the facade exposes a `serde` feature that turns on `serde` across every sub-crate that gates serialization behind the same feature:

```toml
[features]
serde = [
    "semstrait-core/serde",
    "semstrait-model/serde",
    "semstrait-manifest/serde",
    "semstrait-ir/serde",
]
```

Default-OFF, per `30 §10.4`. Consumers who serialize `Manifest` / `SemanticPlan` / `Diagnostic` opt in.

### 5.6 Reserved feature names

The following feature names are RESERVED against future per-engine and per-catalog additions; using one for any other purpose is forbidden:

- Per-engine: `clickhouse`, `trino`, `snowflake`, `bigquery`, `postgres`.
- Per-catalog: `polaris`, `glue`, `hms`, `tabular`.

Reservation does NOT imply a commitment to ship; it prevents name collisions when those integrations land. A new per-engine or per-catalog feature follows the same pattern ratified in `§5.2` / `§5.3`.

---

## 6. Version Alignment

### 6.1 Lockstep coordinated release

Per `30 §2.1`, every `semstrait-*` crate in this workspace ships a single coordinated version. The facade upholds that by pinning each sub-crate at an **exact** workspace version in its `Cargo.toml`:

```toml
# crates/semstrait/Cargo.toml — authoritative pattern

[dependencies]
semstrait-core      = { version = "=1.0.0", path = "../semstrait-core"      }
semstrait-model     = { version = "=1.0.0", path = "../semstrait-model"     }
semstrait-manifest  = { version = "=1.0.0", path = "../semstrait-manifest"  }
semstrait-planner   = { version = "=1.0.0", path = "../semstrait-planner"   }
semstrait-ir        = { version = "=1.0.0", path = "../semstrait-ir"        }
semstrait-adapter   = { version = "=1.0.0", path = "../semstrait-adapter"   }
semstrait-catalog   = { version = "=1.0.0", path = "../semstrait-catalog"   }
semstrait-api       = { version = "=1.0.0", path = "../semstrait-api"       }

[dependencies.semstrait-adapter-datafusion]
version  = "=1.0.0"
path     = "../semstrait-adapter-datafusion"
optional = true

# ... one entry per optional per-adapter / per-catalog crate
```

`=1.0.0` (exact-version pin) — not `^1.0.0` or `1.0` — because the facade re-exports types by identity (`30 §11.3`). A caller who consumes `semstrait::manifest::Manifest` and `semstrait-manifest::Manifest` directly MUST see the same type; cargo's MINOR-compatibility semantics would silently allow a newer patched `semstrait-manifest` to slide in and break type-identity across the boundary.

### 6.2 Release rhythm

- **PATCH.** Every sub-crate bumps patch together; the facade bumps to the same patch; `cargo publish` runs across the workspace in one sweep.
- **MINOR.** Same rhythm; new variants / new fields / new prelude additions land in one coordinated MINOR.
- **MAJOR.** The facade's MAJOR matches the workspace MAJOR (`30 §2.1`); `prelude::*` evolution across a MAJOR is documented in `implementation/42_migration_notes.md`.

### 6.3 Consumer guidance

A consumer who writes

```toml
[dependencies]
semstrait = "1.0"
```

gets the facade plus pinned sub-crate versions consistent with it; **never** a mixed-version workspace. Mixing sub-crate versions by hand — e.g. bumping `semstrait-manifest = "1.1"` while `semstrait = "1.0"` is still on `=1.0.0` — fails to compile because the facade's pin conflicts. This is intentional: `30 §11.3`'s cross-crate-break guarantee rests on it.

---

## 7. Stability

### 7.1 The most stable surface

`semstrait` is ratified in `30 §13` as **Stable in v1** with the strongest promise in the workspace:

> *A `use semstrait::prelude::*;` written against v1.0 MUST compile against every v1.x release.*

"Compile" is the binding word: the promise is type-identity stability, not body-behavior stability. An upstream MINOR may widen a type's field set (`#[non_exhaustive]`), add a variant, relax a method bound, or introduce a new stage diagnostic — the caller's code continues to compile.

### 7.2 What the promise does NOT cover

- **Warnings.** A v1.x MINOR may introduce new `#[deprecated]` attributes that produce warnings against a v1.0 call site. Per `30 §12.1`, deprecation is additive; callers have one MINOR cycle to migrate before a future MAJOR removes the symbol.
- **Wildcard matches.** A caller who pattern-matches on a `#[non_exhaustive]` enum without a wildcard arm is already violating `30 §4.4`. The facade cannot shield such code from MINOR variant additions.
- **Runtime behavior.** The promise is compile-level. Byte-identical `SemanticPlan` / `EngineArtifact` outputs are a per-sub-crate determinism concern (I4); `39` inherits, does not amplify.

### 7.3 Breaking changes within v1

Breaking changes to any sub-crate propagate through the facade identically (per `30 §11.3`): a `semstrait-manifest` MAJOR is a `semstrait` MAJOR. Within v1, no such break is admitted — the coordinated-release discipline ensures the whole workspace stays on v1.x until a MAJOR cut.

Prelude-specific breaks (a name removed from `prelude::*`, a name retyped incompatibly) follow `30 §12`: deprecated for at least one MINOR before removal.

### 7.4 Migration policy for `prelude::*` growth

A name enters `prelude::*` by appearing in `crates/semstrait/src/prelude.rs` in a MINOR release — additive, no migration note needed beyond a one-line changelog entry.

A name leaves `prelude::*` by:

1. Landing with `#[deprecated(since = "…", note = "use `semstrait::<module>::<name>` instead")]` in MINOR N.
2. Removal in MAJOR N+1.
3. `implementation/42_migration_notes.md` entry covering the removal.

---

## 8. Crate Boundaries

| Boundary | Status |
|---|---|
| New types | **NO.** Zero `pub struct` / `pub enum` / `pub trait` declared in `crates/semstrait/src/`. |
| New free functions | **ONE (`semstrait::run`, §4).** No others; additions are amendments against `§4.4`. |
| New logic / algorithms | **NO.** `semstrait::run`'s body is a straight chain of sub-crate calls; every other facade surface is `pub use`. |
| Feature-gated re-exports | **YES.** Per `§5.2` / `§5.3`; gated behind the same feature names across `Cargo.toml` and `lib.rs`. |
| Prelude curation | **YES.** §3. Adding / removing names follows §7.4. |
| Name rebranding / aliasing | **NO.** Re-exports preserve the sub-crate name exactly. |
| Workspace-internal dependency | **YES — on every sub-crate at an exact pin.** §6.1. |
| Upward workspace dependency | **NO.** The facade is terminal; no workspace crate imports `semstrait`. |
| I/O | **NO** direct I/O. Any I/O reachable through the facade is performed by the sub-crate that owns it (e.g. `compile` via `CatalogProvider`, `Repository::load` via `FileSystem`). |
| Async | **Inherited.** `semstrait::run` is `async fn` because `compile` is (`33 §9`). Non-compile paths reachable through the facade are synchronous. |
| Documentation | **YES.** Every re-exported item inherits its sub-crate doc comment; the facade adds no per-item doc overrides. |

The entirety of `crates/semstrait/src/` in v1 is expected to be well under 200 source lines: the `lib.rs` re-export block, the `prelude.rs` bundle, and the `run.rs` single function.

---

## 9. Round-1 Open Items

The following drafting decisions are **defaulted** in this document but MUST be confirmed before v1 ratification. All are captured in `docs/design/questions/open/39_questions.md`:

- **Q-FAC-001** — Default-feature composition: `default = ["ansi-sql"]` vs `default = []`. Current default: `ansi-sql` on, to give `cargo add semstrait` a working adapter out of the box.
- **Q-FAC-002** — Prelude membership of `Name` (`semstrait-ir §5.4`): promoted to the prelude (current default) vs reachable only through `semstrait::ir::Name`. The IR `Name` newtype is not a common first-touch type; concern is resolved one way or the other at `38` ratification.
- **Q-FAC-003** — `semstrait::run` error type: `SemStraitError` (current default, delegated to `38`) vs `Diagnostic` (align with `30 §5`'s "public APIs return `Diagnostic`" principle). Parked until `38` lands its `SemStraitError` shape.
- **Q-FAC-004** — `semstrait::run` catalog-wiring: hard-coded `NoopCatalogProvider` (current default, zero-I/O one-shot) vs caller-supplied `&dyn CatalogProvider` parameter. The latter breaks the "one-shot" ergonomic; the former rules out catalog-bound one-shots.
- **Q-FAC-005** — Exact-version pinning of sub-crates: `=1.0.0` (current default, upholds `30 §2.1`) vs `~1.0` (patch-compatible) vs `^1.0` (minor-compatible). Type-identity argument in `§6.1` favors exact; check against `cargo-semver-checks` before ratification.
- **Q-FAC-006** — Reserved feature names (`§5.6`): worth enumerating pre-emptively, or open-namespace (first-come-first-served)? Current default: reserve.
- **Q-FAC-007** — Prelude growth budget: should the prelude cap at ~25 names to remain scannable, or grow organically with sub-crate additions? Current default: no hard cap; membership principles in `§3.3` apply.
- **Q-FAC-008** — `semstrait::VERSION` constant usefulness: ship it (current default) vs rely on `env!("CARGO_PKG_VERSION")` inline at consumer site.

Each item is parked with arguments-for, arguments-against, and a next-step in `questions/open/39`.

---

## 10. Cross-References

- Overview: `00 §4.1` (canonical vocabulary — every re-exported type defined there), `00 §9` (design invariants I7, I10 uphold through the re-export boundary).
- API contracts: `30 §2.1` (coordinated-release semver), `30 §3.4` (re-export policy), `30 §10` (feature-flag policy), `30 §13` (stability table; facade row).
- Sub-crate contracts being re-exported: `31` (`semstrait-core`), `32` (`semstrait-model`), `33` (`semstrait-manifest`), `34` (`semstrait-planner`), `35` (`semstrait-ir`), `36` (`semstrait-adapter`), `37` (`semstrait-catalog`), `38` (`semstrait-api`).
- Implementation: `implementation/40_refactor_plan.md` — current-vs-target delta for `crates/semstrait/src/`.

---

## 11. Round-1 Ratifications

- §2.1 — Roster of module-level re-exports (one module per sub-crate, plus `prelude` and `run`).
- §2.2 — Every sub-crate re-exported wholesale at `semstrait::<module>`; no filtering at the module level.
- §3.2 — Prelude member list (core / model / manifest / planner / ir / adapter / catalog / api subsets).
- §3.3 — Membership principles: first-call relevance, unambiguous short name, v1-stable.
- §4.1 — `semstrait::run` signature: `async fn run(yaml: &str, request: Request, adapter: &dyn EngineAdapter) -> Result<EngineArtifact, SemStraitError>`.
- §4.4 — No other one-shot free functions in v1.
- §5.1 — `default = ["ansi-sql"]`.
- §5.2 / §5.3 — Per-adapter features (`datafusion`, `duckdb`, `spark`, `substrait`) and per-catalog features (`iceberg-rest`, `unity`).
- §6.1 — Exact-version (`=1.0.0`) pinning for every sub-crate dependency.
- §7.1 — v1 promise: `use semstrait::prelude::*;` compiled against v1.0 compiles against every v1.x.
- §8 — Crate boundaries: zero new types, zero new logic (except `run`), no upward workspace dependency.

Exact feature-name literals in §5 are ratified; default-feature contents (`§5.1`) and prelude membership at the edges (`§3.2`) may be amended under the open items in §9 without touching the structural ratifications above.

---

*Cross-references in this document are by section (e.g. `30 §2.1`, `33 §9`, `37 §4.1`). No code-path references are used, per `00 §8`.*
