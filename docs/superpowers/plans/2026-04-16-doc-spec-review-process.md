# Doc/Spec Review Process Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a mandatory four-phase AI agent self-review process for all documentation and spec changes, defined in `spec/10-process.md §16.9` and implemented as a `doc-review` skill.

**Architecture:** A new spec section (`§16.9`) defines the normative process — phases, pass/fail criteria, and trigger rule. A companion skill (`doc-review`) provides the agent-executable instructions. The spec section is the source of truth; the skill implements it.

**Tech Stack:** Markdown, YAML frontmatter (skill), git

---

## File Map

| Action | Path | Purpose |
|--------|------|---------|
| Modify | `spec/10-process.md` | Add §16.9 Documentation and Spec Review after §16.8 |
| Create | `C:\Users\ktjn\.claude\skills\doc-review\SKILL.md` | Agent-executable skill implementing the review process |

---

### Task 1: Create branch

**Files:**
- No file changes — branch creation only

- [ ] **Step 1: Create and switch to a short-lived branch**

```bash
git checkout -b claude/doc-spec-review-process
```

Expected: switched to new branch `claude/doc-spec-review-process`

---

### Task 2: Add §16.9 to `spec/10-process.md`

**Files:**
- Modify: `spec/10-process.md` (insert after §16.8 block, before the `---` divider before section 17)

- [ ] **Step 1: Read the current end of §16.8 to find the exact insertion point**

Read `spec/10-process.md` lines 185–200 to confirm the line number of the `---` divider between §16 and §17.

- [ ] **Step 2: Insert §16.9 before the `---` divider before section 17**

The new section to insert (place immediately before the line `---` that precedes `## 17. Project Plan`):

```markdown

### 16.9 Documentation and Spec Review

Any agent PR that touches files under `spec/` or `docs/` must run the `doc-review` skill and pass all four phases before opening a PR or claiming the change complete.

**Trigger rule:** Mandatory whenever the agent modifies any file under `spec/` or `docs/`. Not optional. Must complete before `superpowers:verification-before-completion` or PR creation.

**Phases — all must pass:**

#### Phase 1: Structural Validation
- Valid Markdown: no unclosed fences, broken headings
- No bare `TODO` or `TBD` placeholders remaining in the document
- ADR files must contain: Status, Context, Decision, Consequences sections
- Spec files must have a numbered heading consistent with their filename
- Diagrams (Mermaid or similar) must have valid syntax

#### Phase 2: Cross-Reference Consistency
For every ADR or spec referenced in a changed file:
- The linked file exists at the stated path
- The reference is bidirectional (if spec A links to ADR-007, ADR-007 must be consistent with spec A)
- The description of the linked decision matches what the linked file actually says

#### Phase 3: Coverage Completeness
- If a spec change touches architecture, technology choices, deployment model, data model, security model, or roadmap scope → an ADR must also be touched, or the PR must explicitly state why no ADR change is needed
- If an ADR is touched → all specs that reference that ADR must be checked for staleness
- `spec/README.md` table must accurately reflect any added, renamed, or removed spec files

#### Phase 4: Quality Gates
- No contradictions between changed files and the rest of the corpus
- No sections removed without a replacement or an explicit note explaining the removal
- Changed files maintain accurate cross-links to related specs

**Report format:**

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

**Failure handling:**
- `FAIL` in any phase: agent fixes the issue and re-runs from Phase 1. Cannot open a PR until all phases pass.
- `WARN`: agent lists all warnings in the PR body under "Acknowledged doc/spec review warnings."
- `PASS` (all phases): agent notes "Doc/spec review: all phases passed" in the PR body.
```

- [ ] **Step 3: Verify the edit looks correct**

Read `spec/10-process.md` around the newly inserted section to confirm it renders correctly and the `---` divider before §17 is still intact.

- [ ] **Step 4: Commit**

```bash
git add spec/10-process.md
git commit -m "docs(process): add §16.9 documentation and spec review process"
```

---

### Task 3: Create the `doc-review` skill

**Files:**
- Create: `C:\Users\ktjn\.claude\skills\doc-review\SKILL.md`

Note: `C:\Users\ktjn\.claude\skills\` is the personal skills directory for Claude Code on this machine. The directory may not exist yet — create it.

- [ ] **Step 1: Create the skills directory if it does not exist**

```bash
mkdir -p "C:/Users/ktjn/.claude/skills/doc-review"
```

- [ ] **Step 2: Write the skill file**

Create `C:\Users\ktjn\.claude\skills\doc-review\SKILL.md` with this content:

```markdown
---
name: doc-review
description: Use when the agent has modified any file under spec/ or docs/ and is about to open a PR or claim the change complete. Runs four-phase review: structural validation, cross-reference consistency, coverage completeness, and quality gates.
---

# Doc/Spec Review

**MANDATORY** before any PR that touches `spec/` or `docs/`. All four phases must pass. This is not optional.

## When to Use

- Any time you edit, create, or delete a file under `spec/` or `docs/`
- Before `superpowers:verification-before-completion`
- Before opening a PR

## Process

Run phases in order. On any FAIL, fix the issue and re-run from Phase 1.

### Phase 1: Structural Validation

For every changed file:
- [ ] No bare `TODO` or `TBD` placeholders
- [ ] Valid Markdown (no unclosed fences, broken headings)
- [ ] ADR files have: Status, Context, Decision, Consequences sections
- [ ] Spec files have a numbered heading matching their filename
- [ ] Diagram syntax is valid (Mermaid, etc.)

