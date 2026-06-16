---
name: consolidation-2026-06-16
description: Design for post-modelable-migration repo consolidation — archive completed plans/specs, update docs/memory, tidy .mdl files, scan for dead generated code
metadata:
  type: project
---

# Repo Consolidation — Post-Modelable-Migration

**Date:** 2026-06-16  
**Status:** Approved  
**Scope:** Option B — all completed plans (modelable + other confirmed-done non-modelable), matching specs, docs/memory updates, .mdl tidy, dead-code scan.

---

## 1. Plans & Specs Archiving

### 1a. Modelable migration plans → `archived/plans/`

All four phases of the modelable type-mapping migration are complete as of 2026-06-15. Move every per-domain plan:

| File | Notes |
|---|---|
| `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` | Master plan (all phases) |
| `docs/superpowers/plans/2026-06-10-modelable-pilot-span-row-types.md` | Phase 2 pilot |
| `docs/superpowers/plans/2026-06-12-tracing-attributes-json-type.md` | Phase 2.4 |
| `docs/superpowers/plans/2026-06-13-logs-modelable-migration.md` | Phase 3.1 |
| `docs/superpowers/plans/2026-06-13-metrics-modelable-migration.md` | Phase 3.2 |
| `docs/superpowers/plans/2026-06-13-tracing-typescript-field-case.md` | Phase 2.5 |
| `docs/superpowers/plans/2026-06-14-admin-members-modelable-migration.md` | Phase 3.4 |
| `docs/superpowers/plans/2026-06-14-alerts-modelable-migration.md` | Phase 3.7 |
| `docs/superpowers/plans/2026-06-14-dashboards-modelable-migration.md` | Phase 3.8 |
| `docs/superpowers/plans/2026-06-14-incidents-modelable-migration.md` | Phase 3.6 |
| `docs/superpowers/plans/2026-06-14-notifications-modelable-migration.md` | Phase 3.3 |
| `docs/superpowers/plans/2026-06-14-slos-modelable-migration.md` | Phase 3.5 |
| `docs/superpowers/plans/2026-06-15-nlq-visualization-modelable-migration.md` | Phase 3.9 |

### 1b. Modelable migration specs → `archived/specs/`

Create `archived/specs/` and move all migration design docs:

- `docs/superpowers/specs/2026-06-12-tracing-attributes-json-type-design.md`
- `docs/superpowers/specs/2026-06-13-logs-modelable-migration-design.md`
- `docs/superpowers/specs/2026-06-13-metrics-modelable-migration-design.md`
- `docs/superpowers/specs/2026-06-13-tracing-typescript-field-case-design.md`
- `docs/superpowers/specs/2026-06-14-admin-members-modelable-migration-design.md`
- `docs/superpowers/specs/2026-06-14-alerts-modelable-migration-design.md`
- `docs/superpowers/specs/2026-06-14-dashboards-modelable-migration-design.md`
- `docs/superpowers/specs/2026-06-14-incidents-modelable-migration-design.md`
- `docs/superpowers/specs/2026-06-14-notifications-modelable-migration-design.md`
- `docs/superpowers/specs/2026-06-14-slos-modelable-migration-design.md`
- `docs/superpowers/specs/2026-06-15-nlq-visualization-modelable-migration-design.md`
- `docs/superpowers/specs/2026-06-15-phase4-modelable-cleanup-design.md`

### 1c. Duplicate removal

`docs/superpowers/plans/2026-05-06-identity-provider-zitadel.md` already exists in `archived/plans/`. Delete the `docs/superpowers/plans/` copy.

### 1d. Other completed non-modelable plans

For each plan below, read the file's checkbox status and move to `archived/plans/` (and its matching spec to `archived/specs/`) if all tasks are checked:

