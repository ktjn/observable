# Verified Development: Specification

Status: **proposal**. This document defines optional methods and reference-tool extensions, not additional mandatory metamodel invariants. The [metamodel](spec.md), [authority boundaries](MODEL-SCOPE.md) and consuming project's acceptance policy remain authoritative. Implementation languages, agents, CI tools and proof engines are interchangeable.

## 1. Objective

Maintain a reviewable behavioral specification independent of any implementation. Permit an agent or engineer to develop, replace and verify a bounded component without access to its predecessor's source code, provided the new implementation satisfies accepted contracts, assumptions and verification obligations.

The target outcome is:

```text
Intent -> Accepted specification -> Bounded component build -> Independent verification
                       |                                |
                 Proposed changes                 Evidence and trust report
                       |                                |
                   Review/merge <---- Acceptance/defect classification
```

Do not conflate a passing reference-validator result, passing tests, mutation score, a proof under assumptions, and system-wide conformance.

## 2. Existing baseline and boundaries

Reuse the current `System`, `Component`, `Capability`, `Requirement`, `Risk`, `Evidence`, `subject_bindings` and `SpecImport` concepts. Reuse `--impact`, `--plan`, `--compat`, `--context`, `--evidence-report`, `--check-report`, `--check-isolation` and the existing greenfield, brownfield, feature and replacement workflows.

- A **change set** is a proposed, reviewable change to these existing entities; it is not a second authoritative specification.
- A **contract adapter** connects an accepted capability to a language- or protocol-specific verification artifact; it never silently defines new behavioral requirements.
- An **assurance report** evaluates the scope, trust assumptions and sufficiency of evidence. It is not itself proof of completeness.
- A **replacement experiment** tests whether the accepted specification is sufficiently informative to independently reconstruct the behavior of a component.
- External tools (e.g., OpenAPI, AsyncAPI, Smithy, Pact, Specmatic, Microcks, TLA+, Alloy, Verus, mutation tools) are optional integrations. The project selects them.

Non-goals: a new agent IDE or general-purpose orchestration runtime; mandatory formal verification; a universal API protocol; replacing established contract-testing systems; automatically inferring correct requirements from legacy code; requiring independent physical deployment of every component.

## 3. Requirements

### 3.1 Change sets (CHG)

| ID | Requirement | Acceptance condition |
|---|---|---|
| CHG-01 | Proposed changes live separately from the accepted baseline. | The active spec remains unchanged until a reviewed acceptance/apply action. |
| CHG-02 | Every change is bound to the exact baseline revision and intended outcome. | Missing/stale revision and ambiguous affected IDs fail validation. |
| CHG-03 | Changes carry entity-level additions, modifications, deprecations and removals. | A deterministic semantic delta is produced for supported entity types. |
| CHG-04 | Impact and compatibility are derived from existing model semantics. | Impact includes affected capabilities, dependents, requirements, risks, mocks and evidence to invalidate; existing compatibility policy is reused. |
| CHG-05 | Material ambiguities, decisions, risk changes and verification obligations are reviewed before acceptance. | The decision record identifies approver, decision, rationale, unresolved questions and allowed exceptions. |
| CHG-06 | Applying an accepted change is atomic, repeatable and detects conflicts. | Stale bases fail closed; re-apply is idempotent or reports the exact prior application; no partial writes. |
| CHG-07 | Accepted changes retain provenance and reversibility information. | Audit history includes baseline/target revisions and changed entity IDs; reversal is another reviewed change, not hidden rewriting. |

A change can also propose a replacement with **no intended behavioral delta**. This must still identify implementation revisions and required fresh evidence.

### 3.2 Executable capability contracts (CTR)

| ID | Requirement | Acceptance condition |
|---|---|---|
| CTR-01 | A capability may reference one or more versioned contract artifacts. | Every binding identifies capability/version, artifact type, path/URI, immutable digest or revision and authority. |
| CTR-02 | Adapters use a small, tool-neutral result protocol. | Input includes accepted spec revision, subject revision, artifact binding and execution scope; output reports checks, results, assumptions, provenance, artifacts, skipped/unsupported items and environment. |
| CTR-03 | Each adapter declares its verification semantics and limits. | No adapter claims proof of behavior beyond its checked properties and operating conditions. |
| CTR-04 | A provided capability's real and programmable mock implementations can use the same contract obligations. | Tests are executed against both; differences and omitted scenarios are reported explicitly. |
| CTR-05 | Adapter output is mapped to existing requirement/criterion IDs and versions. | Unmapped/stale results cannot silently satisfy release obligations. |
| CTR-06 | Trust boundaries and external dependencies are not confused with verified providers. | Results distinguish checked behavior from accepted third-party assumptions and unverified cases. |
| CTR-07 | Unsupported artifact types do not invalidate the core model. | An adapter can be absent; the outcome is `unchecked`, not fabricated success. |

