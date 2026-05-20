---
doc: design/questions/open/21_questions
status: Living
purpose: Parked unresolved questions surfaced while drafting `data-kinds/21_dataset.md`
depends-on:
  - data-kinds/21_dataset.md
  - data-kinds/20_taxonomy.md
  - foundations/11_names_and_scopes.md
  - foundations/12_nesting_policy.md
  - foundations/13_types_and_grain.md
  - foundations/14_expressions.md
  - foundations/19_expression_flow.md
  - foundations/15_mapping_and_binding.md
  - foundations/17_temporal_shape.md
  - apis/30_api_contracts.md
  - apis/34_semstrait_planner.md
---

# Open Questions — `data-kinds/21_dataset.md`

> Items surfaced during Round-1 drafting of the `SimpleDataKind` (Dataset) spec. Each entry restates the question, summarizes options, records `21`'s Round-1 working default, and flags the subsequent doc that must resolve before `21` is final. Post-thirteenth-pass re-evaluation (2026-04-30) confirmed Round-1 defaults remain consistent with the sealed trait hierarchy / `LeafExtras` architecture; Q-DS-001 has been ratified at Option A and migrated to [`../closed/21_questions.md`](../closed/21_questions.md). Q-DS-002..005 remain deferred to their named subsequent docs.

---

## Q-DS-002 — Wrapper code discipline for re-surfaced errors

**Question.** Compile-time errors raised inside a `SimpleDataKind`'s resolution flow often originate in foundations docs (`14`, `14a`, `19`, `15`) with their own code ranges. Should `21` wrap every such error under a `COMP_E_21xx` code (adding Simple-level diagnostic context), pass them through unchanged (preserving origin), or wrap selectively?

**Refs.**
- `21 §8` — `COMP_E_2101 SimpleBindingResolutionFailed` (catch-all wrapper), `COMP_E_2102` / `2106` / `2107` (selective wrappers).
- `15 §11.3` — re-surfaces expression errors from `14` / `14a` / `19` without re-codification.
- `19 §8.1` — `EXPR_E_02xx` range ownership.
- `30 §6.2` — code-range governance.

**Options.**
- **(A) Selective wrapping (Round-1 default).** Only wrap when the Simple-level context materially aids the operator (e.g. "compile of Simple `orders` failed because its Binding's glob matched zero files"). Otherwise pass through with the origin code and fill `Diagnostic.location` with the Simple's position.
- **(B) Universal wrapping.** Every error surfacing inside a Simple's resolution gets a `COMP_E_21xx` code, carrying the inner cause in a `cause: Box<CompileError>` field. Maximizes Simple-level grepability but doubles the code count and tends to hide origin ranges.
- **(C) No wrapping ever.** Pass through every error verbatim, including location. Minimal code churn but verbose-unhelpful when a single `COMP_E_0308 MissingBindingEntry` fires with no indication of which Simple it came from — `Diagnostic.location` alone may not be enough operator context for large Models.

**Arguments for (A).** Matches `15 §11.3`'s "re-surface without re-codify" posture; keeps the code catalog compact; uses `Diagnostic.location` for operator context.

**Arguments for (B).** Gives operators a Simple-first grep surface ("show me every Simple-compile error") without traversing the union of all sub-ranges. Valuable when ingestion is the common failure mode.

**Arguments for (C).** Purity of separation-of-concerns; but weak on operator ergonomics.

**Current position in `21`.** Option A. `21 §8` wraps in four cases (`2101`, `2102`, `2106`, `2107`) where context materially aids; other compile errors pass through with origin codes.

**Blocking.** Not blocking `21`. Decide during `30 §6.2` (code-range governance) and `34` (observability patterns) drafting — if log aggregation / dashboard filters prefer one-kind-one-code-range, promote to Option B. If pure pass-through is preferred, shrink §8 to just `2101`.

---

## Q-DS-003 — Multi-source per-branch metadata emission at L2

