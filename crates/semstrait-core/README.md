# semstrait-core

The compilation engine. Transforms semantic model definitions and query requests into SQL and Substrait compute plans.

No I/O. No network. No engine dependencies. Pure transformation logic.

---

## Design Decisions

1. **PlanNode is the IR; SQL and Substrait are output formats.** Per CONTEXT.md Design Principle #2: "Substrait is the output, not the IR." All semantic resolution (kind routing, additivity, temporal filters) happens inside the planner on PlanNode trees. Substrait protobuf is emitted only after all decisions are made.

2. **ANSI SQL identifier quoting.** All identifiers (table names, column names, aliases) are wrapped in double quotes via `quote_ident()` to prevent SQL injection and handle reserved words.

3. **Composite join keys.** `JoinEdge.column_pairs: Vec<(String, String)>` supports multi-column joins. SQL ON clauses join pairs with AND. Direction flipping (left/right) applied when BFS traverses relationships in reverse.

4. **Duplicate name validation at parse time.** Kind names, dataset names within kinds, and dimension/measure names within kinds are checked for uniqueness during parsing via `validate_structure()`.

5. **Named column references, not positional.** PlanNode uses `Column { relation, name }` throughout. A final Project node renames physical columns to semantic names.

---

## Responsibility

`semstrait-core` owns the complete pipeline from YAML semantic model to compiled query output:

```
YAML file
  |  parser::load / parser::refs / parser::nesting
  v
SemanticModel (schema::model)  <- typed representation of the v1.2 semantic model
  |  compiler::find_matching_kind
  v
Kind selection                 <- best kind for requested dimensions/measures/domain
  |  kind_resolver::resolve_kind
  v
ResolvedKind                   <- GrainsetPlan / UnionsetPlan / JoinsetPlan
  |  plan_builder::build_plan
  v
PlanNode  (pub(crate))         <- private relational algebra tree (the IR)
  |
  +- sql_emitter::emit_sql ----------> String        ANSI SQL
  |
  +- substrait_conv::emit_plan ------> substrait::Plan  protobuf bytes
                  assembled into
                       v
CompiledPlan  (pub)            <- the public output
```

Everything above `CompiledPlan` is an implementation detail of this crate.

---

## Module map

```
semstrait-core/src/
+-- lib.rs                       Public API surface
+-- compiler.rs                  SemanticCompiler trait + StatelessCompiler
+-- output.rs                    CompiledPlan, OutputColumn, CompileOpts
+-- diagnostics.rs               Diagnostic, CompileError, ValidationReport, error codes
+-- registry.rs                  SchemaRegistry trait + FileSystemRegistry + InMemoryRegistry
+-- schema/
|   +-- mod.rs
|   +-- model.rs                 SemanticModel, Kind, Dataset, Dimension, Measure, Metric
|   +-- types.rs                 DataType, Aggregation (with aliases)
+-- parser/                      (internals private: nesting, refs)
|   +-- mod.rs                   parse_file(), parse_str()
|   +-- load.rs                  YAML loading, deserialization, validate_structure()
|   +-- refs.rs                  Reference resolution ($ref)
|   +-- nesting.rs               Nesting matrix validation
+-- dsl/                         DSL expression system (internals private: ast, lexer, token)
|   +-- mod.rs                   parse_dsl(), lower_expr(), lower_aggregate()
|   +-- ast.rs                   DslExpr AST
|   +-- token.rs                 Token types
|   +-- lexer.rs                 Tokenizer (logos)
|   +-- parser.rs                DSL string -> DslExpr
|   +-- lower.rs                 DslExpr -> planner Expr, lower_aggregate()
+-- planner/                     Kind resolution + PlanNode construction + emission
    +-- mod.rs                   Submodule declarations (ir, resolve, validate, transform, build, emit)
    |
    +-- ir/                      IR Foundation
    |   +-- mod.rs               Re-exports: Expr, Column, PlanNode, EmitError
    |   +-- expr.rs              Expr, Column, AggregateExpr, BinaryOperator, Literal
    |   +-- plan_node.rs         PlanNode enum + all node structs (Scan, Join, Filter, etc.)
    |   +-- error.rs             EmitError (thiserror)
    |
    +-- resolve/                 Kind Resolution (trait: KindResolver)
    |   +-- mod.rs               KindResolver trait, QueryRequest, ResolvedKind, resolve_kind()
    |   +-- grainset.rs          GrainsetResolver — dataset selection by grain coverage
    |   +-- unionset.rs          UnionsetResolver — UNION ALL with NULL-fill
    |   +-- joinset.rs           JoinsetResolver — BFS join tree via petgraph
    |
    +-- validate/                Validation & Constraints
    |   +-- mod.rs               Re-exports
    |   +-- constraints.rs       Measure constraint checking (require_dimension, etc.)
    |   +-- additivity.rs        Additivity resolution + semi-additive wrapping
    |   +-- metric_chain.rs      Metric chaining depth validation (max 3, no cycles)
    |   +-- domain.rs            Domain prefix matching and filtering
    |
    +-- transform/               Expression Transformers
    |   +-- mod.rs               Re-exports
    |   +-- temporal.rs          Temporal filters (SCD type 2/5, snapshot)
    |   +-- bucketed.rs          Bucketed dimension CASE WHEN compilation
    |
    +-- build/                   Plan Building (ResolvedKind -> PlanNode)
    |   +-- mod.rs               build_plan() entry point, metric dependency resolution
    |   +-- grainset.rs          Grainset plan: single-dataset or multi-dataset UNION
    |   +-- unionset.rs          Unionset plan: UNION ALL branches
    |   +-- joinset.rs           Joinset plan: JOIN tree construction
    |   +-- common.rs            Shared helpers (scan, aggregate, project, additivity wrap)
    |
    +-- emit/                    Plan Emission (trait: PlanEmitter)
        +-- mod.rs               PlanEmitter trait, SqlEmitter, SubstraitEmitter
        +-- sql.rs               emit_sql() — PlanNode -> ANSI SQL (with quote_ident)
        +-- substrait.rs         emit_plan() — PlanNode -> substrait::proto::Plan (protobuf)
```

