---
doc: design/questions/closed/36_questions
status: Closed
purpose: Resolved questions originally raised against `apis/36_semstrait_adapter.md`
---

# Closed Questions — `apis/36_semstrait_adapter.md`

> Historical record of ratified adapter decisions. Live items are in [`../open/36_questions.md`](../open/36_questions.md).

---

## Q-ADAPT-001 — `EngineAdapter::adapt` return shape  *[Closed — superseded by tuple-return ratification]*

**Status.** Closed. The eleventh-pass typed-kind cascade (2026-04-29) ratified the workspace-wide fail-fast tuple at `30 §7` and propagated it into `36 §3.1` / §17. `EngineAdapter::adapt` now signs as

```rust
fn adapt(
    &self,
    plan: &SemanticPlan,
    manifest: &SemanticManifest,
) -> Result<
    (EngineArtifact, Diagnostics<AdaptErrorKind>),
    (Diagnostic<AdaptErrorKind>, Diagnostics<AdaptErrorKind>),
>;
```

The dual-method `adapt_with_diagnostics` extension that this question prefigured is no longer needed: warnings ride alongside both the success arm and the failure arm of the workspace-wide tuple, keyed to the per-stage typed-kind enum `AdaptErrorKind` (`36 §10`).

**Original framing (preserved).** `36 §3.1` formerly ratified `adapt(&SemanticPlan, &SemanticManifest) -> Result<EngineArtifact, AdaptError>`. `30 §7`'s stage-result pattern recommends carrying `Vec<Diagnostic>` alongside successful output. The question was whether `adapt` should also carry `Vec<Diagnostic>` on success (e.g. "structural rewrite for `string_agg` applied on Spark") or stay bare with diagnostic appendage on `SemanticPlan.diagnostics` (`35 §3.1`).

- **Round-1 default**: bare `Result<EngineArtifact, AdaptError>`, with revisit gated on `34`'s planner-wiring draft.
- **Argument pro tuple-shape**: matches `30 §7`'s documented stage pattern; uniform across stages; decouples adapter-emitted diagnostics from the plan's diagnostic list (so a plan consumed twice by two adapters produces two independent lists).
- **Argument pro bare shape**: simpler call-site ergonomics; current-code parity; `SemanticPlan.diagnostics` accommodates the warnings already; `ADAPT_W_*` codes were reserved-but-unused in v1.

**Resolution.** `36` ratifies the workspace-wide tuple shape uniformly. Adapter-emitted warnings flow through `Diagnostics<AdaptErrorKind>` (the second tuple element on success; appended in either arm). `SemanticPlan.diagnostics` continues to carry planner-stage diagnostics; adapters do not mutate it. The retired `AdaptError` carrier and the old `ADAPT_W_*` numeric codes are gone (typed-kind discipline replaces them). Future adapter warnings are added as `AdaptErrorKind` variants with `Severity::Warning`.
