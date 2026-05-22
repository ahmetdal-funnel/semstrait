---
doc: design/questions/closed/35_questions
status: Closed
purpose: Resolved questions originally raised against `apis/35_semstrait_ir.md`
depends-on:
  - apis/35_semstrait_ir.md
---

# Closed Questions — `apis/35_semstrait_ir.md`

---

## Q-IR-001 — `IR_E_35xx` subsystem-prefix registration in `30 §6.2`

**CLOSED (structure-optimization pass, 2026-05-03).** Superseded by typed-kind diagnostic discipline.

**Resolution.** Numeric subsystem-prefix allocation is archived context only; active v1 diagnostics use typed stage kinds and variant identity.

---

## Q-IR-014 — `SemAnnotation` wire stability posture

**CLOSED (IR redesign Phase-1 ratification, 2026-05-21).**

**Resolution.** `SemAnnotation` wire posture ratified at `35 §11.1.1` (variant inventory + TRACE/PLAN classification) and `35 §15.3` (Substrait carrier policy). Carrier is `RelCommon.advanced_extension.optimization[]` under URN `urn:semstrait:annotations:v1`; drop-safe contract via Substrait's `optimization[]` semantics; binary round-trip is full, JSON is best-effort. Forward growth handled by `#[non_exhaustive]` on `SemAnnotation` and `BoundaryPosition`; new variants land as MINOR additions per `30`.

---

## Q-IR-002 — `NodeId` stability across runs

**CLOSED (IR redesign Phase-2 ratification, 2026-05-21).**

**Resolution.** `NodeId` remains per-process opaque (`Uuid::new_v4()`); cross-run diff comparison is undefined. Reference IRs (DataFusion, Calcite, Spark Catalyst, Substrait) all keep node identity intra-process; content-hash identity introduces an "equality implies subtree equality" invariant that breaks under optimizer rewrite (every ancestor flips on any descendant change). Doc-comment at `35 §11.1` tightened to make the per-process / per-`SemanticPlan`-lifetime contract explicit; consumers requiring cross-run diff MUST compare structurally.

---

## Q-IR-006 — `Schema` ownership boundary (`ir` vs `core`)

**CLOSED (IR redesign Phase-2 ratification, 2026-05-21).**

**Resolution.** `Schema` lives in `semstrait-common` (`31 §4.4`) and is re-exported by `35 §3.4`; `ir` owns no parallel `Schema` type. The current `crates/semstrait-ir/src/schema.rs::Schema` is a pre-cascade duplicate scheduled for deletion at refactor time — consumers move to `semstrait_common::Schema` / `SchemaColumn`. `35 §11.1` paragraph updated to reflect the canonical `{ columns: Vec<SchemaColumn> }` shape (replacing the legacy `{ fields: Vec<Field> }` framing). Rationale: `Schema` is shared vocabulary across model→manifest→ir→planner→adapter; per `13 §2`'s placement rule, shared vocabulary lives in core. Two `Schema`s means two equality semantics, two ordinal lookups, two serde wire forms — strict no-good.

---

## Q-IR-007 — diagnostics on `SemanticPlan` vs separate result envelope

**CLOSED (IR redesign Phase-2 ratification, 2026-05-21).**

**Resolution.** Warnings flow through BOTH the planner result tuple (`Diagnostics<PlanErrorKind>` second element) AND `SemanticPlan.diagnostics` per `30 §7.3` and `34 §13.2` — same content, different audiences. The tuple is the caller's operational contract; the plan-side field is the artifact's self-describing contract for downstream adapters and inspectors. Removing the plan-side field would force every consumer to thread the result tuple through every layer that touches the plan, contradicting S7 / R3 (planner is producer; adapter is consumer; the plan IS the contract). Errors are NOT on `SemanticPlan` — they abort planning and never reach a successful artifact. `35 §9.1` doc-comment tightened to make the both-arms rationale explicit.

---

## Q-IR-010 — `Capability` roster placement split (`35` vs `36`)

**CLOSED (IR redesign Phase-2 ratification, 2026-05-21).**

**Resolution.** Type **definition** lives in `35 §12.6` (closed catalog rule R4); `36` drives variant additions through concrete adapter-feature need and owns the per-adapter `AdapterCapabilities` roster (`36 §6.1`). Resolves the prior circular framing where both `35 §12.6` and `36 §6.3` claimed "ratified in `36`" while `35`'s dep graph forbade importing from `36`. Adding a new variant is now a `35`-side MINOR edit driven by `36`-side rationale, the same flow as `PlanNode` variants (defined in `35`, produced by `34`). `Dialect::capabilities() -> &'static [Capability]` (`35 §12.5`) stays in `35`; `35` re-exports nothing from `36`. Cross-doc cleanup landed in `36 §6.1` comment + `§6.3` ratification clause.

