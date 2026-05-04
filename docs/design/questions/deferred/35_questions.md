---
doc: design/questions/deferred/35_questions
status: Deferred (post-v1 IR surface refinements)
purpose: Deferred non-blocking questions moved from `open/35_questions.md`
depends-on:
  - apis/35_semstrait_ir.md
---

# Deferred Questions — `apis/35_semstrait_ir.md`

| ID | Topic | Last known default |
|---|---|---|
| Q-IR-003 | non-equi residual join field reservation | defer field addition |
| Q-IR-004 | aggregate filter field reservation | reserve field; usage deferred |
| Q-IR-005 | `Dialect` sealing posture | keep non-sealed |
| Q-IR-008 | visitor enter/exit trait expansion | keep single-method |
| Q-IR-009 | `EnginePlan::Substrait` boxing strategy | keep boxed variant |
| Q-IR-011 | `SourceRef` rendering/accessor posture | accessors, no `Display` |
| Q-IR-012 | dedicated `Distinct` node variant | keep aggregate lowering |
| Q-IR-013 | split `FetchNode` into limit/offset nodes | keep combined node |

Re-open when optimizer/adapter implementation needs any of these shape choices to ship behavior.

