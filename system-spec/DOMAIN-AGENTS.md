# Domain-oriented subagent coordination

**Optional development methodology.** Extends [FEATURE.md](FEATURE.md), [BROWNFIELD.md](BROWNFIELD.md) and [GREENFIELD.md](GREENFIELD.md) without changing the metamodel's authority boundaries in [MODEL-SCOPE.md](MODEL-SCOPE.md). An agent may be a person, process, session or AI subagent; neither agent runtimes nor orchestration frameworks are prescribed.

Within this methodology, this coordination is required whenever a feature, greenfield build or brownfield discovery scope spans more than one logical domain: assign an accountable agent per affected domain and at least one coordinator. A single-domain scope stays optional — a lone domain agent can coordinate its own work without this workflow. Consuming projects decide enforcement mechanics (tooling, gating, automation), as for any other methodology here.

## Roles and ownership

- **Domain agent:** one accountable agent per affected logical domain, with optional subordinate component agents. Owns domain-scoped discovery, requirement decomposition, verification cases, bounded implementation and evidence. Must not modify another domain's owned artifacts without an explicit handoff.
- **Coordinator:** for any multi-domain scope, assign at least one coordinator to own the cross-domain plan, dependency graph (`system.domains` and `uses`), integration obligations, approval consistency and final evidence aggregation. Multiple coordinators may partition subgraphs, but one designated lead resolves shared dependencies and maintains the overall status.
- **Schema/contract owner:** the authoritative owner of a changed or discovered shared data model, interface or contract. A coordinator does not silently assume ownership of another domain's schema.
- **Requester / acceptance authority:** approves the complete scope-wide requirement and verification baseline before implementation (or the specification-authority / adoption-level decision in [BROWNFIELD.md](BROWNFIELD.md)). Coordinator approval is not a substitute.

Domains and architectural layers are orthogonal. Domain boundaries are logical, not necessarily repository, deployment or runtime boundaries. Agents may delegate components, but each deliverable has one accountable owner.

## Execution lifecycle

1. **Discover and scope:** identify affected capabilities, domains, components, shared data models, contracts, risks and impacted consumers. Record a dependency graph and owners. In brownfield reverse discovery ([BROWNFIELD.md](BROWNFIELD.md#reverse-discovery)), each domain agent characterizes its domain's observed boundaries, data models and invariants while the coordinator reconciles cross-domain `requires`, `via` and `uses` edges and flags architecture waivers.
2. **Specify:** domain agents independently propose domain requirements, subrequirements and verification cases (or observed contracts and characterization tests in brownfield); coordinators reconcile cross-domain behavior, conflicting assumptions, interface expectations, migrations and parent-level integration tests.
3. **Approve:** obtain requester approval of the *complete* applicable scope baseline (feature baseline in [FEATURE.md](FEATURE.md), verification gate in [GREENFIELD.md](GREENFIELD.md), or adoption-level / specification-authority gate in [BROWNFIELD.md](BROWNFIELD.md)), including schema evolution and rollback expectations. No implementation or refactoring before this gate.
4. **Resolve schema and contracts first:** schedule shared schema, data model and interface contract work as predecessor tasks for all affected consumers. Verify compatibility, migration strategy, versioning, fixtures, programmable mocks and contract tests before dependent implementation starts. Where needed, use expand–migrate–contract stages, retaining old and new representations until consumers are ready. Destructive changes require explicit cutover and recovery criteria.
5. **Dispatch domain work:** after each domain's actual predecessors are verified, issue bounded domain handoffs. Domain agents can parallelize independent component work using provider-first waves. Do not globally block unaffected domains on unrelated schema changes.
6. **Verify domain outputs:** require requirement-linked evidence, contract conformance, scope checks and an explicit unmet/blocked report from each domain. Changes to accepted behavior or verification expectations reopen the affected approval scope.
7. **Integrate:** coordinators run cross-domain compatibility, migration, integration, security and failure-path verification. Preserve domain evidence provenance and distinguish domain success from end-to-end success.
8. **Accept:** reconcile all domain results, unresolved risks, rollback readiness and parent-level evidence; present them for final requester acceptance.

## Dependency and concurrency rules

- Model tasks as a DAG. Edges express *verified prerequisites*, not merely implementation preferences.
- Shared schema/contract changes precede consumers that rely on them; independent schema changes can execute in parallel.
- Schema-first does not mean physical database migrations must always be deployed first. For incompatible changes, enforce a safe staged rollout (e.g. expand schema, deploy compatible producers/consumers, migrate data, contract schema) with explicit checkpoints.
- Assign exactly one writer/owner for each shared artifact per execution wave; competing edits require coordination, not last-writer-wins merges.
- Parallelize only when ownership and dependency boundaries permit it. Detect cycles and resolve them through contract design, staged interfaces or an explicit joint task before dispatch.
- A coordinator must not silently rewrite approved domain requirements or change schema semantics. Material discoveries return to specification review.
- A blocked domain reports the unmet predecessor and waits; other independent domains may continue.

## Handoff and return contract

Each domain handoff should contain: feature baseline revision; domain and accountable agent; affected component IDs; approved requirements and test cases; owned paths; required/provided contracts; schema versions and migration stages; predecessor IDs; allowed parallel work; integration obligations; expected evidence and return format.

Each return should include: baseline revision; implemented requirement IDs; changed artifacts; verification results and provenance; contract/schema versions exercised; migration/rollback status; unmet criteria; discovered risks; cross-domain issues; and approval changes requested.

The coordinator maintains a ledger of tasks and states (`pending`, `blocked`, `ready`, `running`, `verified`, `failed`, `accepted`) and records the evidence and gate decision behind transitions. This is a conceptual workflow, not a mandatory serialized model entity.

## Acceptance criteria

1. A change or discovery scope touching multiple domains has an explicit coordinator and an accountable agent for each affected domain.
2. All domain requirements, characterization cases and cross-domain integration cases trace to the approved baseline.
3. Shared schema and contract dependencies are identified and verified before affected consumer tasks begin.
4. Compatible staged migration and recovery behavior are specified for breaking changes; no destructive cutover precedes consumer readiness.
5. Independent domains can execute concurrently without conflicting artifact ownership.
6. Domain agents return requirement-linked evidence and blockers; coordinators independently verify integration and aggregate evidence.
7. A material contract or requirement change reopens the affected approval gate; unaffected approvals remain valid.
8. No agent's success report alone is treated as final requester acceptance.

## Reference-tool mapping

Use `--impact` for affected entities, `--plan` for provider-first component waves, `--context` for bounded component work, `--compat` for schema/contract compatibility, `--check-scope` and `--check-return` for handoff checks, and `--evidence-report` for aggregation. These tools do **not** implement a domain scheduler, coordinator agent, schema migration engine or independent integration verifier. Consuming projects decide whether and how to automate this workflow.
