# Specification: Agent-Oriented Component Development

**Status:** Proposed
**Scope:** Build-time engineering system, not a runtime architecture
**Applies to:** Observable 0.2 component decomposition
**Related:** [Implementation plan](../docs/plans/agent-component-development.md), [ADR-035](adr/ADR-035-component-independence.md), [AGENTS.md](../AGENTS.md), [process](10-process.md)

## 1. Purpose

Enable AI coding agents to implement bounded changes to Observable's logical components using approved requirements, capability contracts, explicit source ownership, and independent verification. Minimize agent context, enable safe parallelism, and make completion evidence reproducible.

This system **must not** introduce runtime service discovery, dependency injection, a production control plane, or new dependencies between Observable services. It does not replace Cargo, npm, Make, existing Testcontainers suites, local CI, or human-reviewed PRs.

## 2. Core invariants

- A component is a logical specification, not necessarily a directory, crate, package, or deployable service.
- Every writable path has at most one component owner per execution plan. Shared paths require an integration-owned work package.
- A requirement has a stable ID, normative acceptance criteria, and an explicit verification method.
- Agents cannot change approved specifications, acceptance suites, architectural policies, or their own permissions while implementing.
- Required capabilities are declared independently of provider implementations; provider selection happens **at build/planning time**.
- A contract dependency permits parallel implementation after its contract is frozen. An implementation prerequisite blocks scheduling until its producer is accepted.
- A successful agent response is never evidence of correctness. Only independent verification can accept a change.
- No automated merge to main. Each iteration uses a short-lived branch and PR with human review, consistent with AGENTS.md.
- Generated code is changed through its Modelable source and regeneration scripts, never by direct edits.
- Existing regression checks remain protected and cannot be weakened to obtain a pass.

## 3. Domain model

**Component:** Stable ID, source ownership, capabilities provided/required, requirements, build/test adapter, policy references.

**Capability:** Versioned behavioral contract, mandatory and optional features, conformance obligations. A feature is not merely a label: it maps to testable expectations.

**Requirement:** ID, normative statement, scope, acceptance criteria, severity, verification references.

**Change request:** Requested outcome, targeted requirements, allowed specification baseline, risk classification.

**Work package:** Immutable, content-addressed assignment containing a base commit, component ID, required contracts, allowed read/write paths, context manifest, resource limits, and verification plan.

**Evidence:** Machine-readable record tied to source tree, specification, test-plan, toolchain and environment digests.

**Integration candidate:** Deterministic composition of accepted commits against a pinned base.

## 4. Existing Observable component mapping

The initial logical components are those defined by ADR-035 and docs/component-decomposition.md:

| Logical component | Initial ownership / source |
| --- | --- |
| observable-contracts | `libs/domain`, `libs/observable-storage-contracts`, contract schemas, `models/*.mdl` |
| observable-auth | `services/auth-service`, auth-owned support code |
| observable-control | `services/admin-service` plus explicitly assigned migrations from query/ingest |
| observable-ingest | `services/ingest-gateway` |
| observable-process | `services/stream-processor`, `libs/observable-process` |
| observable-store-clickhouse | `services/storage-writer`, `migrations/clickhouse` |
| observable-query | `services/query-api`, `libs/query-core` |
| observable-alerting | `services/alert-evaluator` plus explicitly assigned extraction slices |
| observable-web | `apps/frontend`, playground ownership per decomposition |
| observable-distribution | Compose, Helm, demos, testbench, full-stack E2E |

These are **target boundaries**, not assertions that today's physical ownership is already exclusive. Before enforcement, produce a reviewed path inventory and classify ambiguous areas. `libs/domain-core`, `libs/observable-config`, `libs/observable-telemetry`, `libs/test-support`, `libs/playground-wasm`, generated files, shared migrations, root manifests and scripts require explicit assignment or shared-resource locks. No broad wildcard may silently grant overlapping writes.

## 5. Specification representation

Use human-readable YAML manifests with JSON Schema structural validation. CUE constraints may be introduced for cross-file validation only when simpler deterministic checks are insufficient. Normalize all inputs to a canonical versioned JSON representation before hashing.

Proposed manifest:

```yaml
apiVersion: agentbuild.dev/v1alpha1
kind: Component
metadata:
  id: observable-query
  version: 0.1.0
spec:
  ownership:
    writable:
      - services/query-api/**
      - libs/query-core/**
    protected:
      - spec/**
      - scripts/local-ci.sh
  provides:
    - capability: telemetry.query
      version: 1.0.0
      contract: contracts/query.openapi.yaml
  requires:
    - capability: telemetry.storage-schema
      version: ">=1.0.0 <2.0.0"
      features: [read-traces]
      dependencyKind: contract
  requirements:
    - id: QUERY-001
      statement: Tenant scope is enforced for trace queries.
      acceptance:
        - test: services/query-api/tests/http_api_integration.rs
          type: integration
  build:
    adapter: cargo
    targets: [query-api, query-core]
```

