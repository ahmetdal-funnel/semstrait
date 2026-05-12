# Design Docs Maintenance Rules

Status: **Living**

This file defines editing discipline for humans and AI agents maintaining `docs/design/`.

Use with:

- `[00_overview.md](00_overview.md)`
- `[INDEX.md](INDEX.md)`
- `[STATUS.md](STATUS.md)`

---

## 1) Single source of truth

Every concept has exactly one authoritative home.

If ownership changes, update in the same commit:

1. owning doc,
2. `[INDEX.md](INDEX.md)`,
3. `[STATUS.md](STATUS.md)` (if state/phase/questions changed).

---

## 2) Keep root docs thin

- `00_overview.md`: contract, invariants, layering, concise vocabulary map.
- `INDEX.md`: navigation and ownership map.
- `STATUS.md`: active handoff state only.

Long narrative replay belongs in archive files or git history.

---

## 3) No duplicate normative prose

Repeat pointers, not paragraphs.

When a rule is already ratified elsewhere:

- summarize in one line,
- link to owner,
- avoid restating full rationale.

---

## 4) Question-state directory is authoritative

State is defined by directory:

- `questions/open/` -> active v1 backlog
- `questions/closed/` -> resolved history
- `questions/deferred/` -> parked/post-v1

When status changes, move/land content accordingly.

---

## 5) Open question template (compact)

Keep active questions decision-oriented:

1. Question (1-2 lines)
2. Why it matters for v1
3. Current default
4. Decision trigger / owner

Avoid essay-style option trees in active `open/` unless strictly necessary.

---

## 6) v1 scope discipline

`open/` should contain only implementation-impacting v1 decisions.

Move to `deferred/` when items are:

- ergonomics/polish only,
- empirical mapping depth not blocking v1,
- speculative post-v1 extension design.

---

## 7) Typed diagnostics language only

Prefer typed-kind vocabulary (`*ErrorKind`, `Diagnostic<K>`, `Diagnose`, `tracing`).

Treat stable numeric-code-table discussion as historical context unless a doc is explicitly archival.

---

## 8) Registry docs are living catalogs

Registry docs track engine/provider mapping reality and may churn faster.

Do not let registry churn rewrite canonical foundational semantics.

---

## 9) Link hygiene

Before closing a session:

- validate newly added root links (`00`, `INDEX`, `STATUS`, this file),
- ensure moved question IDs still have a discoverable pointer path.

---

## 10) Session-close checklist

At session end, propose updates to `STATUS.md`:

- what moved between open/closed/deferred,
- what was deferred newly,
- where next session should start.