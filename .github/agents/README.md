# Advisory Role-Based Agent Operating Model

This directory defines the coordinator-plus-specialists agent model for the Observable repository.
The model is **advisory-first**: roles reduce context noise and improve governance consistency without
hard tool restrictions or mandatory role locking.

## Why this model?

The Observable repo spans multiple concerns (backend Rust services, frontend, Helm/infra, spec/ADRs,
iteration plans) and carries substantial governance context in AGENTS.md, spec/10-process.md, and the
ADR library. When a single agent session ingests everything it needs for a documentation review *and*
a Rust service change at the same time, context degrades. Specialist agents solve this by loading only
the slice of context relevant to their domain.

## Roles at a Glance

| Role | Agent file | Invocation | Primary surface |
|------|-----------|------------|-----------------|
| **Coordinator** | `coordinator.agent.md` | User-facing; entry point | Cross-domain orchestration |
| **Spec & Docs Steward** | `spec-steward.agent.md` | Subagent (read-only) | `spec/`, `docs/superpowers/` |
| **Architecture Steward** | `architecture-steward.agent.md` | Subagent (read-only) | `spec/adr/`, architecture sections of specs |
| **Planning Steward** | `planning-steward.agent.md` | Subagent (read-only) | `docs/superpowers/plans/` |
| **Implementation Steward** | `implementation-steward.agent.md` | Subagent (code) | `services/`, `apps/`, `libs/`, `migrations/`, `tests/` |

## Coordinator Routing Rules

The coordinator decides which specialist(s) to invoke based on what surfaces the task touches:

| Task touches… | Invoke |
|---------------|--------|
| `spec/`, `docs/superpowers/`, AGENTS.md, CLAUDE.md | spec-steward |
| `.github/agents/` (role charters, routing rules, escalation triggers) | spec-steward |
| `spec/adr/`, architecture/deployment/data-model/security decisions | architecture-steward |
| `docs/superpowers/plans/` or phase/sequencing questions | planning-steward |
| `services/`, `apps/`, `libs/`, `migrations/`, `tests/` | implementation-steward |
| Multiple surfaces | Multiple specialists in sequence; coordinator reconciles |

## Escalation and Handoff Triggers

- **Architecture change** (deployment model, data model, security model, API contract, technology choice):
  coordinator must invoke architecture-steward *before* implementation-steward writes any code.
  architecture-steward confirms ADR coverage; if a new ADR is needed, coordinator pauses and creates it.
- **Spec/doc change** (any file under `spec/` or `docs/superpowers/`):
  coordinator invokes spec-steward to review consistency and cross-reference accuracy before commit.
- **Plan change** (adding, resequencing, or closing slices in `docs/superpowers/plans/`):
  coordinator invokes planning-steward to confirm alignment with active Phases 2–8 plan.
- **Pure implementation** (Rust/frontend/migration code, no doc changes):
  coordinator delegates directly to implementation-steward; no specialist review required unless the
  change introduces a new architectural boundary (see Architecture change above).

## Non-Goals (Advisory Pilot — Phase 1 of the Agent Model)

- No hard tool deny-lists or allow-lists enforced.
- No mandatory role declaration on every PR.
- No process rewrites beyond this role-model introduction.
- Soft enforcement (PR labels, checklist gates) is deferred to a follow-up iteration after a pilot.

**Note:** "Phase 1" here refers to the first phase of the agent-model rollout (the advisory pilot),
not the closed project Phase 1 described in AGENTS.md.

## Pilot Metrics

During the advisory pilot (next 2–3 implementation iterations) track:

1. **Context efficiency** — does each specialist invocation load only its bounded context set?
2. **Governance misses** — are mandatory AGENTS.md checks (CI gate, ADR sync, Testcontainers) still
   caught before PR? Any regression here is a blocking signal to adjust routing rules.
3. **Review ownership clarity** — can reviewers identify who is responsible for each surface in the PR?

After the pilot, reassess routing rules and optionally move to soft enforcement.

## Changing the Model

Changes to this directory (role charters, routing rules, escalation triggers) touch the governance
process. They must be reviewed by a human maintainer and updated in the same iteration as any
related changes to AGENTS.md, CLAUDE.md, or `spec/10-process.md`.

**Maintenance note — mandate duplication:** Specialist agent files intentionally repeat key mandates
from AGENTS.md (e.g. local-ci.sh, Testcontainers) so each file is a self-contained context pack.
This means **changes to those mandates in AGENTS.md must also be reflected in any affected specialist
files** in the same iteration. Check `implementation-steward.agent.md` for CI/Testcontainers rules
and the other steward files for any overlapping governance text whenever AGENTS.md is updated.
