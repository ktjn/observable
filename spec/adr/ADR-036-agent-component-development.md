# ADR-036: Build-Time Agent Component Development

**Status:** Proposed
**Date:** 2026-10-16

## Context

Observable's 0.2 roadmap and ADR-035 define independently buildable and deployable runtime components. AI development agents need smaller, verifiable assignments with explicit source ownership, dependency awareness and controlled parallelism. Existing AGENTS.md requires short-lived branches, human-reviewed PRs, protected regression gates and local validation.

## Decision

Introduce an opt-in **build-time-only** agent development layer using logical component specifications, explicit source ownership, capability contracts, requirement traceability, immutable work packages, dependency-aware scheduling and independent verification. Reuse native build tools and existing local CI. The agent is a replaceable executor, never the verifier or merge authority.

The main specification is [spec/20-agent-component-development.md](../20-agent-component-development.md); delivery sequencing is [docs/plans/agent-component-development.md](../../docs/plans/agent-component-development.md).

## Consequences

Benefits: smaller agent contexts, parallel independent work, deterministic acceptance, traceability and recoverable agent runs.

Costs: ownership inventory, new schemas/CLI, sandboxing, verification overhead and contract maintenance.

## Non-decisions

No runtime component registry, service topology change, automatic main merge, replacement for ADR-035, or mandatory agent vendor.

## Alternatives

- Agent-only prompts: insufficient enforcement and evidence.
- Runtime orchestration: wrong lifecycle and unnecessary production complexity.
- Replace native build systems: excessive migration and risk.
- Immediate repository split: conflicts with ADR-035 sequencing.

## Formal verification decision

Use **Alloy** to validate bounded structural properties of component ownership, capability resolution, dependency graphs and requirement/evidence relationships. Use **TLA+/TLC** to explore temporal properties of agent scheduling, leases, failure/retry, stale verification and integration. Models are development-time artifacts only; deterministic runtime-of-the-CLI validators and independent executable tests remain the enforcement mechanisms.

Keep bounded-model counterexamples mapped to executable regression fixtures and document model scope and fairness assumptions. This extends the existing formal-verification approach in `spec/19-formal-verification.md` without changing Observable's production runtime architecture. See section 17 of `spec/20-agent-component-development.md` and the formal workstream in the implementation plan.
