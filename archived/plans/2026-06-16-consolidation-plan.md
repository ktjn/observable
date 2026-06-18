# Repo Consolidation — Post-Modelable-Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Archive all completed modelable-migration and other confirmed-done plans/specs, update agent-context + memory to reflect current state, tidy `.mdl` header comments, document the binding duplication, and scan for dead generated artifacts.

**Architecture:** Pure docs and comment-only change — no Rust or TypeScript compilation units are modified. Files are moved with `git mv` to preserve history. All changes land in one branch `chore/consolidate-post-modelable`, opened as a single PR. No `bash scripts/local-ci.sh` required (docs-only exemption per `AGENTS.md`). Exception: if `scripts/local-ci.sh` has a hard-gated `.mdl` diff-check, run it for Task 8 only.

**Tech Stack:** Git (file moves), Markdown edits.

---

## File Map

| Action | Path |
|---|---|
| Create dir | `archived/specs/` |
| Move (×13) | `docs/superpowers/plans/2026-06-*-modelable-*.md` → `archived/plans/` |
| Move (×12) | `docs/superpowers/specs/2026-06-1*-*modelable*-design.md` + `*phase4*` → `archived/specs/` |
| Move (×6) | completed non-modelable plans → `archived/plans/` |
| Move (×6) | matching non-modelable specs → `archived/specs/` |
| Delete | `docs/superpowers/plans/2026-05-06-identity-provider-zitadel.md` (duplicate) |
| Modify | `docs/agent-context.md` |
| Modify | `C:\Users\ktjn\.claude\projects\C--git-Observable\memory\project_modelable_migration.md` |
| Modify | `C:\Users\ktjn\.claude\projects\C--git-Observable\memory\MEMORY.md` |
| Modify (×9) | `models/{admin,alerts,dashboards,incidents,logs,metrics,nlq,notifications,slos}.mdl` |
| Modify (×2) | `models/tracing.mdl`, `models/logs.mdl` (binding duplication note) |

---

## Task 1: Create branch

- [x] **Create and switch to the consolidation branch**

```bash
git checkout -b chore/consolidate-post-modelable
```

Expected: `Switched to a new branch 'chore/consolidate-post-modelable'`

---

## Task 2: Archive modelable migration plans

Move all 13 completed modelable migration plans from `docs/superpowers/plans/` to `archived/plans/`.

- [x] **Move the plans with git mv**

```bash
cd C:/git/Observable
git mv docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md archived/plans/
git mv docs/superpowers/plans/2026-06-10-modelable-pilot-span-row-types.md archived/plans/
git mv docs/superpowers/plans/2026-06-12-tracing-attributes-json-type.md archived/plans/
git mv docs/superpowers/plans/2026-06-13-logs-modelable-migration.md archived/plans/
git mv docs/superpowers/plans/2026-06-13-metrics-modelable-migration.md archived/plans/
git mv docs/superpowers/plans/2026-06-13-tracing-typescript-field-case.md archived/plans/
git mv docs/superpowers/plans/2026-06-14-admin-members-modelable-migration.md archived/plans/
git mv docs/superpowers/plans/2026-06-14-alerts-modelable-migration.md archived/plans/
git mv docs/superpowers/plans/2026-06-14-dashboards-modelable-migration.md archived/plans/
git mv docs/superpowers/plans/2026-06-14-incidents-modelable-migration.md archived/plans/
git mv docs/superpowers/plans/2026-06-14-notifications-modelable-migration.md archived/plans/
git mv docs/superpowers/plans/2026-06-14-slos-modelable-migration.md archived/plans/
git mv docs/superpowers/plans/2026-06-15-nlq-visualization-modelable-migration.md archived/plans/
```

- [x] **Commit**

```bash
git commit -m "docs: archive 13 completed modelable migration plans"
```

---

## Task 3: Archive modelable migration specs

Create `archived/specs/` and move all 12 completed modelable migration design docs.

- [x] **Create archived/specs/ and move the specs**

