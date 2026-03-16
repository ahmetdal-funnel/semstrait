# semstrait-cli

Developer CLI for semstrait. Compile, execute, validate, and introspect semantic models from the terminal.

---

## Commands

```
semstrait <COMMAND>

Commands:
  query     Compile a query and print the output
  execute   Compile and execute a query against an engine
  explain   Print a human-readable plan tree for a query
  validate  Validate a model and/or query
  lineage   Print OpenLineage JSON for a query
  schema    Print model metadata (measures, dimensions, datasets)
  serve     Start the HTTP API server
```

---

## Usage examples

**Compile to Substrait (pipe to engine):**
```bash
semstrait query -m sales.yaml -q query.json --format substrait | \
  duckdb :memory: "SELECT * FROM substrait_scan(read_blob('/dev/stdin'))"
```

**Compile to DuckDB SQL:**
```bash
semstrait query -m sales.yaml -q query.json --format sql --dialect duckdb
```

**Validate before deploying a model change:**
```bash
semstrait validate -m sales.yaml && echo "Model OK"
```

**Inspect the logical plan:**
```bash
semstrait explain -m sales.yaml -q query.json
# Prints:
# Sort(date.year DESC)
#   Aggregate(revenue=sum(amount), group_by=[date.year, region])
#     Join(orders ⋈ dim_region ON region_id)
#       Scan(warehouse.fact_orders)
#       Scan(warehouse.dim_region)
```

**Get lineage as OpenLineage JSON:**
```bash
semstrait lineage -m sales.yaml -q query.json | jq .inputs
```

**Start the HTTP server:**
```bash
semstrait serve -m ./models -p 3000
```

---

## Output formats (`--format`)

| Value | Description |
|---|---|
| `substrait` | Base64-encoded Substrait bytes (default for `query`) |
| `substrait-bin` | Raw binary Substrait bytes (pipe-friendly) |
| `sql` | Dialect SQL string |
| `json` | JSON object with both `substrait` (base64) and `sql` |
| `explain` | Human-readable plan tree |

---

## Global flags

```
-m, --model <PATH>      Path to the semantic model YAML file
-q, --query <PATH>      Path to the query JSON file (or inline JSON via -)
    --dialect <DIALECT> SQL dialect: ansi, duckdb, spark, snowflake, bigquery, trino [default: ansi]
    --format <FORMAT>   Output format [default: substrait]
    --no-color          Disable terminal colors
-v, --verbose           Show diagnostics and warnings
```

---

## Query JSON format

The query file passed to `-q` is a JSON representation of `QueryRequest`:

```json
{
  "model": "sales",
  "measures": ["revenue", "order_count"],
  "dimensions": ["date.year", "region"],
  "filters": [
    {"column": "date.year", "op": "eq", "value": "2024"}
  ],
  "limit": 100
}
```

Pass `-q -` to read from stdin.

---

## Design notes

The CLI depends only on `SemanticCompiler` trait plus `semstrait-http` for the `serve` command. It holds no semantic model knowledge. `semstrait explain` produces its output by formatting the `CompiledPlan`'s Substrait bytes using `substrait-explain` — not by inspecting internal plan nodes.
