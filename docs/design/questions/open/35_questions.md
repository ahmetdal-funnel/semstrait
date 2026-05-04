---
doc: design/questions/open/35_questions
status: Living (focused v1 backlog)
purpose: Active architecture-impacting questions for `apis/35_semstrait_ir.md`
depends-on:
  - apis/35_semstrait_ir.md
  - apis/34_semstrait_planner.md
  - apis/36_semstrait_adapter.md
---

# Open Questions — `apis/35_semstrait_ir.md`

Closed:
- [`../closed/35_questions.md`](../closed/35_questions.md)

Deferred:
- [`../deferred/35_questions.md`](../deferred/35_questions.md)

---

## Q-IR-002 — `NodeId` stability across runs

Should `NodeId` remain per-run opaque identity, or move to deterministic/content-derived identity for cross-run diffability?

Current default: per-run opaque identity.

---

## Q-IR-006 — `Schema` ownership boundary (`ir` vs `core`)

Should plan-layer schema remain IR-owned or be hoisted to shared core vocabulary?

Current default: IR-owned schema shape.

---

## Q-IR-007 — diagnostics on `SemanticPlan` vs separate result envelope

Should planner diagnostics remain embedded on plan object or move to stage-result wrapper only?

Current default: diagnostics field on `SemanticPlan` with equality/hash exclusions.

---

## Q-IR-010 — `Capability` roster placement split (`35` vs `36`)

Should capability enum placement and roster authority remain split or converge?

Current default: enum in `35`, roster authority in `36`.

---

## Q-IR-014 — `SemAnnotation` wire stability posture

Is current annotation wire strategy stable enough for v1 baseline while `34` annotation producers continue maturing?

Current default: keep `SemAnnotation` on nodes with non-exhaustive forward growth.

