# Agent Context

This file is the living starting map for agents working in this repository. It does not replace
reading the code. Every implementation task still requires inspecting the relevant files before
making changes.

## Required Startup Path

1. Read `AGENTS.md`.
2. Read `spec/adr/README.md`, then read any overlapping ADRs in full.
3. Read the active plan in `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`.
4. Read this file.
5. Inspect the actual code, tests, scripts, specs, and docs touched by the task before editing.
6. Create or switch to a dedicated short-lived branch before changing files.

## Current Source Of Truth

- Repository process: `AGENTS.md` and `spec/10-process.md`.
- Active roadmap: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`.
- Historical Phase 1 plan: `archived/plans/2026-04-17-phase1-internal-mvp.md`; do not treat it as an active backlog.
- Architecture decisions: `spec/adr/`.
- Product and platform specs: `spec/`.

## Codebase Map

- `apps/frontend/`: React 19 + Vite frontend.
- `apps/frontend/src/components/`: shared reusable frontend components.
- `apps/frontend/src/features/**/components/`: feature-scoped frontend components.
- `services/`: Rust platform services.
- `libs/`: shared Rust libraries.
- `contracts/` and `proto/`: API and protobuf contracts.
- `migrations/`: versioned database migrations.
- `charts/`: Helm deployment assets.
- `scripts/`: local CI, smoke, migration, and operational scripts.
- `tests/`: cross-cutting test suites and end-to-end checks.

## Standing Constraints

- Never commit or merge directly to `main` without human review.
- Every implementation iteration needs a short-lived branch, commit, push, and pull request.
- Pure documentation changes are exempt from `bash scripts/local-ci.sh`; code changes are not.
- Backend changes crossing PostgreSQL, ClickHouse, Redpanda/Kafka-compatible brokers, object
  storage, OpenFGA, or similar real dependency boundaries need the narrowest applicable
  Testcontainers integration test unless the PR explains why a different regression signal applies.
- Frontend work must reuse existing shared or feature components before adding new ones.
- ADRs and specs must be updated together when architecture, technology choices, deployment model,
  data model, security model, or roadmap scope changes.

## Keep This File Updated

Update this file in the same PR when a change affects future agent orientation, including:

- repo layout or ownership boundaries;
- active roadmap or plan selection;
- required verification commands or exemptions;
- architectural assumptions, deployment assumptions, or dependency-boundary rules;
- important gotchas discovered while implementing or verifying a slice.

If a change does not affect future agent guidance, state that in the PR description instead of
editing this file.
