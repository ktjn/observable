# Verified Development: Implementation Plan

Status: **proposed roadmap**, complementary to [VERIFIED-DEVELOPMENT.md](VERIFIED-DEVELOPMENT.md) and the existing [PLAN.md](PLAN.md). No milestone is claimed complete by this document.

## Goal and sequencing

Demonstrate that accepted, implementation-independent requirements and contracts can guide an **independent replacement** of one component, then harden the model and tooling only where the experiment reveals concrete gaps.

```text
M0 baseline and pilot
   |
M1 replacement experiment + gap report
   |
M2 reviewed specification change sets
   |
M3 executable contract adapter protocol
   |
M4 trust and verification adequacy reports
   |
M5 cross-project adoption and hardening
```

Preserve the current metamodel and reference implementation as independent concerns. Each addition should be opt-in unless multiple adopters justify promoting it. Existing specifications must continue to load unmodified.

## M0 — Establish baseline and pilot (P0)

**Purpose:** Choose a bounded capability with meaningful edge cases and existing consumers; avoid using a toy CRUD path as proof of feasibility.

Tasks:
- Select one capability from [modelable](https://github.com/ktjn/modelable) or [searchable](https://github.com/ktjn/searchable); choose based on available, reviewable behavioral contract and deterministic test harness, not language preference.
- Pin a repository revision and accepted specification revision. Inventory requirements, risks, invariants, assumptions, required/provided capabilities and external dependencies.
- Identify the consumer interface and set up a provider-owned programmable mock with isolated consumer execution where feasible.
- Enumerate at least: valid input, invalid input, state changes, error behavior, retries, concurrency/order sensitivity and environmental assumptions, marking irrelevant cases explicitly.
- Assign an independent reviewer and an implementation agent/session without source access.

Deliverables: `examples/replacement-pilot/README.md` (scope), pinned artifacts, decision log, gap register and test plan. Do not copy proprietary or sensitive data into the example.

**Gate M0:** Baseline and acceptance authority recorded; no critical ambiguity disguised as an accepted requirement. A reviewer can distinguish known/unknown and verified/assumed behavior.

## M1 — Independent replacement experiment (P0)

**Purpose:** Test sufficiency of current specifications before extending the universal schema.

Tasks:
- Produce a bounded `--context` handoff with accepted contracts, required mock controls and verification obligations; remove predecessor implementation source and test implementation details that reveal it.
- Ask an independent agent to create an alternate implementation. Prefer another algorithm or language where interoperability is practical, but behavior—not technology change—is the test.
- Run independent contract, property, negative and failure-path checks against both implementations where appropriate. Add a small set of deliberately wrong mutants (e.g., remove validation, disable idempotency, permit invalid state).
- Record disagreements in five categories: implementation defect, missing/ambiguous requirement, inadequate verification, legacy discrepancy, intentionally unconstrained difference.
- Compare contract results and repeatability; explicitly list anything that cannot be verified without the original code or real dependencies.

Deliverables: replacement candidate, fresh test/proof evidence, mutation results, `PILOT-REPORT.md` and ranked model/tooling gaps.

**Gate M1:** A new implementation can be evaluated without source access to its predecessor, all failed/unchecked properties are visible, and no claim of exhaustive equivalence is made. A blocked candidate with a precise gap report is still a useful experimental outcome.

## M2 — First-class specification change sets (P0)

**Purpose:** Preserve an authoritative baseline while enabling reviewed, machine-readable changes.

Tasks:
1. Define a **change-set JSON Schema** as a separate proposed-document schema; do not overload the authoritative system specification.
2. Model `id`, `base_revision`, `intent`, `affected`, operations (`add|modify|deprecate|remove`), expected versions, decisions, risk/verification changes, review state and provenance.
3. Implement pure `plan_change(baseline, change)` and `validate_change` logic; reuse `--impact`, `--compat` and evidence invalidation.
4. Provide CLI equivalents of `change inspect` (diff/impact/compatibility), `change validate` and `change apply`; command naming can be refined before implementation.
5. Make apply atomic with exact-baseline/revision conflict detection, deterministic ordering and audit record. Reapplication must be idempotent or yield an explicit conflict.
6. Test stale bases, invalid IDs, entity cycles, risky removal, merge conflicts, accepted/rejected decisions and no partial updates.

Deliverables: change-set schema, parser, CLI actions, positive/negative corpus, change workflow example, docs. All operations are explicit; the tool must not generate missing behavioral requirements.

**Gate M2:** A reviewed change can be applied repeatably; a rejected or stale change cannot modify the baseline. Existing `Spec` documents remain valid.

## M3 — Executable contract adapters (P0)

**Purpose:** Avoid reimplementing Pact, Specmatic, Smithy and other domain-specific tools.

Tasks:
1. Define an adapter protocol with versioned **input** (spec/capability/subject revisions, contract artifact digest, requirement/criterion mapping, mode, allowed environment) and **output** (checks, results, method, provenance, limitations, skips, subject digest).
2. Implement a protocol validator and a runner interface separate from metadata-only validation. Adapter failures or missing binaries are `failed`/`unchecked`, never `passed`.
3. Adapt existing JUnit `--check-report` / `--emit-evidence` handling first, avoiding duplicate verification logic.
4. Select and implement **one** external adapter based on pilot need: OpenAPI/Specmatic, Pact, Smithy validation or another contract engine. Defer additional adapters until adoption justifies them.
5. Integrate existing `--check-isolation`: run the same accepted contract suite against real and mock, then collect criterion-linked results and limitations.
6. Require digest-pinned artifacts and controlled CI for executable adapters; no remote fetch or execution during ordinary spec validation.
7. Test malformed output, stale binding, unsupported format, timeout, path traversal, false passes, and real/mock mismatch.

Deliverables: `adapter-protocol.schema.json`, runner/collector, reference adapter, fixture and documentation. Protocol is tool-agnostic; contract source formats are adapters, not new metamodel requirements.

**Gate M3:** One executable adapter produces reproducible, criterion-linked evidence for both real and mock implementations. Missing or unsupported verification is surfaced.

## M4 — Trust, coverage and verification effectiveness (P1)

**Purpose:** Make evidence **sufficiency questions** visible rather than equating all passing checks.

Tasks:
1. Extend `--evidence-report` via an optional assurance view or separate `--assurance-report`. Reuse current evidence evaluation logic and acceptance scope.
2. Represent assurance classifications (`proved|bounded|tested|analyzed|assumed|unverified|failed`) without changing `Evidence.method` merely to fit a report.
3. Represent assumptions and trusted boundaries as reviewable relationships to capability/requirement versions, including owner, justification, limitations and verification status.
4. Derive transitive assumption dependence: if A relies on B and B is unverified under the chosen policy, mark A's claim **conditional** rather than fully proven.
5. Record mutation/counterexample challenges with target obligation and expected/actual rejection; report surviving relevant mutants.
6. Emit JSON and Markdown reports with current spec/candidate revisions, open gaps, deferred scope, residual risks and review decisions.
7. Make accepted policy optional; refuse to imply that mutation score or evidence count alone proves assurance.

Deliverables: assurance-report schema, renderer, assumption linking model or external sidecar as justified, tests, replacement pilot report.

**Gate M4:** A reviewer can identify precisely what is proved, tested, assumed, unverified or failed for the pilot, including conditional provider trust and mutation results.

## M5 — Cross-project validation and integration (P1/P2)

Tasks:
- Trial the flow on a **second** project with different language/runtime and component shape to test generality.
- Assess adapter portability for Rust, TypeScript and Java without making any required.
- Connect change-set review to a normal GitHub PR workflow, leaving Git as the repository authority for file revisions.
- Consider first-class consumer/provider compatibility matrices, Arazzo workflows, Smithy-style extensions and external catalog exports only after evidence from pilots.
- Document migration and compatibility of the new optional schemas; ensure no regression to existing release, impact, context, isolation and evidence commands.

**Gate M5:** Two materially different pilots reproduce the workflow using the same conceptual contracts and evidence model; project-specific integrations remain optional.

## Delivery ownership and parallelism

| Workstream | Independent ownership | Dependency |
|---|---|---|
| Pilot requirements and acceptance | Specification reviewer | M0 |
| Candidate implementation | Isolated implementation agent | Approved M0 handoff |
| Independent tests and negative cases | Verification reviewer | Approved M0 handoff; may run parallel to candidate |
| Change-set model/CLI | Tooling agent | M1 findings; no need for M3/M4 |
| Adapter protocol and first implementation | Integration agent | M1 findings; can run parallel to M2 after interface agreement |
| Assurance report | Verification tooling agent | Adapter result contract and M1 evidence |
| Integration and acceptance | Project owner | Results of relevant gates |

Implementation agents cannot modify accepted specifications or verification criteria to satisfy their own work. Reviewer-owned changes use the change-set workflow once available.

## Test and quality strategy

- **Unit/model tests:** parse and validate new optional artifact formats, deterministic diffs, stale-revision detection, impact correctness, assumption graph, report serialization.
- **Property/metamorphic tests:** change-set application is deterministic; rejected changes do not mutate the baseline; applying unrelated changes preserves untouched entity semantics.
- **Contract tests:** independent real/mock suite using identical accepted obligations; explicit unsupported modes.
- **Mutation tests:** invalid candidate intentionally violates targeted requirements and must be reported as failing when the verification obligation claims to cover it.
- **Security tests:** path traversal, symlink escapes, malformed adapter output, over-large artifacts, command injection, secrets exposure and network access assumptions. Execute code only in controlled CI.
- **Regression/packaging:** existing schema, corpus, CI, wheel/sdist, manifest, Windows/Linux and optional features all continue to work. Update reference JSON Schemas and toolkit packaging only when code genuinely changes.

No milestone is complete merely because documentation or a metadata validator passes. The consuming project decides acceptance from reproduced evidence, limitations and explicit risk decisions.

## Dependencies, risks and decision points

| Risk | Mitigation / decision |
|---|---|
| Metamodel overgrowth | Start with sidecar artifacts; promote only reusable semantics supported by two pilots |
| Duplicated contract tool implementation | Delegate protocol details to mature tools through adapters |
| False assurance from tests/mocks | Explicit assumptions, bounds, scope and negative/mutation tests |
| AI silently modifies requirements | Pin baseline and separate implementer from specification authority |
| Unsafe execution of adapter/project code | Isolated, short-lived CI with least privilege and no production credentials |
| Version sprawl | Bind all artifacts/results to accepted spec and implementation digests |
| Vendor or language lock-in | No mandatory proof language, API format, agent runner or test engine |
| Brownfield behavior laundering | Require a review decision before observed bugs become accepted obligations |

## Proposed PR breakdown

- **PR A:** Pilot baseline, artifact handoff and gap register (no schema changes).
- **PR B:** Minimal change-set format, impact/diff and conflict-safe apply.
- **PR C:** Tool-neutral adapter protocol and JUnit adapter.
- **PR D:** One external contract adapter + real/mock conformance fixture.
- **PR E:** Trust/coverage/mutation assurance reporting.
- **PR F:** Cross-project validation and optional refinements.

## Current recommended next action

**Start M0 on one bounded capability in `modelable` or `searchable`, then execute M1.** The experiment determines which changes are necessary; do not implement all model/schema extensions before it.

References: [VERIFIED-DEVELOPMENT.md](VERIFIED-DEVELOPMENT.md), [PLAN.md](PLAN.md), [FEATURE.md](FEATURE.md), [COMPONENT-ISOLATION.md](COMPONENT-ISOLATION.md), [REPLACEMENT.md](REPLACEMENT.md), [MODEL-SCOPE.md](MODEL-SCOPE.md).