---

## Compilation pipeline

### 1. Parse (parser/)

`parse_file()` / `parse_str()` loads YAML into `ModelFile { semantic_model: SemanticModel }`. Validates references, nesting depth, structural constraints, and duplicate names.

### 2. Kind selection (compiler.rs)

The compiler iterates over `model.kinds`, filtering by domain prefix match and checking that all requested dimensions/measures/metrics exist in the kind. First matching kind wins.

If no kind matches, falls back to dataset-based compilation.

### 3. Kind resolution (planner/kind_resolver.rs)

Dispatches to the appropriate resolver based on kind type:

- **Grainset** -- selects the best dataset by temporal grain match, produces column mappings (semantic -> physical)
- **Unionset** -- selects branches that have any requested columns, NULL-fills missing columns
- **Joinset** -- builds a join tree from the relationship graph via BFS, prunes unused joins, supports composite keys

### 4. Plan building (planner/plan_builder.rs)

Converts `ResolvedKind` into a `PlanNode` tree:

- **Grainset (single)**: `Scan -> [Filter] -> [BucketProject] -> Aggregate -> [AdditivityWrap] -> Project -> [MetricProject]`
- **Grainset (multi)**: per-dataset `Scan -> [Filter] -> Project` -> `Union -> Aggregate -> [AdditivityWrap] -> [MetricProject]`
- **Unionset**: per-branch `Scan -> [Filter] -> Project(NULL-fill)` -> `Union -> Aggregate -> [AdditivityWrap]`
- **Joinset**: `Scan(anchor) -> Join chain -> Project -> Aggregate -> [AdditivityWrap]`
- **Dataset fallback**: `Scan -> [Filter] -> Aggregate` (for models without kinds)

Features wired into plan building:
- **Measure-level filters**: CASE WHEN wrapping inside aggregate expressions
- **Bucketed dimensions**: CASE WHEN Project before Aggregate for computed dimensions
- **Semi-additive measures**: two-stage aggregation (inner: resolution strategy, outer: declared agg)
- **Key column validation**: prevents distributional aggs (SUM/AVG) on primary keys
- **Metric compilation**: post-aggregation computed columns via metric Project
- **Implicit measures**: metrics auto-include referenced measures in aggregation
- **User attribute injection**: dataset filters with `{user_attribute}` placeholders replaced at compile time

Physical column names are used throughout the tree. A final `Project` node renames to semantic names.

### 5. Emission (planner/sql_emitter.rs, planner/substrait_conv.rs)

Two output formats, gated by `CompileOpts`:

- **SQL** (`emit_sql`): Walks the PlanNode tree and emits ANSI SQL. All identifiers are double-quoted.
- **Substrait** (`emit_plan`): Walks the PlanNode tree and emits `substrait::proto::Plan` protobuf. Serialized to bytes via prost.

---

## Key types

### Public