**Question.** When a Simple's L1 fans out to N `PhysicalSource`s (glob-expanded), L2 (Rename) emits per-branch metadata-Dimension literals inside each Union branch. Should `SimpleStrategy` emit these literals **unconditionally** (every metadata Dimension in the `ResolvedColumnMapping.metadata` is materialized in every L2), or should it **prune** metadata Dimensions that no downstream layer (L3/L4/L5/filters) reads?

**Refs.**
- `_drafts/34_simple_strategy.md §3` — L2 Rename emission (formerly `21 §4.3`; relocated per the post-thirteenth-pass cascade rebase, 2026-04-30).
- `21 §10.4` — worked example reading-key notes the question.
- `34 §5` — optimizer pushdown / elision (TBD).

**Options.**
- **(A) Unconditional at `SimpleStrategy`; optimizer elides (Round-1 default).** L2 always emits every `metadata` entry; `34`'s optimizer pass prunes unused ones. Keeps `SimpleStrategy` simple and layered.
- **(B) Prune at `SimpleStrategy`.** L2 only emits metadata literals that downstream layers reference. Smaller plans out of the box; requires `SimpleStrategy` to carry a Request-referenced-Semantics set at L2.
- **(C) Mix.** Unconditional for metadata Dimensions used in Filter / L3 / L4 / L5; pruned otherwise. Forces a bespoke rule rather than leveraging a general elision pass.

**Arguments for (A).** Layer-locality: L2's logic is uniform; optimizer handles elision as a general Project-pruning rule.

**Arguments for (B).** Smaller pre-optimizer plan; fewer per-branch literals to carry around; matches the legacy implementation pattern (per `docs/DATASET.md §2.2` — only requested metadata is emitted).

**Arguments for (C).** Finest-grained control; most complex to specify.

**Current position in `21`.** Option A. L2 unconditional; §10.6 reading-key notes the future elision in `34`.

**Blocking.** Not blocking `21`. Decide when `34 §5` ratifies the optimizer's Project-pruning rule — if pruning is readily available as a general pass, stick with (A); if not, downgrade to (B).

---

## Q-DS-004 — Temporal-shape identifier on computed Dimensions

**Question.** A `temporal_shape:` declaration's identifying field (`event_time`, `snapshotted_at`, `valid_from`, `valid_to`) names a Semantics. Can that Semantics be a **Computed Dimension** (with a `ColumnMappingValue::Computed` expression), or is it restricted to Dimensions that map to a `ColumnMappingValue::Column` (physical column)?

**Refs.**
- `21 §5` — `TemporalShape` interaction (cross-ref only).
- `21 §7` — `VALID_E_2108` / `VALID_E_2109` — shape-identifier checks.
- `17` — full `TemporalShape` semantics (being drafted concurrently).

