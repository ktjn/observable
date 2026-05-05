# Doc/Spec Review Process Design

**Date:** 2026-04-16
**Topic:** Automated AI agent review process for documentation and spec changes

---

## Problem

The project mandates ADR/spec synchronization and documentation quality in `spec/10-process.md`, but there is no defined process for *how* an AI agent reviews its own doc/spec changes before claiming them complete. Without a structured self-review, agents can open PRs with broken cross-references, missing ADR updates, or contradictions against the rest of the corpus.

---

## Solution

A multi-phase review process with two artifacts:

1. **`§16.9` in `spec/10-process.md`** — normative definition of the process, phases, pass/fail criteria, and mandatory trigger rule
2. **`doc-review` skill** — implements the process; agents invoke it before opening any PR that touches `spec/` or `docs/`

---

## Review Phases

All four phases must pass before the agent can open a PR or claim a doc/spec change complete.

### Phase 1: Structural Validation

Checks every changed file for required structure:

- Valid Markdown (no unclosed fences, broken headings)
- No bare `TODO` or `TBD` placeholders left in the document
- ADR files must contain: Status, Context, Decision, Consequences sections
- Spec files must have a numbered heading consistent with their filename
- Diagrams (Mermaid or similar) must have valid syntax

### Phase 2: Cross-Reference Consistency

For every ADR or spec referenced in a changed file:

- The linked file exists at the stated path
- The reference is bidirectional (if spec A links to ADR-007, ADR-007 must reference or be consistent with spec A)
- The description of the linked decision matches what the linked file actually says

Detects: dangling links, wrong ADR numbers, descriptions that have drifted from the actual decision.

### Phase 3: Coverage Completeness

- If a spec change touches architecture, technology choices, deployment model, data model, security model, or roadmap scope → an ADR must also be touched, or the agent must explicitly state in the PR why no ADR change is needed
- If an ADR is touched → all specs that reference that ADR must be checked for staleness
- `spec/README.md` table must accurately reflect any added, renamed, or removed spec files

### Phase 4: Quality Gates

- No contradictions between changed files and the rest of the corpus
- No sections removed without a replacement or an explicit note explaining the removal
- Changed files maintain accurate cross-links to related specs

---

## Report Format

The skill produces this report after each run:

```
## Doc/Spec Review Report

### Phase 1: Structural Validation — PASS | WARN | FAIL
- [finding] → [file:line]

### Phase 2: Cross-Reference Consistency — PASS | WARN | FAIL
- [finding] → [file:line] ↔ [linked file:line]

### Phase 3: Coverage Completeness — PASS | WARN | FAIL
- [finding] → [spec file] requires ADR update OR [ADR file] requires spec sync

### Phase 4: Quality Gates — PASS | WARN | FAIL
- [finding] → [contradiction or gap location]

### Summary
Overall: PASS | FAIL
Warnings requiring PR acknowledgement: N
Blockers requiring fix before PR: N
```

---

## Failure Handling

- **FAIL in any phase:** agent fixes the issue and re-runs from Phase 1. Cannot open a PR until all phases pass.
- **WARN:** advisory finding. Agent must list all warnings in the PR body under "Acknowledged doc/spec review warnings."
- **PASS (all phases):** agent notes "Doc/spec review: all phases passed" in the PR body.

---

## Trigger Rule

The `doc-review` skill is mandatory whenever the agent modifies any file under `spec/` or `docs/`. It is not optional and must complete before the agent proceeds to `superpowers:verification-before-completion` or opens a PR.

---

## Spec Section

This process is codified as `§16.9 Documentation and Spec Review` in `spec/10-process.md`.

---

## Skill

The skill is saved as `doc-review` in the user's skills directory and follows the rigid skill pattern — agents must follow it exactly without adaptation.