This is illustrative, not a claim that `contracts/query.openapi.yaml` already exists. The pilot must use real existing paths and tests. Validate unique IDs, paths, version ranges, references, cycles, feature definitions, and coverage.

## 6. Capability resolution

1. Resolve a consumer's required capability against the approved component manifest index.
2. Check version ranges, required features, contract compatibility, platform/binding constraints, and policy.
3. Select the pinned provider explicitly; ambiguous matches fail until resolved.
4. Emit a lockfile recording component, provider, contract and source digests.
5. Separate **contract edges** from **implementation prerequisite edges**. Reject cycles in the latter.
6. Do not infer behavioral compatibility solely from SemVer or type compatibility.

Contract verification must be pluggable: OpenAPI/HTTP, versioned event schemas, storage-schema compatibility, Rust library interfaces, and UI behavior/accessibility contracts. Avoid inventing a universal transport.

## 7. Work-package compilation

Inputs: approved change request, base revision, component manifests, dependency lockfile, verification policy.

Output: immutable work package with objective, requirement IDs, scoped context, read/write allowlists, protected paths, frozen contract digests, test commands, retry budget and completion conditions.

Context selection is deterministic: include relevant specifications, owned implementation, consumer/provider contracts, applicable tests, architectural decisions, and concise dependency summaries. Token budget is advisory; never truncate mandatory requirements or acceptance criteria. If necessary split the task.

## 8. Agent lifecycle

`planned -> ready -> running -> submitted -> verifying -> accepted -> integrating -> integrated`

Terminal/exception states: `blocked`, `failed`, `cancelled`, `superseded`, `stale`.

Agents run on dedicated short-lived branches in isolated Git worktrees, preferably within sandboxed containers or equivalent isolation. Worktrees alone are not security boundaries. Agent adapters accept a work package and return a commit/patch plus logs. Agents must not merge or certify themselves.

A failed verification may produce a bounded repair assignment using the same frozen specification and a failure report. Repeated failures escalate to specification review; agents may not silently weaken tests.

## 9. Dependency-aware scheduler

Persist task state in local SQLite. Build a DAG from implementation prerequisites. Contract dependencies do not block implementation when approved baselines exist. Select ready tasks by critical-path priority with deterministic tie-breaking. Acquire exclusive leases for writable paths and shared resources (generated code, workspace manifests, migration catalogs, ports, Docker/Compose projects). Cap concurrent agents and verification jobs separately.

On worker failure or lease expiry, reconcile actual Git state before retry. On contract or base revision change, mark affected work packages stale and replan. Scheduler must never accept overlapping writes or reuse evidence for a different source tree.

## 10. Independent verification

Run outside the implementing agent's environment, using the submitted source revision and frozen verification plan.

Mandatory gates:
1. Validate specification and work-package digests.
2. Diff against base and reject writes outside allowed ownership or to protected files.
3. Check requirements-to-test traceability and test discovery counts.
4. Validate capability contracts and dependency compatibility.
5. Execute scoped formatting, lint, typecheck, unit and relevant contract/integration tests.
6. Enforce architectural rules and generated-code drift checks.
7. Execute required Observable local regression gates, including `scripts/local-ci.sh` as prescribed by `spec/10-process.md` and `AGENTS.md`; preserve applicable Testcontainers, HTTP handler, NLQ, performance, browser and formal-verification checks.
8. Reverify the composed integration candidate before marking work integrated.

Test results supplied by the agent are diagnostic only. A zero-test pass does not satisfy an expected acceptance suite. Missing tools or skipped gates are partial evidence, not success. Human review remains mandatory.

## 11. Evidence and certification

Evidence includes work-package ID, base and submitted commit, normalized spec hash, contract hashes, verification plan hash, runner/tool versions, environment fingerprint, discovered tests, requirement outcomes, command exit codes, logs/artifact references, scope diff and final decision.

Use JSON initially, with optional in-toto attestations later. A certificate is valid only for the exact source tree, specifications and verification plan. Distinguish `passed`, `failed`, `uncovered`, `inconclusive` and `stale`.

## 12. Security

Use least-privilege execution, no production credentials, disabled network by default, resource quotas and command timeouts. Treat repository instructions, dependencies, agent output and test logs as untrusted. Enforce writable paths at sandbox level where supported and always check Git diffs. Prevent symlink/path traversal and submodule escapes. Protect acceptance suites, CI scripts, architecture policies and model generation sources with reviewer-controlled change procedures.

## 13. Integration with existing engineering workflow

