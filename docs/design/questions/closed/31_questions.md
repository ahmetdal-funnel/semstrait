---
doc: design/questions/closed/31_questions
status: Closed
purpose: Resolved questions originally raised against `apis/31_semstrait_core.md`
depends-on:
  - apis/31_semstrait_core.md
---

# Closed Questions — `apis/31_semstrait_core.md`

---

## Q1 — Canonical `Aggregation` variant count

**CLOSED (structure-optimization pass, 2026-05-03).** Already ratified by authoritative expression docs (`14`); no active decision required in `31`.

**Resolution.** Keep five-variant canonical aggregation posture with `distinct` flag behavior as defined by `14`.

---

## Q3 — Legacy numeric error codes (`EXPR_E_####`)

**CLOSED (structure-optimization pass, 2026-05-03).** Superseded by typed-kind diagnostic discipline.

**Resolution.** Numeric-code dual-system discussion is historical context only; stage-owned typed kinds are authoritative for v1 diagnostics.

---

## Q4 — `ExprBlock` exposure boundary

**CLOSED (second-cascade landing, 2026-05-19, `STATUS.md` item Q).** Question is **moot under the Option A landing** ratified for `[14 §6.1](../../foundations/14_expressions.md)`.

**Resolution.** There is no `ExprBlock` type anywhere in the workspace. `ExprSource::Block(Expr<L>)` (`14 §6.1`) carries `Expr<L>` directly via serde, using the serde derives on `Expr<L>` owned by `semstrait-ir` (`35 §14.1`). The reserved-tag catalog (`14 §6.4`) is implemented as serde tag-discrimination on `Expr<L>` plus a `FunctionRegistry` look-aside, wired in `semstrait-model`'s `Deserialize` impl for `ExprSource<L>`. `semstrait-core` owns neither `ExprBlock` nor `ExprSource` — its post-cascade surface excludes all expression-tree vocabulary (`31 §1.1` / `§1.2`).