Initial adapters: reuse the existing JUnit import for a first executable adapter; follow with a single protocol-oriented adapter selected by the pilot (e.g., OpenAPI/Specmatic or Pact). No new contract language is required.

### 3.3 Verification assurance (ASR)

| ID | Requirement | Acceptance condition |
|---|---|---|
| ASR-01 | Report obligation coverage by accepted requirement and criterion. | Each is classified `proved`, `bounded`, `tested`, `analyzed`, `assumed`, `unverified` or `failed`, retaining all contributing evidence rather than collapsing conflicting claims. |
| ASR-02 | Report trusted assumptions as dependencies of verification claims. | Each assumption names provider/boundary, validity conditions, verification status and affected obligations. |
| ASR-03 | Bind evidence to exact specification and implementation revisions. | A stale binding is reported separately and cannot be treated as current. |
| ASR-04 | Record verification effectiveness tests. | For a deliberately invalid mutant/counterexample, capture targeted property, expected rejection, actual outcome, tool/revision and evidence links. |
| ASR-05 | Surviving mutants create visible gaps. | An un-killed relevant mutant cannot be counted as adequate coverage without a documented acceptance decision. |
| ASR-06 | Acceptance remains policy-driven, scoped and reproducible. | Reports show evaluated scope, deferred/waived obligations, residual risk, acceptance policy ID/version and reasons. |
| ASR-07 | Present both machine-readable and human-reviewable results. | JSON and concise Markdown views agree on status and evidence references. |

Statuses are **assurance classifications**, not automatically new `Evidence.method` enum members. Proof claims always state tool, checked property, assumptions and bounds, including soundness/coverage limitations. Absence of mutation testing is `not assessed`, not automatically failure under every project policy.

### 3.4 Replacement and independent execution (RPL)

| ID | Requirement | Acceptance condition |
|---|---|---|
| RPL-01 | Accepted component slices can be exported without implementation-private source. | The handoff includes required/provided capability contracts, risk/requirement obligations, mocks, assumptions and interface data models, but excludes old implementation code. |
| RPL-02 | Components adopted into the isolation workflow can run with substituted dependencies. | Existing `--check-isolation` checks run with executable, controlled harnesses; unknown coupling is recorded rather than ignored. |
| RPL-03 | A provider publishes programmable mocks of its provided capabilities. | Real/mock conformance checks and scenario limitations accompany the capability version. |
| RPL-04 | Replacement acceptance uses fresh, independently obtained evidence. | Old implementation evidence never transfers automatically to a new revision. |
| RPL-05 | Differences are classified before changing acceptance criteria. | Each discrepancy is marked implementation defect, specification gap, verification gap, legacy discrepancy or intentionally unconstrained behavior. |
| RPL-06 | A pilot can construct an alternative implementation from the accepted slice. | An independent implementer without the predecessor's code can deliver a candidate, run checks and produce a reviewed assurance report. |

## 4. Proposed artifacts (illustrative, not existing schema)

### 4.1 Change set

```yaml
id: change-42
base_revision: git:abc123
intent: "Make reservation retries idempotent"
affected: [reserve-inventory, req-reserve-contract]
decisions:
  - question: "What happens after a committed write with a timeout?"
    accepted: "Reusing the same key returns the committed outcome"
operations:
  - op: modify
    kind: requirement
    id: req-reserve-contract
    expected_version: "1.0"
    proposed_version: "1.1"
review:
  status: proposed
```

An accepted change set carries an explicit decision and may produce a deterministic patch; the old and target specs remain available for impact and compatibility checks.

### 4.2 Contract binding and adapter result

```yaml
binding:
  capability: reserve-inventory
  version: "1.0"
  authority: derived
  format: openapi
  artifact: contracts/inventory.yaml
  sha256: "<actual digest>"
  verifies: [req-reserve-contract]
result:
  adapter: example-adapter
  subject_revision: "git:def456"
  spec_revision: "git:abc123"
  checks:
    - criterion: req-reserve-contract/returns-conflict
      status: passed
      mode: mock
      provenance: junit:artifacts/contract-tests.xml
  unsupported: [distributed-retry-timing]
```

