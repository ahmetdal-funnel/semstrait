---
doc: design/questions/closed/33_questions
status: Closed
purpose: Resolved questions originally raised against `apis/33_semstrait_manifest.md`
---

# Closed Questions — `apis/33_semstrait_manifest.md`

> Historical record of ratified manifest decisions. Live items are in [`../open/33_questions.md`](../open/33_questions.md); deferred items are in [`../deferred/33_questions.md`](../deferred/33_questions.md).

---

## Q9 — `ResolvedRelationshipGraph` public-field vs accessor — CLOSED (2026-05-28)

**Status: CLOSED — dissolved.** Settled by manifest ratification clauses C17(d) + C9.6 (see `_research/manifest/RATIFICATION_LOG.md`, 2026-05-28). The accessor-vs-public-field axis no longer applies because `ResolvedRelationshipGraph` itself does not exist on the SemanticManifest.

**Resolution.** Per C17(d), the manifest carries primitive collections only — `Relationship` records live in a relationship map keyed by stable id (originally `BTreeMap<RelationshipId, Relationship>`; re-keyed to `BTreeMap<EntityId, Relationship>` under STATUS item U.2's single-id-lane unification — see `33 §4.1`), with adjacency derived on demand by consumers (planner runtime DAG construction in `34 §1.4A`, daggy-backed). The graph object itself is a runtime construction, not a manifest field. Per C9.6, no `compositions:` top-level field exists on the manifest either; relationship-graph-style traversal stays in the planner runtime per the `34` lifecycle. The Round-1 "accessor-only" position is upheld by absence: there is no graph object on the manifest to expose at all.

**Refs.**

- `_research/manifest/RATIFICATION_LOG.md` — C17(d) (manifest carries primitives only); C9.6 (no `compositions:` top-level field).
- `33 §3.4` — `SemanticManifest` field roster (post-C17 cascade).
- `33 §8.2` — relationship lookup surface.
- `34 §1.4A` — planner runtime graph lifecycle (daggy).

**Question (closed scope).** Should `ResolvedRelationshipGraph` be a public field on `SemanticManifest` (promoted from `33 §8.2`'s accessor-only posture) or remain behind the accessor?

**Resolution rationale.** Both Round-1 options (accessor-only, public field) presupposed that `ResolvedRelationshipGraph` was a manifest-resident artifact. C17(d) ratifies the inverse posture: manifest is a typed-pool of primitives; graph topology is derived in the planner. The choice question dissolves — there is no field, no accessor; the planner constructs `RelationshipGraph` from `manifest.relationships` on each request. Round-1 accessor-only framing retained for historical reference.
