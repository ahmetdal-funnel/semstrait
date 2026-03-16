# semstrait-cli — Implementation Plan

Phase 6 of workspace plan. Can begin as soon as Phase 1 (semstrait-core) is complete — `query`, `validate`, `schema` commands only require the compiler. `execute` requires Phase 4. `serve` requires Phase 5.

---

## Phase 6.1 — clap command tree skeleton

**Task:** Define the full command tree with `clap::Parser` derive macros. All subcommand handlers return `Ok(())` initially. Wire up global flags and verify `--help` output matches intended UX.

```rust
#[derive(Parser)]
#[command(name = "semstrait", about = "Semantic layer compiler")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Query(QueryArgs),
    Execute(ExecuteArgs),
    Explain(ExplainArgs),
    Validate(ValidateArgs),
    Lineage(LineageArgs),
    Schema(SchemaArgs),
    Serve(ServeArgs),
}
```

`QueryArgs` carries `--model`, `--query`, `--format`, `--dialect`. All `Args` structs share a common `CommonArgs` via `#[command(flatten)]`.

**Deliverable:** `semstrait --help` and `semstrait query --help` show correct usage. All commands exit 0 with no output.

---

## Phase 6.2 — Compiler construction

**Task:** Build the `StatelessCompiler` from CLI args. This is the wiring point.

```rust
fn build_compiler(args: &CommonArgs) -> impl SemanticCompiler {
    let registry = FileSystemRegistry::new(&args.model_base_path());
    StatelessCompiler::new(registry)
}
```

`--model` can be an absolute path, a relative path, or just a model name. The resolution logic:
- If it ends in `.yaml`, use `ModelRef::FilePath`
- Otherwise, use `ModelRef::Key` with the filename stem as the name

This logic lives in a `resolve_model_ref(s: &str) -> ModelRef` helper.

---

## Phase 6.3 — `query` command

**Task:** Implement `semstrait query`. First real command.

```rust
fn run_query(args: &QueryArgs) -> Result<()> {
    let compiler = build_compiler(&args.common);
    let request = load_query_request(&args.query_path)?;
    let opts = build_compile_opts(args)?;

    let plan = compiler.compile(&resolve_model_ref(&args.model), &request, &opts)
        .map_err(|e| format_compile_error(e))?;

    match args.format {
        Format::Substrait    => print_base64(plan.substrait()),
        Format::SubstraitBin => stdout().write_all(plan.substrait())?,
        Format::Sql          => println!("{}", plan.sql().unwrap_or("(no SQL requested)")),
        Format::Json         => println!("{}", serde_json::to_string_pretty(&plan_to_json(&plan))?),
        Format::Explain      => println!("{}", format_explain(plan.substrait())?),
    }

    if args.verbose {
        eprintln_diagnostics(plan.diagnostics());
    }

    Ok(())
}
```

`format_explain(bytes)` calls `substrait_explain::parse` + `substrait_explain::format` to produce a human-readable plan string from the Substrait bytes. This keeps the explain output honest — it shows what was actually serialised, not what the planner thought it produced.

**Test:** `semstrait query -m test_data/steelwheels.yaml -q test_data/queries/revenue_by_year.json --format sql` produces deterministic SQL. Snapshot test this.

---

## Phase 6.4 — `validate` and `schema` commands

**Task:** Straightforward mappings to `compiler.validate()` and `compiler.schema_info()`.

`validate` prints a coloured diagnostic list. Exit code: 0 if valid, 1 if any Error-level diagnostics. This makes `semstrait validate -m model.yaml` reliable in CI pipelines.

`schema` prints a summary table:
```
Model: sales
  Datasets: 2 (orders_daily, orders_raw)
  Measures: revenue, order_count, avg_order_value
  Dimensions: date.year, date.month, region, product.category
```

---

## Phase 6.5 — `explain`, `lineage`, `execute`

**Task:** Implement the remaining subcommands.

`explain` — same as `query --format explain`. Kept as a separate subcommand for discoverability and because it will eventually show additional context (join strategy, dataset selected, grain routing decisions) not available from the Substrait bytes alone.

`lineage` — calls `compile()` with `include_lineage: true`. Prints `plan.lineage().to_openlineage_event(...)` as pretty JSON. Requires Phase 2.

`execute` — constructs `PassthroughAdapter` from `--endpoint` arg. Calls execute. Prints rows as a formatted table using `comfy-table` or similar. Requires Phase 4.

---

## Phase 6.6 — `serve` command

**Task:** Delegate to `semstrait_http::start_server(config, compiler, adapter)`.

```rust
fn run_serve(args: &ServeArgs) -> Result<()> {
    let compiler = Arc::new(build_compiler(&args.common));
    let adapter = args.endpoint.as_ref()
        .map(|url| Arc::new(PassthroughAdapter::new(url, args.dialect)) as Arc<dyn ConnectorAdapter>);

    let config = ServerConfig {
        bind_addr: args.bind_addr(),
        model_path: args.model_base_path(),
        default_dialect: args.dialect,
        request_timeout: Duration::from_secs(args.timeout_secs.unwrap_or(30)),
    };

    tokio::runtime::Runtime::new()?.block_on(
        semstrait_http::start_server(config, compiler, adapter)
    )
}
```

Requires Phase 5.

---

## Error presentation

CLI errors are formatted for humans, not machines:

```
error[RESOL_E002]: unknown measure 'revnue'
  --> sales.yaml
  |
  | context: did you mean 'revenue'?

For model validation details, run: semstrait validate -m sales.yaml
```

Exit codes: 0 success, 1 compile/validation error, 2 usage error, 3 I/O error.
