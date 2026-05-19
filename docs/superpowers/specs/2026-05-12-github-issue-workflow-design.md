# GitHub Issue Workflow — Revised Design

**Date:** 2026-05-12
**Status:** Approved
**Scope:** `issue-worker.agent.md`, new `code-reviewer.agent.md`, `README.md` routing table, `AGENTS.md` step summary

---

## Problem

The current issue workflow has three gaps:

1. **No "complete" signal on the issue** — the issue moves from `in-progress` to `ready-for-review` when a PR is opened, but is never explicitly closed or marked approved by an agent. GitHub's `Closes #N` closes it on merge, but there is no intermediate approved state.
2. **No second-agent code review** — all review is human-only. There is no structured automated review step before human approval.
3. **No stale-lock recovery** — if a worker crashes after claiming an issue, the issue stays `in-progress` indefinitely and blocks other workers.

---

## Label State Machine

The issue lifecycle is a defined state machine. Every transition is explicit; no state is skipped.

```
(open, unassigned)
      │
      │  worker self-assigns + adds label
      ▼
  in-progress  ◄─── stale recovery re-enters here
      │
      │  worker opens PR, removes in-progress
      ▼
ready-for-review
      │
      │  reviewer approves PR, removes ready-for-review
      ▼
   approved
      │
      │  PR merged (Closes #N auto-closes issue)
      ▼
   (closed)
```

**Labels used:**

| Label | Meaning |
|---|---|
| `in-progress` | A worker has claimed the issue and is actively implementing |
| `ready-for-review` | PR is open; awaiting reviewer agent and human sign-off |
| `approved` | Reviewer agent has approved the PR; ready to merge |

The issue is **not** closed at approval time. Keeping it open until merge ensures the issue remains visible as a signal that the PR still needs to land.

---

## Worker Agent Changes (`issue-worker.agent.md`)

### Phase 1 — Find & Claim (addition)

After self-assigning and adding `in-progress`, post a progress comment:

```
gh issue comment <NUMBER> --body "Agent starting work. Branch: <branch-name>"
```

This gives humans a visible link without checking labels or searching branches.

### Phase 7 — PR (additions)

After opening the PR, also add the `ready-for-review` label to the PR itself so GitHub's PR list filters work:

```
gh pr edit <PR-NUMBER> --add-label "ready-for-review"
```

Remove `in-progress` and add `ready-for-review` on the issue as before:

```
gh issue edit <NUMBER> --remove-label "in-progress" --add-label "ready-for-review"
```

### Phase 8 — Chain to Reviewer (new)

After the PR is open, invoke the `code-reviewer` agent as a subagent, passing the PR number and issue number. The worker's own job ends here — it does not wait for or act on the reviewer's output.

The worker output report gains one line:

```
**Reviewer:** dispatched → code-reviewer (Opus 4.7) on PR #<NUMBER>
```

### Stale Lock Recovery (new section in Constraints)

An issue is stale-locked when it carries `in-progress` but has no open PR and no recent branch activity.

**Detection:**
```
gh issue list --label="in-progress" --state=open --json number,title,assignees,updatedAt
```

Any issue with `in-progress` and no linked PR that has not been updated in over 24 hours is a candidate.

**Recovery steps:**
1. Check for a branch matching `fix/issue-<NUMBER>-*` or `feat/issue-<NUMBER>-*`.
   - If it exists with commits in the last 24 hours: the worker may still be active — skip this issue.
2. If no branch or no recent commits: post a comment (`"Reclaiming stale issue — previous worker appears inactive"`), unassign the previous assignee, reassign to `@me`, and continue from Phase 1.
3. If a partial branch exists: push new work on top rather than deleting it.

**Constraint bullet added:** *"If you find an `in-progress` issue with no open PR and no branch activity in 24 hours, apply the stale-lock recovery procedure before skipping it."*

---

## Reviewer Agent (`code-reviewer.agent.md`) — New File

**Model:** Opus 4.7 (fixed — strongest reasoning, consistent regardless of worker model)
**Invoked by:** issue worker as a subagent after PR is opened
**Input:** PR number, issue number

### Review Steps

1. Read the full PR diff: `gh pr diff <NUMBER>`
2. Read the PR description and linked issue body: `gh pr view <NUMBER>`, `gh issue view <ISSUE>`
3. Work through the review checklist (see below)
4. Read any `spec/` files relevant to the changed surface
5. Render verdict and post structured review comment
6. Execute the approval or change-request action

### Review Checklist

The reviewer checks each item and records a result for the structured comment:

- Failing test committed before fix (bugs only)
- All acceptance criteria from the issue are covered by tests
- `bash scripts/local-ci.sh` passed (confirmed from PR description or CI output)
- Testcontainers test added if a DB/queue boundary was touched, or reason stated
- ADR sync satisfied, or reason stated why not required
- Spec alignment — no conflict with relevant `spec/` files
- No security issues — injection, auth bypass, unsafe unwraps in handler paths

### On Approval

```
gh pr review <NUMBER> --approve --body "<structured comment>"
gh issue edit <ISSUE> --remove-label "ready-for-review" --add-label "approved"
```

### On Changes Requested

```
gh pr review <NUMBER> --request-changes --body "<structured comment>"
```

Leave the issue at `ready-for-review`. The worker addresses feedback and re-pushes. The reviewer is then re-invoked: either the worker chains to it again after the re-push, or a human invokes it directly via `code-reviewer.agent.md`. Either path is valid; the re-invocation method does not affect the reviewer's behaviour.

### Structured Review Comment Format

Each checklist item is marked `[x]` (pass) or `[ ]` (fail/concern) based on actual findings. A fail on any item results in "Changes requested".

```markdown
## Code Review — Opus 4.7

**Verdict:** ✅ Approved / ❌ Changes requested

### Checklist
- [x/] Failing test committed before fix (bugs) / N/A — feature issue
- [x/] All acceptance criteria covered by tests
- [x/] local-ci.sh passed
- [x/] Testcontainers: <added / not required — reason>
- [x/] ADR sync: <satisfied / not required — reason>
- [x/] Spec alignment: <clean / concern — description>
- [x/] No security issues

### Notes
<specific findings per failed item, or "none">
```

---

## README Routing Table Update

Add a `code-reviewer` row to the routing table in `.github/agents/README.md`:

| Role | Agent file | Invocation | Primary surface |
|---|---|---|---|
| **Code Reviewer** | `code-reviewer.agent.md` | Subagent (invoked by issue-worker) | PR diff review, approval, issue label update |

Add a routing rule: when the task is "review an open PR linked to an issue" (i.e. the PR exists and `ready-for-review` is set) → invoke `code-reviewer`.

---

## AGENTS.md Step Summary Update

The GitHub Issues Workflow step 7 in `AGENTS.md` currently ends at "Open a PR, remove `in-progress`, add `ready-for-review`." It gains:

- Step 7 addition: also add `ready-for-review` label to the PR itself
- Step 8 (new): reviewer agent (Opus 4.7) is automatically chained; approves PR, updates issue to `approved`
- Step 9 (new): PR is merged by a human; `Closes #N` auto-closes the issue

---

## Files Changed

| File | Change |
|---|---|
| `.github/agents/issue-worker.agent.md` | Phase 1 progress comment, Phase 7 PR label, Phase 8 reviewer chain, stale-lock recovery section |
| `.github/agents/code-reviewer.agent.md` | New file |
| `.github/agents/README.md` | New routing row and rule for code-reviewer |
| `AGENTS.md` | Steps 7–9 update in GitHub Issues Workflow section |
