# Roadmap Reconciliation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the active roadmap and repository guidance accurately reflect shipped work, unresolved prerequisites, security priority, and the next five issue-backed slices.

**Architecture:** Treat current code and integration tests as the status authority, then align the roadmap, agent guidance, plan archive, and GitHub issue backlog around that evidence. This is a documentation-only iteration; future runtime and architecture changes remain in separate issues and PRs.

**Tech Stack:** Markdown, Git, GitHub CLI, ripgrep, repository doc-review workflow

## Global Constraints

- Do not change runtime code, schemas, generated artifacts, dependencies, or ADR decisions.
- Preserve unrelated untracked `scripts/seed/**/__pycache__/` directories.
- Security P0 and correctness risks precede discretionary feature work.
- A roadmap item is complete only when all named slices are complete; otherwise split it.
- Every promoted implementation slice must have a GitHub issue before work begins.
- Move finished detailed plans to root `archived/plans/` and update every active reference.
- Pure documentation changes are exempt from `bash scripts/local-ci.sh`.

---

### Task 1: Correct roadmap truth and selection rules

**Files:**
- Modify: `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`
- Reference: `docs/superpowers/specs/2026-07-11-roadmap-reconciliation-design.md`

**Interfaces:**
- Consumes: implementation evidence recorded in the approved design
- Produces: the authoritative corrected backlog and near-term sequence

- [x] **Step 1: Replace the selection rule**

Replace the mechanical highest-tier rule with this decision order:

```markdown
1. Address a ready P0 security or correctness risk before discretionary feature work.
2. Otherwise pick the highest-value ready slice that is independently reviewable and has resolved prerequisites and architecture decisions.
```

- [x] **Step 2: Correct shipped and partial statuses**

Make these exact semantic changes:

```text
Saved Views: logs complete; traces open; metrics open.
SLO burn-rate evaluation: complete; retain only fast/slow presentation work under Service Health.
Admin RBAC mutations: complete; quota mutation and UI remain open.
OIDC/session tests: JWT and persistence coverage exist; callback/provider/cookie paths remain open.
Tenant middleware tests: query-api coverage exists; focused admin-service middleware coverage remains open.
Self-observability: OTLP initialization exists; /metrics endpoints and service-specific instruments remain open.
```

- [x] **Step 3: Decompose oversized or blocked entries**

Represent these as sequenced slices:

```text
Export: logs sync CSV/JSON; traces sync CSV/JSON; metrics sync CSV/JSON; OTLP contract decision; async architecture and implementation.
Fleet: inventory contract/data source; inventory UI; remote-configuration protocol; remote-configuration UI.
Quota: backend mutation/audit contract; frontend controls.
```

- [x] **Step 4: Update sequencing**

Set the near-term sequence to:

```text
1. Fail closed on missing session-signing secret.
2. Test OIDC login/callback behavior.
3. Add focused admin-service auth/tenant middleware coverage.
4. Complete Saved Views for traces.
5. Complete Saved Views for metrics.
```

- [x] **Step 5: Verify the corrected roadmap**

Run:

```bash
rg -n "Export APIs|Saved Views|SLO Burn|RBAC|quota|session signing|OIDC|tenant-scoping|Fleet|next" docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md
```

Expected: no statement says Saved Views or SLO burn-rate evaluation is wholly unfinished; Export and Fleet are not single ready-now slices; the session-secret fix is first.

### Task 2: Repair plan archive and governance pointers

**Files:**
- Archive completed plan: `archived/plans/2026-06-27-otel-demo-integration.md`
- Archive completed plan: `archived/plans/2026-06-30-prometheus-remote-write.md`
- Archive completed plan: `archived/plans/2026-07-01-admin-ui-cleanup.md`
- Archive completed plan: `archived/plans/2026-07-01-alert-inhibition-rules.md`
- Normalize completed plan archive: `archived/plans/2026-06-26-ui-usability-remediation.md`
- Modify: `AGENTS.md`
- Modify: `docs/agent-context.md`
- Modify: affected Markdown references found by `rg`

**Interfaces:**
- Consumes: corrected roadmap from Task 1
- Produces: one active-roadmap pointer and one canonical finished-plan archive