- `docs/superpowers/plans/2026-05-18-p5-s1-incident-timeline.md` (agent-context says COMPLETED 2026-05-18)
- `docs/superpowers/plans/2026-05-30-clickhouse-insert-efficiency.md` (agent-context: complete)
- `docs/superpowers/plans/2026-05-30-p4-s4-dashboard-rebac.md` (agent-context: complete)
- `docs/superpowers/plans/2026-05-31-context-preservation.md` (agent-context: complete)
- `docs/superpowers/plans/2026-05-31-live-tail.md` (agent-context: complete)
- `docs/superpowers/plans/2026-05-20-p5-s4-topology-impact-view.md` (P5-S4 code is in place per agent-context)
- `docs/superpowers/plans/2026-05-18-seed-generator.md` — verify checkboxes
- `docs/superpowers/plans/2026-05-19-p5-s3-runbook-attachment.md` — verify checkboxes
- `docs/superpowers/plans/2026-06-01-admin-console-member-management.md` — verify checkboxes

**Active plans — do not touch:**
- `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`
- `docs/superpowers/plans/2026-06-04-observability-feature-parity-plan.md`
- `docs/superpowers/plans/2026-06-10-p9-s5-service-catalog-health-signals.md`

---

## 2. Docs & Memory Updates

### 2a. `docs/agent-context.md`

- Update "Modelable Type-Mapping Migration" section header: change "Phase 3 complete, 2026-06-15" to "Phase 4 complete, 2026-06-15".
- In "Completed / archived detailed plans", update the `p5-s1-incident-timeline` entry (still points to `docs/superpowers/plans/`) to point at `archived/plans/`.
- Add entries for every plan moved in this PR to the "Completed / archived detailed plans" list.
- Remove any `docs/superpowers/plans/` references to plans being archived.

### 2b. Memory (`C:\Users\ktjn\.claude\projects\C--git-Observable\memory\`)

Update `project_modelable_migration.md`:
- Change "Phase 2 (tracing pilot) done as of 2026-06-13, Phase 3 (remaining domains) not started"
- To: "All four phases complete as of 2026-06-15. Master plan archived at `archived/plans/2026-06-08-modelable-type-mapping-migration-plan.md`."

---

## 3. Code: models/*.mdl Tidy + Dead-Code Scan

### 3a. `models/*.mdl` header comments

Add a one-line header comment to each `.mdl` file that currently lacks one, identifying the domain and where generated artifacts land. Format:

```
// Domain: <name>. Generated: libs/domain/src/generated/<domain>/ (Rust), apps/frontend/src/api/generated/<domain>/ (TypeScript).
```

### 3b. `binding ch-observable` duplication (Phase 1 backlog item 4)

Both `models/logs.mdl` and `models/tracing.mdl` declare:
```
binding ch-observable { adapter: clickhouse }
```
This causes a `UNIQUE constraint failed` error on a clean workspace compile. Options:
- If modelable supports a workspace-level `binding` declaration, extract it to `models/workspace.mdl`.
- Otherwise, add a `// NOTE: duplicate ch-observable binding — Phase 1 backlog item 4` comment above each declaration so the issue is visible rather than a silent trap.

Do not change the functional behavior of existing `.mdl` files; this is a documentation/comment-only change if workspace-level bindings aren't supported yet.

### 3c. Dead-code scan

Read-only scan; report findings, delete only what is provably unused:

1. `libs/domain/src/generated/` — confirm every directory corresponds to a current `.mdl` file.
2. `apps/frontend/src/api/generated/` — same check.
3. Grep each `apps/frontend/src/api/<domain>.ts` re-export file: confirm all re-exported generated types are imported at least once elsewhere in `apps/frontend/src/`.
4. If any generated type is never imported → flag for deletion (do not auto-delete without noting in the plan).

---

## Verification

This is a pure docs+code-comment change (no Rust/TypeScript compilation units modified). Per `AGENTS.md`:
- No `bash scripts/local-ci.sh` required (all changes are under `docs/`, `archived/`, `models/*.mdl` comments, and memory files).
- **Exception:** if `scripts/local-ci.sh` has a hard-gated modelable diff-check that fires on `.mdl` changes, run it for the `.mdl` comment additions in 3a/3b only.
- Git: create branch `chore/consolidate-modelable-migration`, commit, push, open PR.