The agent system prepares changes and evidence; it does not override `AGENTS.md`, `spec/10-process.md`, ADR-019 or ADR-035. Every iteration uses a branch and PR. The local validation policy remains authoritative; GitHub workflows are not redefined as substitutes for local verification. Documentation-only changes follow the existing exemption for mandatory code checks.

## 14. MVP acceptance criteria

- Invalid or conflicting component ownership is rejected.
- A work package can be generated deterministically from pinned inputs.
- At least two non-conflicting implementation tasks run concurrently.
- Overlapping source edits are serialized or rejected.
- Protected specification/test edits fail verification.
- A deliberately failing requirement is detected despite an agent reporting success.
- Missing test discovery fails mandatory acceptance.
- Contract changes invalidate affected consumers.
- A failed worker can be retried without silently reusing stale evidence.
- Integration verification executes on the merged candidate.
- No unreviewed merge to main occurs.
- Existing Observable regression coverage is preserved.

## 15. Non-goals

No runtime component registry, runtime provider selection, distributed agent fleet, custom coding model, production orchestration, replacement build system, automatic acceptance-test generation, repository split, or change to Observable service topology.

## 16. Open decisions

- Initial CLI adapter: Codex CLI, OpenCode or another installed agent.
- First pilot components and exact ownership inventory.
- Whether to use filesystem sandboxing immediately or begin with restricted containers.
- Which existing contract artifacts can be reused without introducing a duplicate source of truth.

See the [implementation plan](../docs/plans/agent-component-development.md) for staged delivery and exit gates.

## 17. Formal model verification: Alloy and TLA+

Formal verification complements, but does not replace, executable validators, native tests, Testcontainers, or the required local CI gate. The models describe **build-time agent development behavior**, not Observable's production service topology. Follow the development-only principles of [spec/19-formal-verification.md](19-formal-verification.md).

### 17.1 Alloy: structural constraints

Maintain a bounded Alloy model for Component, SourcePath, Capability, Provider, Requirement, AcceptanceTest, WorkPackage and Dependency. Check:

- **Unique write ownership:** any concrete writable source path has at most one owner in an execution plan. Explicit shared-resource locks are distinct from ownership.
- **Capability resolution:** each required capability binds to exactly one approved provider satisfying version, mandatory features and compatible contract/binding constraints.
- **Graph validity:** implementation-prerequisite edges form an acyclic graph; contract-only edges are not incorrectly treated as execution prerequisites.
- **Traceability:** every mandatory requirement has at least one approved verification obligation; work packages cannot claim unrelated requirements.
- **Evidence provenance:** accepted evidence references the same source/specification/verification-plan digests as the certified candidate.

Use Alloy Analyzer to find counterexamples in finite scopes. Document chosen bounds; passing a bounded check is not an unbounded proof. Prefer small, deliberately invalid fixtures alongside valid fixtures. Map counterexamples into resolver, manifest-validator and evidence-validator regression tests.

### 17.2 TLA+/TLC: lifecycle and concurrency

Maintain a bounded TLA+ model for work-package states, implementation prerequisites, contract baseline revisions, ownership/resource leases, worker failure, retries, verification, integration and invalidation. Model at least these actions: Plan, MakeReady, AcquireLease, Start, Submit, VerifyPass, VerifyFail, ReleaseLease, ExpireLease, Retry, InvalidateContract, RebaseCandidate and Integrate.

Required safety properties:

- **Mutual exclusion:** conflicting work packages are never active with overlapping write ownership or exclusive resource leases.
- **Prerequisite safety:** a task starts only after its implementation prerequisites are accepted and its contract baseline is current.
- **Acceptance integrity:** failed, skipped, zero-discovery, tampered or stale verification cannot transition to accepted.
- **Integration integrity:** integration is impossible unless evidence matches the actual candidate tree and approved specification/test plan.
- **Retry integrity:** retries cannot inherit an invalid lease, bypass a required gate or reuse stale evidence.

Check bounded liveness under explicit fairness and environment assumptions: a ready task with available resources can eventually execute; a crashed worker's lease can eventually be reclaimed. Do not claim unconditional progress when a worker, reviewer or external dependency may remain unavailable.

### 17.3 Executable alignment

Keep models in `agentbuild/formal/alloy/` and `agentbuild/formal/tla/` with pinned tool versions and reproducible local commands. Model fixtures should cover valid and intentionally broken cases, with expected counterexamples. A passing model must be connected to at least one executable validator/scheduler/verifier test covering the same invariant. CI/local checks run when relevant formal models or orchestration code change, respecting Observable's local-validation policy; do not impose full model checking on unrelated component-only edits.

Alloy/TLC output is evidence for the correctness of bounded orchestration designs, **not** certification of an agent's generated application code. Application changes still require independent requirement and integration verification.