### Phase 2: Cross-Reference Consistency

For every ADR or spec cross-referenced in a changed file:
- [ ] Linked file exists at the stated path
- [ ] Reference is bidirectional and accurate
- [ ] Description of the linked decision matches the linked file

### Phase 3: Coverage Completeness

- [ ] If the change touches architecture, technology, deployment model, data model, security model, or roadmap scope → an ADR is also touched, OR the PR explicitly states why no ADR change is needed
- [ ] If an ADR is touched → all specs referencing that ADR are checked for staleness
- [ ] `spec/README.md` table reflects any added, renamed, or removed spec files

### Phase 4: Quality Gates

- [ ] No contradictions between changed files and the rest of the corpus
- [ ] No sections removed without a replacement or explicit note
- [ ] Changed files have accurate cross-links to related specs

## Report

After completing all phases, output this report:

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

## PR Requirements

- **FAIL**: Do not open PR. Fix issues and re-run from Phase 1.
- **WARN**: Include all warnings in PR body under "Acknowledged doc/spec review warnings."
- **PASS**: Include "Doc/spec review: all phases passed" in PR body.

## Scope

Start with files changed in the current branch (`git diff --name-only main`). Expand cross-reference checks across the full `spec/` corpus only for the specific linkages those files participate in.
```

- [ ] **Step 3: Verify the file was written correctly**

Read `C:\Users\ktjn\.claude\skills\doc-review\SKILL.md` and confirm frontmatter, all four phases, report format, and PR requirements are present.

- [ ] **Step 4: Commit**

```bash
git add "C:/Users/ktjn/.claude/skills/doc-review/SKILL.md"
git commit -m "feat(skill): add doc-review skill for mandatory spec/docs review"
```

---

### Task 4: Self-apply the doc-review skill and open PR

**Files:**
- No additional file changes (unless review finds gaps)

- [ ] **Step 1: Run the doc-review skill on the changes in this branch**

Identify changed files:
```bash
git diff --name-only main
```

Expected output includes: `spec/10-process.md` (the skill file is outside the repo, so only `spec/10-process.md` is in scope for the repo-side review).

Run all four phases against `spec/10-process.md`:

- Phase 1: Confirm no TODOs, valid Markdown, section heading matches filename (`10-process.md` → numbered heading present)
- Phase 2: The new §16.9 does not introduce new cross-references to check beyond what already exists
- Phase 3: This change IS a process/spec change → confirm it does not require an ADR (it documents agent behavior, not an architecture decision — state this explicitly in PR)
- Phase 4: Confirm no contradictions with existing §16.x sections

- [ ] **Step 2: Output the review report**

```
## Doc/Spec Review Report

### Phase 1: Structural Validation — PASS
### Phase 2: Cross-Reference Consistency — PASS
### Phase 3: Coverage Completeness — PASS
  - No ADR required: §16.9 documents agent process behavior, not an architecture, technology, deployment, data model, security, or roadmap decision.
### Phase 4: Quality Gates — PASS

### Summary
Overall: PASS
Warnings requiring PR acknowledgement: 0
Blockers requiring fix before PR: 0
```

If any phase fails, fix and re-run before continuing.

- [ ] **Step 3: Push the branch**

```bash
git push -u origin claude/doc-spec-review-process
```

- [ ] **Step 4: Open PR**

```bash
gh pr create \
  --title "feat(process): add doc/spec review process (§16.9) and doc-review skill" \
  --body "$(cat <<'EOF'
## Summary

- Adds §16.9 to `spec/10-process.md` defining a mandatory four-phase doc/spec review process for AI agents
- Adds `~/.claude/skills/doc-review/SKILL.md` implementing the review as an executable skill
- Trigger: mandatory whenever agent modifies files under `spec/` or `docs/`, before any PR

## Phases

1. Structural validation (no TODOs, valid Markdown, required ADR/spec sections, diagram syntax)
2. Cross-reference consistency (bidirectional links, accurate descriptions)
3. Coverage completeness (ADR sync required for architecture/technology/data model changes)
4. Quality gates (no contradictions, no unexplained removals, accurate cross-links)

## ADR/spec sync

No ADR update required: §16.9 documents AI agent process behavior, not an architecture, technology, deployment, data model, security model, or roadmap decision.

## Doc/spec review

Doc/spec review: all phases passed.

## Next slice

Wire the `doc-review` skill as an explicit step in `CLAUDE.md` if the team wants it enforced as a hook rather than relying on agent compliance.
EOF
)"
```

---

## Self-Review

**Spec coverage check:**
- ✅ §16.9 in `spec/10-process.md` — Task 2
- ✅ `doc-review` skill — Task 3
- ✅ Trigger rule (mandatory on `spec/` or `docs/` changes) — Task 3 skill + Task 2 spec
- ✅ Four phases with pass/fail criteria — Tasks 2 and 3
- ✅ Report format — Tasks 2 and 3
- ✅ Failure handling (FAIL blocks, WARN acknowledged in PR) — Tasks 2 and 3
- ✅ Self-application of the skill to this very PR — Task 4

**Placeholder scan:** No TBD, TODO, or "implement later" present.

**Type consistency:** No code types involved — all Markdown and YAML. Section numbering (§16.9) is consistent between spec and plan.