```bash
mkdir -p archived/specs
git mv docs/superpowers/specs/2026-06-12-tracing-attributes-json-type-design.md archived/specs/
git mv docs/superpowers/specs/2026-06-13-logs-modelable-migration-design.md archived/specs/
git mv docs/superpowers/specs/2026-06-13-metrics-modelable-migration-design.md archived/specs/
git mv docs/superpowers/specs/2026-06-13-tracing-typescript-field-case-design.md archived/specs/
git mv docs/superpowers/specs/2026-06-14-admin-members-modelable-migration-design.md archived/specs/
git mv docs/superpowers/specs/2026-06-14-alerts-modelable-migration-design.md archived/specs/
git mv docs/superpowers/specs/2026-06-14-dashboards-modelable-migration-design.md archived/specs/
git mv docs/superpowers/specs/2026-06-14-incidents-modelable-migration-design.md archived/specs/
git mv docs/superpowers/specs/2026-06-14-notifications-modelable-migration-design.md archived/specs/
git mv docs/superpowers/specs/2026-06-14-slos-modelable-migration-design.md archived/specs/
git mv docs/superpowers/specs/2026-06-15-nlq-visualization-modelable-migration-design.md archived/specs/
git mv docs/superpowers/specs/2026-06-15-phase4-modelable-cleanup-design.md archived/specs/
```

- [x] **Commit**

```bash
git commit -m "docs: archive 12 completed modelable migration specs"
```

---

## Task 4: Archive confirmed-complete non-modelable plans and specs

Six non-modelable plans each have a `- [x] … (COMPLETED YYYY-MM-DD)` summary checkbox and matching agent-context confirmation.

- [x] **Move the 6 completed plans and their 6 matching specs**

```bash
git mv docs/superpowers/plans/2026-05-18-p5-s1-incident-timeline.md archived/plans/
git mv docs/superpowers/plans/2026-05-20-p5-s4-topology-impact-view.md archived/plans/
git mv docs/superpowers/plans/2026-05-30-clickhouse-insert-efficiency.md archived/plans/
git mv docs/superpowers/plans/2026-05-30-p4-s4-dashboard-rebac.md archived/plans/
git mv docs/superpowers/plans/2026-05-31-context-preservation.md archived/plans/
git mv docs/superpowers/plans/2026-05-31-live-tail.md archived/plans/

git mv docs/superpowers/specs/2026-05-18-p5-s1-incident-timeline-design.md archived/specs/
git mv docs/superpowers/specs/2026-05-20-p5-s4-topology-impact-view-design.md archived/specs/
git mv docs/superpowers/specs/2026-05-30-clickhouse-insert-efficiency-design.md archived/specs/
git mv docs/superpowers/specs/2026-05-30-p4-s4-dashboard-rebac-design.md archived/specs/
git mv docs/superpowers/specs/2026-05-31-context-preservation-design.md archived/specs/
git mv docs/superpowers/specs/2026-05-31-live-tail-design.md archived/specs/
```

- [x] **Commit**

```bash
git commit -m "docs: archive 6 confirmed-complete non-modelable plans and specs"
```

---

## Task 5: Remove identity-provider-zitadel.md duplicate

`archived/plans/2026-05-06-identity-provider-zitadel.md` already exists. The copy in `docs/superpowers/plans/` is a duplicate.

- [x] **Delete the duplicate**

```bash
git rm docs/superpowers/plans/2026-05-06-identity-provider-zitadel.md
```

- [x] **Verify the archived copy is still there**

```bash
ls archived/plans/2026-05-06-identity-provider-zitadel.md
```

Expected: file exists.

- [x] **Commit**

```bash
git commit -m "docs: remove duplicate identity-provider-zitadel plan (already in archived/)"
```

---

## Task 6: Update agent-context.md

Four targeted edits to `docs/agent-context.md`:

**Edit 1 — Update the modelable section header** (line ~151):

Find:
```
## Modelable Type-Mapping Migration (Phase 3 complete, 2026-06-15)
```
Replace with:
```
## Modelable Type-Mapping Migration (Phase 4 complete, 2026-06-15)
```

**Edit 2 — Update the plan reference in the modelable section** (line ~161):

