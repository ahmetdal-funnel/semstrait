# semstrait-sql

SQL dialect emission from `LogicalPlan` IR.

---

## Responsibility

Walks the `PlanNode` tree and emits SQL via `sqlparser-rs` AST construction. One `SqlEmitter` implementation per dialect family. `DslExprSqlRenderer` converts DSL expressions to SQL fragments.

Does not parse SQL, validate semantic correctness, or know about the semantic model.

---

## Architecture

```
LogicalPlan (from semstrait-ir)
      ↓
SqlEmitter::emit(&plan) → Result<String, EmitError>
      ↓
dialect-specific SQL string
```

The emitter walks the PlanNode tree depth-first, building `sqlparser::ast` nodes, then renders to a string. All identifiers are double-quoted for safety.

---

## Key Types

```rust
pub trait SqlDialect: Send + Sync {
    fn quote_ident(&self, ident: &str) -> String;
    fn date_trunc(&self, part: &str, expr: &str) -> String;
}

pub trait SqlEmitter: Send + Sync {
    fn emit(&self, plan: &LogicalPlan) -> Result<String, EmitError>;
}

pub struct AnsiSqlEmitter<D: SqlDialect> { dialect: D }
```

## Dialect Implementations

| Dialect | Status |
|---------|--------|
| `AnsiDialect` | Complete — default, used by all tests |
| `DuckDbDialect` | Implemented — DuckDB-specific identifier quoting |
| `TrinoDialect` | Implemented — Trino-specific syntax |

---

## Dependencies

- `semstrait-core` — `DslExpr`, `DataType`
- `semstrait-ir` — `LogicalPlan`, `PlanNode`
- `sqlparser` v0.53 — AST construction and rendering
