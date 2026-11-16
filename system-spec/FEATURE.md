# Feature: Changing an Already-Specified System

Contract-first sequencing for adding or changing behavior in a system that already has a validated spec (built via [greenfield](GREENFIELD.md) or onboarded via [brownfield](BROWNFIELD.md) at Level 1+). Artifact rules: [constitution](CONSTITUTION.md). Load this document together with the constitution, the applicable skill and one component slice only.

## Specification authority

For a previously accepted scope, define the intended behavioral delta against the reviewed specification before implementation. Existing code is not authority for silently extending contracts. A change that reveals undocumented behavior requires an explicit decision to preserve, reject or leave it unconstrained. If the aim is to replace code without changing expected behavior, use [REPLACEMENT.md](REPLACEMENT.md).

## Isolation-aware changes

When [component isolation](COMPONENT-ISOLATION.md) is adopted, update the provider's published programmable mock and its contract tests as part of any changed capability. Verify consumers against the mock and the provider independently before the integration join. Changing a mock to match a defective implementation does not amend the accepted contract.

## Feature lifecycle

Each feature is a first-class *conceptual* change record, separate from its requirements. Preserve its original request (a word, a paragraph or a thousand-line specification), decisions, decomposition, verification plans, approval baseline and final acceptance. The current reference format does not require a dedicated Feature object.

### Phase 1 — Specification (before implementation)

| # | Step | Output |
|---|---|---|
| 1 | Request and baseline | Original requester input, requested outcome and existing system baseline |
| 2 | Interpret and scope | Assumptions, ambiguities, positive and prohibited behavior, changed capabilities and affected components |
| 3 | Decompose | Recursive feature requirements and sub-requirements, component allocation and parent-level integration obligations |
| 4 | Specify verification | Precondition, input, expected result, negative cases, boundaries and verification method for every requirement; executable tests where feasible |
| 5 | Completeness review | Traceability and gap report across functional, failure, state, concurrency, security, compatibility, quality and integration behavior |
| 6 | Review and iterate | Requester decisions, revised requirements and test specifications, incrementally approved groups if useful |
| 7 | Approve baseline | Requester-approved versioned feature-wide requirements and verification baseline |

**Completeness review:** Identify requirements with no verification, orphan tests, ambiguous or unmeasurable criteria, conflicting obligations, unassigned component ownership, unresolved assumptions, missing integration checks and uncovered failure behavior. Present findings for requester resolution; tool reports inform but never replace approval.

**Specification approval gate:** No implementation until the requester or explicitly delegated authority approves the applicable *complete feature baseline*. Incremental approvals for requirement groups or component specifications streamline review, but do not authorize implementation of an incomplete feature. An explicitly separated, independently approved feature scope can be implemented on its own. Record approver, scope, revision, verification specifications, assumptions and outstanding decisions. Approval is of required behavior and expected verification, not a test runner, code structure or passing result.

### Phase 2 — Implementation (after approval)

| # | Step | Output |
|---|---|---|
| 8 | Prepare executable checks | Generate or finalize implementation-specific tests from the approved verification specifications |
| 9 | Plan and implement | Dependency-aware component handoffs; changes within approved contracts and ownership |
| 10 | Verify and correct | Execute tests, collect evidence, repair defects and verify parent-level composition |
| 11 | Review discoveries | Classify defects versus missing/incorrect requirements or tests, changed constraints or new risks |
| 12 | Accept | Final evidence and requester acceptance for the implemented feature |

**Feedback loop:** Fix implementation defects without rewriting the specification. A missing or incorrect requirement, materially changed test expectation, new architectural constraint or significant risk reopens the affected specification and its approval before implementation of that changed scope continues. Preserve unaffected approvals when their referenced content and dependencies are unchanged. Never weaken tests just to make implementation pass. Technical verification and final requester acceptance are separate.

### Optional reference-tool checks

- Scope and compatibility: `--impact`, `--compat`, `--plan`.
- Specification and verification: `--stage architecture`, `--stage verification`, `--check-report`.
- Handoff and implementation: `--context`, `--check-scope`, `--check-return`.
- Evidence and acceptance review: `--emit-evidence`, `--evidence-report`, `--stage release`.

These tools do not replace requester approval. Projects select their own review, test, approval and enforcement mechanisms. Test specifications are independent of executable test implementations.

### Implementation rules for this optional methodology

- Each leaf requirement targets one component or capability it provides where practical. Parent requirements retain their own integration and end-to-end obligations; sub-requirements do not substitute for composition checks (`REQ-08`).
- Bind each test to the requirement and criteria it verifies. Under the reference workflow, use `verifies` and `--check-report`; derive evidence from executed reports.
- Define and review tests before implementation. Implementation may begin only against an approved baseline; do not treat a failing preimplementation test as successful verification evidence.
- When component isolation is adopted, verify consumer isolation implementations against provider contracts and verify the integration join.
- Use dependency-aware, bounded handoffs; preserve source ownership, relevant contracts and evidence provenance.
- Reopen the affected approval gate when approved behavior or test expectations change. Preserve failed and superseded evidence.
- Report hidden coupling, ambiguous intent and metamodel gaps as feedback instead of silently changing the contract.

## Domain-oriented subagent workflow