Find:
```
`docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` for the Phase 1 backlog and per-domain design specs.
```
Replace with:
```
`archived/plans/2026-06-08-modelable-type-mapping-migration-plan.md` for the Phase 1 backlog and per-domain design specs.
```

**Edit 3 — Fix the stale p5-s1 path in the completed plans list** (line ~40):

Find:
```
  - `docs/superpowers/plans/2026-05-18-p5-s1-incident-timeline.md` — P5-S1 incident timeline with source links (COMPLETED 2026-05-18)
```
Replace with:
```
  - `archived/plans/2026-05-18-p5-s1-incident-timeline.md` — P5-S1 incident timeline (COMPLETED 2026-05-18)
```

**Edit 4 — Add new archived-plan entries** to the "Completed / archived detailed plans" list, after the existing last entry in that list:

```markdown
  - `archived/plans/2026-05-20-p5-s4-topology-impact-view.md` — P5-S4 topology-aware impact view panel in IncidentDetailPage (COMPLETED 2026-05-20)
  - `archived/plans/2026-05-30-clickhouse-insert-efficiency.md` — stream-processor batching + storage-writer WriteBuffer (COMPLETED 2026-05-30)
  - `archived/plans/2026-05-30-p4-s4-dashboard-rebac.md` — fine-grained dashboard ReBAC via OpenFGA (COMPLETED 2026-05-30)
  - `archived/plans/2026-05-31-context-preservation.md` — global service filter preserved across all signal tabs (COMPLETED 2026-05-31)
  - `archived/plans/2026-05-31-live-tail.md` — live-tail streaming toggle for LogExplorer (COMPLETED 2026-05-31)
  - `archived/plans/2026-06-08-modelable-type-mapping-migration-plan.md` — modelable type-mapping migration master plan, all four phases (COMPLETED 2026-06-15)
  - `archived/plans/2026-06-10-modelable-pilot-span-row-types.md` — Phase 2 pilot: SpanRow/SpanEventRow from tracing.mdl
  - `archived/plans/2026-06-12-tracing-attributes-json-type.md` — Phase 2.4: map<string,json> attributes type
  - `archived/plans/2026-06-13-logs-modelable-migration.md` — Phase 3.1: logs domain
  - `archived/plans/2026-06-13-metrics-modelable-migration.md` — Phase 3.2: metrics domain
  - `archived/plans/2026-06-13-tracing-typescript-field-case.md` — Phase 2.5: TypeScript snake_case generation
  - `archived/plans/2026-06-14-admin-members-modelable-migration.md` — Phase 3.4: admin/members domain
  - `archived/plans/2026-06-14-alerts-modelable-migration.md` — Phase 3.7: alerts domain
  - `archived/plans/2026-06-14-dashboards-modelable-migration.md` — Phase 3.8: dashboards domain
  - `archived/plans/2026-06-14-incidents-modelable-migration.md` — Phase 3.6: incidents domain
  - `archived/plans/2026-06-14-notifications-modelable-migration.md` — Phase 3.3: notifications domain
  - `archived/plans/2026-06-14-slos-modelable-migration.md` — Phase 3.5: slos domain
  - `archived/plans/2026-06-15-nlq-visualization-modelable-migration.md` — Phase 3.9: nlq/visualization domain (last regular Phase 3 domain)
```

- [x] **Make all four edits to `docs/agent-context.md`** using your editor or Edit tool

- [x] **Verify no remaining `docs/superpowers/plans/2026-06-` references in agent-context.md**

```bash
grep "docs/superpowers/plans/2026-06-" docs/agent-context.md
```

Expected: no output (all 2026-06 plan references are now under `archived/`).

- [x] **Verify the active-plan references are untouched**

```bash
grep "docs/superpowers/plans/2026-05-07-remaining-roadmap" docs/agent-context.md
grep "docs/superpowers/plans/2026-06-04-observability-feature-parity" docs/agent-context.md
grep "docs/superpowers/plans/2026-06-10-p9-s5-service-catalog" docs/agent-context.md
```

Expected: each grep finds exactly one line.

- [x] **Commit**

