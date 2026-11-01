# AI System Development Constitution

Shared fundamentals for all work. Track-specific sequencing: [greenfield](GREENFIELD.md) (new systems), [brownfield](BROWNFIELD.md) (existing systems). Metamodel: [`spec.md`](spec.md).

This document defines shared obligations; tracks define sequencing. Tools may check parts of an artifact, but gate decisions require review of its meaning and evidence.

## Fundamental Principle

Implementations are not the source of truth. The source of truth is:

1. Capabilities
2. Business rules
3. Invariants
4. Data contracts
5. API contracts
6. Verification specifications
7. Formal models (where applicable)

Code is one possible implementation of a verified specification.

## Why

- AI quality degrades as context grows: requirement interpretation, architecture discipline, cross-component consistency and traceability suffer.
- Small components, explicit contracts and layering bound the context each task needs.
- Tests, not code, are the executable specification:

```text
Capability → Contract → Data model → Tests → Code
```

---

# Artifact Rules

## Capabilities

- Format: [`spec.md` § 4](spec.md#4-capabilities).
- `Purpose`, `Consumers`, `Producers` and `Business value` are discovery context, not fields.
- Acceptance criteria → `Requirement.criteria`; verification results → `Evidence`.
- As long as the contract holds, the implementation may change freely.

## Invariants

- A condition that must always hold, e.g. inventory never negative; an order paid at most once; customer email unique.
- All architectural decisions must preserve invariants.

## Components

- Derived from invariants and consistency requirements, not technology categories.
  - Bad: `Database`, `REST`, `Kafka` components.
  - Good: `Customer Management`, `Inventory Management`, `Payments`.
- Strong consistency inside a component; eventual consistency between components.

```yaml
components:
  - id: customer-management
    version: "1.0"
    layer: domain
    consistency: strong
    provides: [manage-customer]
    requires: [customer-repository]
    owns_data: [customer]
```

- Contracts and invariants belong to `capabilities`; requirements target capabilities, components or data models.
- No implementation details in component specifications.

## Data Contracts

- Every externally visible structure is explicitly defined.
- No implicit schemas; never infer or invent fields.
- `data_models` are authoritative; at implementation or version change, see skill [`modelable`](.github/skills/modelable/SKILL.md).

```yaml
Customer:
  id: UUID
  email: String
  status: [ACTIVE, BLOCKED]
```

## API Contracts

Every API defines: endpoint, version, request, response, failure conditions, status codes, idempotency, security. All APIs comply with organizational API guidelines.

## Verification

No implementation is accepted until all categories are addressed:

| Category | Covers | `Requirement.kind` | `Evidence.method` |
|---|---|---|---|
| Functional | Expected behaviors | `functional` | `test` |
| Semantic | Business meaning (e.g. `BLOCKED` denies purchase) | `functional` | `test`, `model-check` or `proof` |
| Contract | API compatibility, status codes, versioning | `functional` | `test` or `inspection` |
| Serialization | JSON/XML/protobuf correctness, payload limits | `functional` | `test` |
| Concurrency | Races, lost updates, duplicate requests | `functional` | `test`, `model-check` or `proof` |
| Performance | Latency, throughput, resource limits | `non-functional` | `test` or `analysis` |
| Scalability | Expected load and data volumes | `non-functional` | `test` or `analysis` |
| Resilience | Timeouts, retries, partial failures | `non-functional` | `test` or `analysis` |
| Security | Authentication, authorization, input validation | `non-functional` | `test` or `inspection` |
| Observability | Metrics, tracing, logging, correlation IDs | `non-functional` | `test` or `inspection` |

## Formal Verification

- Assess per component. Indicators: concurrency, distributed state, financial transactions, safety-critical logic, complex state machines, protocols.
- If applicable, produce TLA+/Alloy models or state machines; see skills [`tla`](.github/skills/tla/SKILL.md) (concurrency, protocols) and [`alloy`](.github/skills/alloy/SKILL.md) (structure, relations).
- Record bounded checks as `method: model-check` with explicit `bounds` and unbounded proofs as `method: proof` ([`spec.md` § 8](spec.md#8-verification)).

## Layering

```text
Experience → Application → Domain → Infrastructure
```

- Higher layers may depend on lower layers, never the reverse.
- Domain logic must not depend on infrastructure details.

## Context Budget

- A component's complete specification fits on 1–2 pages (defaults: `max_provides: 5`, `max_requires: 8`, `max_owns_data: 4`); otherwise split it.
- Each AI task receives only the minimum necessary context: these fundamentals, one track document, the applicable skill (if any) and the target component's spec slice (`metamodel.py <spec> --context <component-id>`).

### Implementation Handoff

For each bounded change, give the implementer:

- The capability and expected outcome; accepted contract and data-model versions; affected invariants.
- For local multi-step APIs, the state transitions, confirmation/freshness obligations, committed writes and recovery for each failure outcome. An exception after commit does not imply rollback.
- The component boundary, permitted dependencies and relevant consumer/provider contracts.
- Requirement IDs, measurable acceptance criteria, verification procedures and expected failure cases.
- Applicable formal-model conclusions, assumptions, open risks and active waivers.
- Allowed change scope and the evidence to return. Include neighboring contracts without importing unrelated implementation context.

Resolve missing behavior or contradictory contracts before dependent implementation. If the packet cannot fit the context budget, split the work or revisit the component boundary. The implementer returns the change, requirement-linked results, affected revisions and any unmet criteria; it does not silently redefine the contract to fit the code.

## Traceability

```text
Capability → Requirement → Invariant → Component → Contract → Test → Implementation
```

- Every test traces to a requirement; every implementation element traces to a capability.

## Acceptance

A system is complete only when capabilities, invariants, components, contracts, data models and APIs are specified; verification is complete; formal models are reviewed; performance and concurrency goals are met; traceability is established; and architecture constraints are preserved.

Implementation completion, verification completion and release acceptance are separate states. Record each only when its obligations have been met.

### Gate Decision Record

Keep a short decision record alongside the specification; no particular tool or file format is required:

| Record | Content |
|---|---|
| Scope | Gate/level, capability or change, and exact artifact revisions reviewed |
| Adoption boundary | Whole product or bounded slice; existing authorities/tool map, repository root, validator/schema pin, native gates and external/deferred/unknown units |
| Responsibility | Who prepared the artifacts, reviewed the evidence and owns the acceptance decision; one person may hold several roles |
| Findings | Criteria checked, supporting artifacts, unresolved questions and affected consumers |
| Outcome | Pass, rework, or defer within an allowed adoption-level waiver; identify the requirements/risks that justify any deferral |
| Follow-up | Remediation owner, expiry, next action and which gate must be revisited |

- **Pass** permits the next step for the recorded scope. A passing schema or validator result is supporting evidence only.
- **Rework** keeps the affected gate open. Repair the earliest invalid assumption or artifact, then review dependent outputs again.
- **Defer** preserves visible debt through an allowed, bounded waiver. It does not mark the waived property verified. Release permits no waiver deferrals.

Review criteria for meaning, not merely presence: failure behavior, measurable thresholds, consumer obligations and formal-model assumptions must match the intended system. Inspect the linked execution artifacts before accepting a claimed result.

Boundary allowlists or transitional dependency waivers (e.g., `component-boundary-allowlist.txt`) are permitted only below Level 3 (`release`) and must follow a monotonic ratcheting rule (entries may only decrease or stay equal; CI fails on additions) until completely zeroed. Multi-schema relational databases must enforce explicit schema namespace ownership and schema-scoped user roles, with zero cross-schema foreign keys.

Use the [brownfield intake template](templates/BROWNFIELD-INTAKE.md) for existing systems. Separate observed classification from provisional handling policy; retain skips and missing provider/data mappings as open obligations. Contract/source bindings and local file hashes support review but do not replace source authenticity, build correspondence or native acceptance gates.

### Change and Reverification Loop

1. Identify the requested behavior change and the last reviewed baseline. Trace affected capabilities, components, data models, consumers and requirements, including transitive dependencies (`--impact <id>`).
2. Decide whether the contract stays compatible (`--compat`). If it breaks, define consumer migration and recovery before implementation; a version bump alone does not establish compatibility.
3. Reopen the earliest affected gate. Update its artifacts and verification criteria, then issue a bounded implementation handoff.
4. Treat evidence affected by changed requirements, subjects or dependencies as historical until rerun. Record requirement version, subject revision and the relevant dependency versions with the new results; preserve earlier failures and justify any supersession.
5. Review new results against all affected obligations, including risk mitigations and consumer behavior. Record the gate decision and remaining work.

### Self-Improvement and Model Feedback

When discovery, implementation, verification, formal modeling or incident review reveals new facts (undocumented failure modes, hidden coupling, violated assumptions, unmodeled domain constraints or gaps in the metamodel/process itself):

1. **Surface the finding immediately** in the handoff return or [gate decision record](#gate-decision-record); never mask a missing contract clause, invariant or dependency with an ad-hoc code workaround.
2. **Update the system model**: feed discovered facts back into `capabilities`, `components`, `data_models`, `requirements`, `risks` or formal models, run `--impact`, and reopen the earliest affected gate.
3. **Improve the metamodel and tooling**: when a finding exposes an uncheckable rule or recurring blind spot in [`spec.md`](spec.md), track workflows or [`metamodel.py`](reference-implementation/metamodel.py), propose a minimal update to `spec.md` § 11, `INVARIANTS`, `schema.json` and `corpus.yaml`.

---

# Metamodel Mapping

| Artifact | Metamodel (`spec.md`) |
|---|---|
| Capabilities | `capabilities` (`version`, `preconditions`, `postconditions`, `invariants`, `failure_modes`, `status`) |
| Invariants | `capabilities.invariants` |
| Components | `components` (`provides`, `requires`, `owns_data`, `status`, `source_paths`) |
| Data contracts | `data_models` (`schema_path`, `declassified_by`) |
| API contracts | Capability contracts, `requirements` and `data_models` (`capabilities.interface`) |
| Verification | `requirements` (`status`, `category`, `resolved_by`), `evidence` (`subject_revision`, `supersedes`) |
| Formal verification | `evidence.method: model-check` (with `bounds`) or `proof`; formal models in `provenance` |
| Layering | `system.layers`, `components.layer` |
| Context budget | `system.context_budget` (`max_provides`, `max_requires`, `max_owns_data`); per-component override `components.context_budget` |
| Waivers | `risks` (`mitigated_by`, `waives`, `expires`) |
| Traceability | Capability, component, requirement, data model and evidence references |
| Acceptance | `metamodel.py <spec> --stage release` (see [`spec.md` § 11](spec.md#11-metamodel-validation)) |