For a feature affecting more than one logical domain, the [domain subagent coordination methodology](DOMAIN-AGENTS.md) is required within this methodology: assign an accountable agent per affected domain and at least one coordinator. A single-domain feature stays optional. Resolve shared schema and contract dependencies before dispatching affected consumer implementations, with staged migrations for breaking changes. The complete feature-wide specification approval gate remains authoritative.

## Decompose and hand off

Order: requester-approved requirements, contracts and tests first; then independent implementation per component and integration verification. The toolkit is tool-agnostic: the "implementer" below is whoever receives a slice (a person, a subagent or a separate session).

1. **Write the feature requirement** against the capability during Phase 1, before any implementation.
2. **Split it** into sub-requirements that `refines` it, each targeting exactly one component (or a capability it provides). Each must be buildable and verifiable alone, against fakes.
3. **Approve and plan.** Obtain requester approval of the complete feature requirements and verification baseline, then use reference tools to order the work. `--plan <id>` prints waves, providers first. Components in the same wave are independent and can be implemented in parallel; a later wave starts when the providers it `requires` are verified.
4. **Hand off** one component per implementer. The bundle is: the `--context <component-id>` slice, its sub-requirement, the fakes it uses, the allowed paths (`source_paths`/`test_paths`), and the [handoff return](reference-implementation/handoff-return.schema.json) to fill in.
5. **Check each return**: `--check-scope <component-id> --changed -` (nothing outside its paths) and `--check-return <file>` (every requirement of the component is a result or unmet).
6. **Join.** Record each `results` entry as evidence in the component's fragment. The parent requirement then needs its own integration evidence (`test_scope: integration` component), and every sub-requirement verified (`REQ-08`, since 0.5.0, unreleased). A failed join supersedes evidence and iterates; it does not relabel.

Illustration (not in the shipped example spec): feature "place an order reserves stock" is `req-order-reserves` on `commerce-system`. It is refined by `req-reserve` (target `reserve-inventory`, component `inventory`, wave 1) and `req-order` (target `place-order`, component `checkout`, wave 2, uses the `reserve_inventory` fake). Inventory and checkout implementers work from separate slices; the integration component proves `req-order-reserves` once both are verified and the provider's contract test keeps the fake honest.

Not specified: how an implementer is run (subagent, session, human), or how many run at once; that is project policy.

## Gate Review

| Gate | Bring to review | Decision required before proceeding |
|---|---|---|
| Scope | Requested behavior, `--impact` result, target components and order | The change is bounded; affected consumers and evidence are known |
| Contract | Spec diff, `--compat` result, migration/deprecation plan | Compatible, or every consumer has a migration with recovery; versions follow the compatibility result |
| Specification approval | Original feature request, recursive requirements, verification specifications, completeness findings, incremental review decisions and full baseline revision | Requester approves the entire applicable feature baseline before implementation; otherwise revise |
| Implementation acceptance | Requirement-linked results for the approved revision, integration checks, risk dispositions | Technical obligations satisfied and final acceptance recorded separately from specification approval |

## Prompts

- **Scope**: "Given this request and the current spec, name the capability delta and run `--impact` on each affected ID; list components, consumers and evidence invalidated. No design yet."
- **Contract**: "Propose the spec change (capabilities, interface, data models, versions) for `<component>`; show the `--compat` result against the old spec and any migration needed. Do not invent fields."
- **Verification**: "Write requirements and acceptance criteria for the changed obligations, the fake for each newly required capability, and the provider contract tests that keep the fake honest."
- **Specification review**: "Present the original request, recursive requirements, component allocation, verification cases and completeness gaps. Iterate with the requester; preserve incremental approvals but obtain approval of the full applicable feature baseline before implementation."
- **Implement**: "Use only the requester-approved baseline and its tests. Implement this `--context <component-id>` slice within `source_paths`; return requirement-linked results, unmet criteria and any changes requiring reapproval."

Example of the component layout (source, tests, fakes, schema, checks): [`examples/components/`](examples/components/README.md).

## Using with OpenSpec

[OpenSpec](https://github.com/Fission-AI/OpenSpec) (Fission AI) is a prose spec-and-change workflow for AI assistants: proposal, design notes, tasks, spec deltas (ADDED/MODIFIED/REMOVED), archive. It is a process layer; this model is the checked layer underneath. Neither requires the other, and the toolkit has no OpenSpec importer or exporter.

| OpenSpec | Here |
|---|---|
| Proposal, design notes | Scope and Contract steps, gate reviews |
| Spec delta | `--compat OLD.yaml` and `--impact <id>` |
| Task list | `--plan <id>` waves; one `--context <component>` slice per handoff |
| Archive | Version bumps, `supersedes` evidence history, [gate decision record](CONSTITUTION.md#gate-decision-record) |
| `openspec validate` | `--stage <stage>`, `--check-report` |

- Keep proposals and tasks in OpenSpec if the team uses it; the change is not accepted until the spec passes the Acceptance gate above. Archiving a change is not acceptance.
- Prose requirements and scenarios carry no contracts, components, data models or evidence: express them as `Requirement.criteria` and the model entities before relying on them.
- Treat the spec delta as a review aid; `--compat` is the check.