```bash
git add docs/agent-context.md
git commit -m "docs(agent-context): update modelable section to Phase 4, fix archived plan paths"
```

---

## Task 7: Update memory

The project memory record says "Phase 2 (tracing pilot) done as of 2026-06-13, Phase 3 (remaining domains) not started" — this is stale.

- [x] **Read the current memory file**

Path: `C:\Users\ktjn\.claude\projects\C--git-Observable\memory\project_modelable_migration.md`

- [x] **Replace the body of that file** with the updated content below. Keep the frontmatter unchanged; only replace the body after the `---` closing the frontmatter:

```markdown
All four phases of the modelable type-mapping migration are complete as of 2026-06-15.

- **Phase 1** (modelable extension): tracked upstream at github.com/ktjn/modelable; Observable pinned v0.2.1→v0.4.0 across phases.
- **Phase 2** (tracing pilot): complete. Rust `SpanRow`/`SpanEventRow` and TypeScript `Span`/`SpanEvent` generated from `models/tracing.mdl`.
- **Phase 3** (all 10 domains): complete. Every in-scope domain (tracing, logs, metrics, notifications, admin/members, slos, incidents, alerts, dashboards, nlq/visualization) has generated TypeScript artifacts. Tracing and logs also have generated Rust Row types. `3.5b Schemas` is deliberately deferred — no frontend consumer exists.
- **Phase 4** (cleanup & documentation): complete. ADR-032 written. agent-context updated. Dead code audit found nothing to remove.

**Master plan (archived):** `archived/plans/2026-06-08-modelable-type-mapping-migration-plan.md`
**ADR:** `spec/adr/ADR-032-modelable-type-mapping-adoption.md`
**Phase 1 backlog (9 items):** documented in the master plan's "Phase 1 backlog" section — modelable gaps blocking further Rust-layer migration for 8 domains.

**Why:** Replaces Observable's drift-prone hand-written type-mapping layers (47 backend API types, 41 frontend interfaces, ~6 explicit From/Into mappings) with `.mdl`-generated artifacts as the single source of truth.
**How to apply:** When adding or changing domain types, author/edit the `.mdl` file in `models/`, regenerate with `modelable compile`, and commit the generated artifacts. See agent-context.md "Modelable Type-Mapping Migration" section for the full regeneration command.
```

- [x] **Update `MEMORY.md`** — change the modelable migration entry from:
  ```
  - [Modelable Migration](project_modelable_migration.md) — multi-phase plan adopting modelable as type-mapping source of truth; Phase 2 (tracing pilot) done as of 2026-06-13, Phase 3 (remaining domains) not started
  ```
  to:
  ```
  - [Modelable Migration](project_modelable_migration.md) — all four phases complete as of 2026-06-15; master plan archived; ADR-032 written; 9 Phase 1 backlog items remain upstream
  ```

- [x] **Commit memory updates**

```bash
git add "C:/Users/ktjn/.claude/projects/C--git-Observable/memory/project_modelable_migration.md"
git add "C:/Users/ktjn/.claude/projects/C--git-Observable/memory/MEMORY.md"
git commit -m "memory: update modelable migration record to reflect all four phases complete"
```

---

## Task 8: Add header comments to models/*.mdl

`models/tracing.mdl` already has a multi-line file-level header comment. The other 9 `.mdl` files start directly with `domain X {`. Add a short file-level comment above each `domain` declaration documenting what's generated.

- [x] **Edit `models/admin.mdl`** — insert above `domain admin {`:

```
// Domain: admin (tenant members). Generated: apps/frontend/src/api/generated/admin/ (TypeScript).
// Rust: not generated — timestamp gap (Phase 1 backlog item 5). See admin_members.rs for hand-written types.
```

- [x] **Edit `models/alerts.mdl`** — insert above `domain alerts {`:

```
// Domain: alerts (alert rules and firings). Generated: apps/frontend/src/api/generated/alerts/ (TypeScript).
// Rust: not generated — timestamp gap (Phase 1 backlog item 5). See services/query-api/src/alerts.rs.
```

- [x] **Edit `models/dashboards.mdl`** — insert above `domain dashboards {`:

```
// Domain: dashboards. Generated: apps/frontend/src/api/generated/dashboards/ (TypeScript).
// Rust: not generated — timestamp gap (Phase 1 backlog item 5). See services/query-api/src/dashboards.rs.
```

- [x] **Edit `models/incidents.mdl`** — insert above `domain incidents {`:

```
// Domain: incidents and incident events. Generated: apps/frontend/src/api/generated/incidents/ (TypeScript).
// Rust: not generated — timestamp gap (Phase 1 backlog item 5). See services/query-api/src/incidents.rs.
```

- [x] **Edit `models/logs.mdl`** — insert above `domain logs {`:

```
// Domain: logs. Generated: libs/domain/src/generated/logs/ (Rust Row types), apps/frontend/src/api/generated/logs/ (TypeScript).
// Canonical Rust LogRecord remains hand-written (enum and conversion gaps; see Phase 1 backlog items 1-3).
```

- [x] **Edit `models/metrics.mdl`** — insert above `domain metrics {`:

```
// Domain: metrics. Generated: apps/frontend/src/api/generated/metrics/ (TypeScript).
// Rust: not generated — array rust.type + non-optional array + enum gaps (Phase 1 backlog items 1-3).
```

- [x] **Edit `models/nlq.mdl`** — insert above `domain nlq {`:

```
// Domain: NLQ (natural-language query IR) and visualization. Generated: apps/frontend/src/api/generated/nlq/ (TypeScript).
// Rust: not generated. NlqIr is extended in nlq.ts (Option<T>|null and array<enum> gaps, items 8-9).
```

- [x] **Edit `models/notifications.mdl`** — insert above `domain notifications {`:

```
// Domain: notification channels. Generated: apps/frontend/src/api/generated/notifications/ (TypeScript).
// Rust: not generated — enum-as-String gap (Phase 1 backlog item 3). See query-api/src/notifications.rs.
```

- [x] **Edit `models/slos.mdl`** — insert above `domain slos {`:

```
// Domain: SLO definitions. Generated: apps/frontend/src/api/generated/slos/ (TypeScript).
// Rust: not generated — timestamp gap (Phase 1 backlog item 5). See services/query-api/src/slos.rs.
```

- [x] **Commit**

```bash
git add models/
git commit -m "docs(models): add file-level header comments to .mdl files lacking them"
```

---

## Task 9: Document binding ch-observable duplication

Both `models/tracing.mdl` and `models/logs.mdl` declare `binding ch-observable { adapter: clickhouse }`. This causes `sqlite3.IntegrityError: UNIQUE constraint failed` on a clean workspace compile (Phase 1 backlog item 4). Add a visible comment so the next developer doesn't spend time debugging it.

- [x] **In `models/tracing.mdl`**, find the `binding ch-observable {` declaration and add a comment immediately above it:

```
// NOTE: this binding is also declared in logs.mdl. Duplicate bindings cause a UNIQUE
// constraint error on a clean-workspace compile (Phase 1 backlog item 4 in
// archived/plans/2026-06-08-modelable-type-mapping-migration-plan.md). Workaround:
// compile single-file or use an incremental registry (existing .modelable/registry.db).
```

- [x] **In `models/logs.mdl`**, find the `binding ch-observable {` declaration and add the same comment immediately above it.

- [x] **Commit**

```bash
git add models/tracing.mdl models/logs.mdl
git commit -m "docs(models): document duplicate ch-observable binding (Phase 1 backlog item 4)"
```

---

## Task 10: Dead-code scan

Read-only. Confirm the generated artifact directories are clean; flag anything suspicious rather than deleting.

- [x] **Verify generated Rust directories match current .mdl files**

```bash
ls libs/domain/src/generated/
```

Expected: `logs/` and `tracing/` (the only two domains with Rust generation). If you see any other directory, it is orphaned — note it as a finding.

- [x] **Verify generated TypeScript directories match current .mdl files**

```bash
ls apps/frontend/src/api/generated/
```

Expected: `admin/`, `alerts/`, `dashboards/`, `incidents/`, `logs/`, `metrics/`, `nlq/`, `notifications/`, `slos/`, `tracing/` — one per `.mdl` file. If any directory is missing or extra, note it as a finding.