**Options.**
- **(A) Any Dimension with a matching `DataType` (Round-1 default).** Whether the Dimension maps to a physical column, a literal, a computed expression, or a metadata extraction does not matter. The shape check reads the Dimension's declared `data_type:` from the interface and confirms it is temporal.
- **(B) Restrict to physical-column Dimensions only.** A shape identifier must have a non-trivially-transformed physical column; Computed Dimensions risk performance cliffs (the as-of join's inequality predicate loses index-friendliness when the identifier is a per-row expression evaluation). Restricts the error surface of a future `17` planner.
- **(C) Allow Computed but with a warning.** Accept Computed identifiers at compile but emit a `COMP_W_21xx TemporalIdentifierIsComputed` advisory.

**Arguments for (A).** Most permissive; lets authors model "my Events source has a timestamp as text; I cast it in a Computed Dimension and use it as `event_time`". No semantic issue.

**Arguments for (B).** Performance posture: as-of joins (`17`'s deferred feature) and snapshot-selection both benefit from index-friendly identifiers. A Computed Dimension is opaque at plan time. But this is a planner-performance concern, not a correctness concern — forcing authors to precompute upstream is an overreach.

**Arguments for (C).** Middle ground. Accepts but advises.

**Current position in `21`.** Option A. Any Dimension with a temporal `data_type:` qualifies.

**Blocking.** Not blocking `21`. Decide at `17` drafting — if `17`'s as-of planner performance-models Computed identifiers as a hard case, promote to Option C. Does not affect `SimpleStrategy`'s emission; only `17`'s matrix checks.

---

## Q-DS-005 — Re-aggregation-skip predicate over Computed Dimensions

**Question.** `_drafts/34_simple_strategy.md §5.1`'s re-aggregation-skip predicate (formerly `21 §4.5.1`; relocated per the post-thirteenth-pass cascade rebase, 2026-04-30) examines whether any Dimension in `GROUP BY` is **source-distinguishing** (has distinct values across every source in a multi-source Binding). In v1, the predicate only considers **metadata Dimensions** (whose per-source values are compile-time literals). Should it extend to **Computed Dimensions** whose expressions are demonstrably source-distinguishing (e.g. `Case WHEN year = '2024' THEN 'A' ELSE 'B' END`)?

**Refs.**
- `_drafts/34_simple_strategy.md §5.1` — re-aggregation-skip predicate (formerly `21 §4.5.1`; relocated per the post-thirteenth-pass cascade rebase, 2026-04-30).
- `19 §3.4` — `PhysicalExpr` substitution (per-source literals are substituted per `15 §8`).
- `15 §8` — metadata extraction.

**Options.**
- **(A) v1 only checks metadata Dimensions (Round-1 default).** The predicate is simple and provably correct: `Metadata` values are per-source literals, `literals[source_i] != literals[source_j]` iff the Dimension distinguishes. Computed Dimensions are not checked.
- **(B) Extend to Computed Dimensions via expression inspection.** When a Computed Dimension's `PhysicalExpr` is a pure function of already-distinguishing values (another metadata Dimension; a literal), propagate distinguishing-ness through the expression. Requires an expression-walker that tracks which leaves are per-source literals.
- **(C) Extend to Computed Dimensions via per-source evaluation.** Evaluate the Computed Dimension's expression once per source at plan time (assuming the expression is `literals`-only) and compare outputs. More expensive but more general.

**Arguments for (A).** Simple, provably correct, covers the common case.

**Arguments for (B).** Handles the "I wrapped a metadata Dimension in a `Case`" idiom that authors naturally write. Requires an expression analyzer but `19 §3.10`'s column-reference harvesting is nearby infrastructure.

**Arguments for (C).** Covers every expression shape, including `Coalesce` over metadata literals and so on. Evaluating expressions at plan time is a minor complexity addition.

**Current position in `21`.** Option A. v1 predicate only inspects metadata Dimensions.

**Blocking.** Not blocking `21`. Decide at `34 §<implicit-union>` drafting (the disjointness-elision algorithm body) or when authors explicitly request the Computed extension. Round-1 behavior: `LossyMultiSourceReaggregation` (`PLAN_W_2101`) fires more often than strictly needed, but correctness is preserved.

---

## Index

| # | Title | Owned-by | Blocking? | Next step |
|---|---|---|---|---|
| Q-DS-002 | Wrapper code discipline for re-surfaced errors | `21 §8` | No | Confirm at `30 §6.2` / `34` drafting |
| Q-DS-003 | Multi-source metadata emission at L2 | `_drafts/34_simple_strategy.md §3` | No | Confirm at `34 §5` |
| Q-DS-004 | Temporal-shape identifier on Computed Dimension | `21 §5`, `§7` | No | Confirm at `17` |
| Q-DS-005 | Re-aggregation skip over Computed Dimensions | `_drafts/34_simple_strategy.md §5.1` | No | Confirm at `34 §<implicit-union>` |

None of the remaining `21` open items block `21`'s ratification; every default is internally consistent and every follow-up is scoped to a later doc's drafting. Q-DS-001 was migrated to [`../closed/21_questions.md`](../closed/21_questions.md) on 2026-04-30 (post-thirteenth-pass cascade).