- [x] **Step 1: Move the five completed plans with `git mv`**

Run each exact source/destination pair listed above. Do not move the active unified roadmap.

- [x] **Step 2: Correct repository guidance**

Change `AGENTS.md` Phase Plan Status so follow-on work uses:

```text
docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md
```

Update `docs/agent-context.md` to name the security-first selection rule and the session-secret slice as next.

- [x] **Step 3: Fix moved-plan references**

Run:

```bash
rg -n "docs/superpowers/plans/" . -g "*.md" | rg "2026-06-26-ui-usability-remediation|2026-06-27-otel-demo-integration|2026-06-30-prometheus-remote-write|2026-07-01-admin-ui-cleanup|2026-07-01-alert-inhibition-rules"
```

Expected after edits: zero matches.

- [x] **Step 4: Verify active-plan inventory**

Run:

```powershell
Get-ChildItem docs/superpowers/plans -File | Select-Object -ExpandProperty Name
```

Expected: only `2026-06-19-unified-feature-roadmap.md` and this in-progress reconciliation plan.

### Task 3: Create issue-backed near-term slices

**Files:**
- External state: GitHub issues in `ktjn/observable`
- Modify: `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md` with created issue links

**Interfaces:**
- Consumes: five-slice order from Task 1
- Produces: claimable backlog entries for future iterations

- [x] **Step 1: Create five issues**

Use `gh issue create` with one issue for each exact slice:

```text
security: fail closed when session-signing secret is missing
test(auth): cover OIDC login and callback behavior
test(admin): cover authentication and tenant-scoping middleware
feat(saved-views): support trace explorer configurations
feat(saved-views): support metrics explorer configurations
```

Each body must link #528, quote its roadmap acceptance criterion, and state that the issue is not claimed until implementation starts. Do not add `in-progress` or an assignee.

- [x] **Step 2: Link issue numbers from the roadmap**

Add each created issue number beside its corresponding near-term slice.

- [x] **Step 3: Verify issue state**

Run:

```bash
gh issue list --state open --limit 20
```

Expected: #528 remains assigned and in progress; the five implementation issues are open and unassigned.

### Task 4: Review, archive this plan, and publish

**Files:**
- Move: `docs/superpowers/plans/2026-07-11-roadmap-reconciliation.md` → `archived/plans/2026-07-11-roadmap-reconciliation.md`
- Modify: any reference that still points to the active plan path

**Interfaces:**
- Consumes: completed Tasks 1–3
- Produces: reviewed documentation and a PR that closes #528

- [x] **Step 1: Mark completed implementation and pre-publication work, then archive the plan**

Check Tasks 1-3 and Task 4's completed pre-publication boxes, move the file to root
`archived/plans/`, and update references. Leave publication Steps 5-6 unchecked until their
commit/push and pull-request actions have actually been performed.

- [x] **Step 2: Run structural checks**

Run:

```bash
git diff --check
rg -n "T[B]D|T[O]DO|implement l[a]ter|fill in d[e]tails" AGENTS.md docs/superpowers archived/plans
```

Expected: `git diff --check` succeeds; the placeholder search finds no new placeholder in changed files.

- [x] **Step 3: Run the four-phase doc review**

Apply `doc-review` to every changed or moved Markdown file. Fix any FAIL and restart at Phase 1.

- [x] **Step 4: Obtain specialist review**

Invoke planning-steward and spec-steward on the final diff. Fix every blocking issue they report.

- [ ] **Step 5: Commit and push**

Stage only intended Markdown files and moves, excluding `scripts/seed/**/__pycache__/`, then commit:

```bash
git commit -m "docs(issue-528): reconcile roadmap and archive completed plans"
git push
```

- [ ] **Step 6: Open the pull request**

Create a PR whose body includes:

```text
Closes #528
Docs-only local-CI exemption.
ADR sync not required: no architecture, technology, deployment, data, or security model changed.
Testcontainers not applicable: no runtime or dependency-boundary change.
Regression gates unchanged.
Doc/spec review: all phases passed.
```

Remove `in-progress` from #528 and add `ready-for-review`, creating that label if absent.