Adapters may produce `test`, `inspection`, `analysis` or other existing evidence methods where justified. Unverified adapter claims remain informational and do not silently produce passing evidence.

### 4.3 Assurance report

```yaml
scope: inventory
spec_revision: "git:abc123"
candidate_revision: "git:def456"
policy: project/release-policy@1
obligations:
  - id: req-reserve-contract
    status: tested
    evidence: [ev-reserve-contract]
    assumptions: [storage-atomicity]
    limitations: [network-partition-not-exercised]
trusted_assumptions:
  - id: storage-atomicity
    provider: datastore
    status: accepted-unverified
effectiveness:
  - mutant: remove-idempotency-check
    target: req-reserve-contract
    expected: rejected
    observed: rejected
decision: requires-review
```

The report is **derived** from canonical spec, evidence, tool results and reviewed decisions. It is not a separate authority.

## 5. Verification semantics and security

- Verify declared positive, negative, stateful, failure, temporal, concurrency and nonfunctional behaviors as applicable; identify omissions.
- Separate evidence method, assertion scope, completeness assessment and implementation revision. Passing tests do not imply proof; proofs against a model require explicit correspondence to implementation.
- Adapter execution and component harnesses run **untrusted project code**. Treat manifest paths, command arguments, test reports and tool output as untrusted input. Use least-privilege isolated CI, resource/time limits, no production credentials, restricted network and bounded artifact sizes. Never claim argv/no-shell execution is a sandbox.
- Artifact links must be pinned and path/digest-validated. Avoid arbitrary remote fetches or package execution in metadata-only validation.
- A provider mock has a test-control surface that must not be exposed in production. Mock agreement with a real provider on sampled cases is not semantic equivalence.
- External contracts may refine a capability only through a reviewed decision; generated schema/code must not overwrite accepted requirements.

## 6. Acceptance scenarios

1. **Propose without altering authority:** create a change set, inspect impact/compatibility, reject it; accepted spec stays byte-identical.
2. **Accept and apply:** review a compatible change, apply it once, confirm baseline pin and audit record; applying against a diverged baseline fails without mutation.
3. **Break compatibility:** change failure modes or data fields; existing compatibility rules flag the breaking behavior and dependent verification is invalidated.
4. **Dual contract suite:** real provider and programmable mock both run a shared contract suite; an intentionally incorrect mock fails with a traceable criterion.
5. **Trust gap:** a consumer test passes under a mock while the real provider violates an assumption; the report does not claim the dependent obligation was fully verified.
6. **Mutation:** a deliberately incorrect implementation passes ordinary happy-path tests but is rejected by an added property/negative test; retain both findings.
7. **Replacement:** a new implementer with only accepted spec, interfaces, mocks and tests builds a candidate; all new evidence is bound to the new revision; unaddressed ambiguity blocks claimed completeness.
8. **Backward compatibility:** an existing repository specification that declares no optional change sets/adapters/assurance policy still parses and validates unchanged.
9. **Scope and failure:** partial adoption, unavailable adapter, malicious path, timed-out harness and invalid report remain explicit and cannot silently pass.

## 7. Relationship to existing documents and market practices

- [FEATURE.md](FEATURE.md) remains the behavioral change workflow; proposed change sets make review and application concrete.
- [REPLACEMENT.md](REPLACEMENT.md) remains the implementation replacement workflow; the pilot validates it.
- [COMPONENT-ISOLATION.md](COMPONENT-ISOLATION.md) continues to own consumer fakes, provider mocks and isolated execution.
- [spec.md](spec.md) remains the semantic source of truth; these candidate formats must be reviewed before entering reference schemas.
- Research inputs: [OpenSpec](https://github.com/Fission-AI/OpenSpec) (reviewable deltas), [GitHub Spec Kit](https://github.com/github/spec-kit) (agent workflow), [Smithy](https://smithy.io/2.0/) (extensible traits/projections), [Pact](https://docs.pact.io/) and [Specmatic](https://specmatic.io/) (contract conformance), [Microcks](https://microcks.io/) (mocking), and [Microsoft PDD Kit](https://www.microsoft.com/en-us/research/blog/formally-verifying-agent-written-full-stack-web-apps-with-pdd-kit/) (explicit trust, proof/test boundaries and verification challenges). These are patterns, not imported requirements.