```rust
pub struct SemanticQuery {
    pub model: ModelRef,
    pub dimensions: Vec<String>,
    pub measures: Vec<String>,
    pub metrics: Vec<String>,
    pub domain: Option<String>,
    pub aggregation: Option<String>,
    pub user_attributes: HashMap<String, String>,  // RLS session params
}

pub struct CompileOpts {
    pub emit_sql: bool,        // default: true
    pub emit_substrait: bool,  // default: false
}

pub struct CompiledPlan {
    pub sql: Option<String>,
    pub substrait: Option<Vec<u8>>,  // Substrait protobuf bytes
    pub columns: Vec<OutputColumn>,
    pub warnings: Vec<Diagnostic>,
}

pub trait SemanticCompiler {
    fn compile(&self, query: &SemanticQuery, opts: &CompileOpts)
        -> Result<CompiledPlan, CompileError>;
}

pub struct StatelessCompiler<R: SchemaRegistry> { registry: R }

pub trait SchemaRegistry {
    fn load(&self, model_ref: &ModelRef) -> Result<ModelFile, CompileError>;
}
```

### Private (pub(crate))

```rust
pub(crate) enum PlanNode {
    Scan(Scan),
    Join(Join),
    CrossJoin(CrossJoin),
    Filter(Filter),
    Aggregate(Aggregate),
    Project(Project),
    Sort(Sort),
    Union(Union),
    VirtualTable(VirtualTable),
}
```

`PlanNode` uses named column references (`Column { relation: String, name: String }`), not positional field indices.

---

## Diagnostics

All errors and warnings flow through:

```rust
pub struct Diagnostic {
    pub level: DiagnosticLevel,   // Error | Warning
    pub code: &'static str,       // "PARSE_E001", "CONST_E001", etc.
    pub message: String,
    pub entity_type: Option<String>,
    pub entity_name: Option<String>,
}

pub struct CompileError(Vec<Diagnostic>);
```

Error code prefixes:
- `PARSE_*` -- schema parsing failures
- `CONST_*` -- constraint violations
- `PLAN_*` -- planner errors
- `EMIT_*` -- SQL/Substrait emission failures
- `METRC_*` -- metric chain depth violations
- `ATTR_*` -- user attribute errors (missing session params)

---

## Dependencies

```toml
serde + serde_yaml + serde_json  # YAML/JSON (de)serialization
substrait = "0.62"                # Substrait protobuf types
prost = "0.14"                    # protobuf encoding (matches substrait)
petgraph = "0.6"                  # joinset relationship graph (BFS, cycle detection)
logos = "0.14"                    # DSL lexer (token generation)
thiserror = "2"                   # error type derivation
tracing = "0.1"                   # structured logging (not yet active)
```

---

## Test coverage

203 tests (190 unit + 13 integration):

- Schema parsing, type system, and duplicate name validation
- DSL lexer, parser, and lowering
- Kind resolution (grainset, unionset, joinset)
- Constraint checking, domain filtering, temporal filters
- Plan building and SQL emission (with identifier quoting)
- Measure-level filters (CASE WHEN wrapping)
- Bucketed dimension compilation (CASE WHEN in Project)
- Key column validation (prevent SUM/AVG on primary keys)
- Semi-additive two-stage aggregation
- Dataset-based fallback compilation
- Metric compilation (post-aggregation computed columns)
- Metric constraint inheritance (stricter-wins from dependent measures)
- Joinset composite keys and join direction flipping
- End-to-end compile roundtrips via `tests/compile_roundtrip.rs`
- Substrait output wiring (PlanNode -> Substrait proto -> bytes)
- User attribute injection (RLS filters via `{user_attribute}` placeholder)
- Kind nesting matrix enforcement (grainset->grainset ERROR, joinset->joinset ERROR, max depth 2)

Test fixtures in `test_data/`: minimal, grainset_basic, grainset_measure_filter, grainset_bucketed, grainset_semi_additive, grainset_metrics, unionset_basic, joinset_basic, full_model, invalid/.

---

## What's not yet implemented

- **SQL dialect-specific emission** -- currently ANSI only; `Dialect` enum is defined but all dialects produce identical output
- **Geo dimension compilation** -- schema supports lat/lon but no spatial query compilation
- **Partition-aware planning** -- partition defs are parsed but not used for optimization
- **LogicalPlan enrichment** -- CONTEXT.md describes node_id (UUID), output_schema, semantic annotations; not yet needed
- **Optimizer passes** -- predicate pushdown, projection pruning, null-branch elimination, constant folding
- **Non-core crates** -- CLI, HTTP, connectors are stubs
- **ConsumerProfile** -- engine capability detection for fallback strategies