- [x] **Spot-check re-exports are used**

Run each grep. Each should return at least one hit from outside the `api/generated/` directory itself:

```bash
grep -r "from.*api/generated/admin" apps/frontend/src --include="*.ts" --include="*.tsx" -l | grep -v "api/generated"
grep -r "from.*api/generated/nlq" apps/frontend/src --include="*.ts" --include="*.tsx" -l | grep -v "api/generated"
grep -r "from.*api/generated/slos" apps/frontend/src --include="*.ts" --include="*.tsx" -l | grep -v "api/generated"
```

Expected: at least `api/admin-members.ts`, `api/nlq.ts`, `api/slos.ts` appear. If a domain's generated artifacts are never imported from outside `api/generated/`, flag for deletion — do not delete without noting in this PR.

- [x] **Commit the scan result as a note** (or skip commit if no findings)

If no unexpected findings: no commit needed.  
If findings exist: create `docs/superpowers/specs/2026-06-16-dead-code-findings.md` with a brief list and commit it:

```bash
git add docs/superpowers/specs/2026-06-16-dead-code-findings.md
git commit -m "docs: record dead-code scan findings from consolidation"
```

---

## Task 11: Open pull request

- [x] **Push the branch**

```bash
git push -u origin chore/consolidate-post-modelable
```

- [x] **Open the PR**

```bash
gh pr create \
  --title "chore: consolidate post-modelable-migration plans, specs, and docs" \
  --body "$(cat <<'EOF'
## Summary

- Archives 13 completed modelable migration plans to `archived/plans/`
- Creates `archived/specs/` and moves 12 modelable migration design docs
- Archives 6 confirmed-complete non-modelable plans and their specs (P5-S1, P5-S4, ClickHouse insert efficiency, P4-S4 ReBAC, context preservation, live tail)
- Removes the duplicate `docs/superpowers/plans/2026-05-06-identity-provider-zitadel.md` (already in `archived/`)
- Updates `docs/agent-context.md`: modelable section to Phase 4, fixes stale p5-s1 path, adds 18 new archived-plan entries
- Updates project memory: modelable migration record now reflects all four phases complete
- Adds file-level header comments to 9 `.mdl` files
- Documents the `ch-observable` duplicate-binding gotcha in `tracing.mdl` and `logs.mdl`
- Dead-code scan confirmed no orphaned generated artifacts

## Test plan

- [x] `ls docs/superpowers/plans/` — only active plans remain: `remaining-roadmap-plan`, `observability-feature-parity-plan`, `p9-s5-service-catalog-health-signals`, `seed-generator`, `p5-s3-runbook-attachment`, `admin-console-member-management`, `2026-06-16-consolidation-plan`
- [x] `ls archived/plans/ | wc -l` — count increased by 19 (13 modelable + 6 non-modelable) vs. pre-PR
- [x] `ls archived/specs/` — new directory with 18 files (12 modelable + 6 non-modelable)
- [x] `grep "docs/superpowers/plans/2026-06-" docs/agent-context.md` — no output
- [x] `grep "Phase 4 complete" docs/agent-context.md` — one hit
- [x] All active plan links in agent-context.md resolve to real files

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Self-Review

**Spec coverage check:**

| Spec section | Covered by task |
|---|---|
| 1a. Archive 13 modelable plans | Task 2 |
| 1b. Archive 12 modelable specs | Task 3 |
| 1c. Remove duplicate zitadel.md | Task 5 |
| 1d. Verify + archive 6 other completed plans | Task 4 |
| 2a. Update agent-context.md | Task 6 |
| 2b. Update memory | Task 7 |
| 3a. .mdl header comments | Task 8 |
| 3b. Document binding duplication | Task 9 |
| 3c. Dead-code scan | Task 10 |
| Branch + PR | Tasks 1, 11 |

**Placeholder scan:** No TBDs, no "similar to task N", no undescribed steps. Each file move is enumerated exactly.

**Type consistency:** No code types defined across tasks (docs only). File paths are consistent throughout.
