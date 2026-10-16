# Implementation Plan: Agent-Oriented Component Development

**Status:** Proposed; no implementation started
**Primary specification:** [spec/20-agent-component-development.md](../../spec/20-agent-component-development.md)
**Alignment:** Observable 0.2, ADR-035, AGENTS.md, spec/10-process.md

## Delivery principles

Implement as small, human-reviewed PRs. Preserve the existing 0.2 independent-component roadmap and runtime topology. Prefer local-first Rust CLI, SQLite state and JSON interchange. Reuse Cargo, npm, Make, local-ci and existing test suites. Do not add a central runtime service or a new universal build system.

Each PR must describe the source requirement IDs, changed boundaries, test evidence, regression coverage, rollback, and whether an ADR update is required. Documentation-only PRs follow existing code-check exemptions. Code PRs follow all mandatory AGENTS.md local gates.

## Proposed layout

```text
spec/20-agent-component-development.md
spec/adr/ADR-036-agent-component-development.md
docs/plans/agent-component-development.md
agentbuild/
  schemas/
  components/
  capabilities/
  changes/
  crates/
    agentbuild-cli/
    agentbuild-core/
    agentbuild-scheduler/
    agentbuild-verifier/
  tests/
```

These paths are targets, not existing files. Keep the CLI outside the root Cargo workspace initially if doing so avoids unnecessary rebuild coupling; document the chosen integration.

## PR 0 — Architecture baseline

**Deliverables:** Main specification, this plan, ADR-036; reference the work from the 0.2 roadmap without replacing the authoritative release sequence.

**Exit:** Human review confirms build-time-only scope, unchanged ADR-035 component targets, immutable acceptance criteria and local CI preservation.

## PR 1 — Component inventory and schema

**Deliverables:** Versioned JSON Schema, parser, normalized JSON output, ten draft component manifests, path inventory for Rust services/libs, frontend, Modelable, migrations, generated code, scripts and distribution. Shared/ambiguous ownership explicitly classified.

**Tests:** malformed manifests, duplicate requirement IDs, overlapping writable globs, unowned changed files, symlink/path escapes, missing references.

**Exit:** `agentbuild validate` deterministically rejects invalid ownership and all pilot-owned paths are accounted for.

## PR 2 — Capabilities and dependency resolution

**Deliverables:** Capability catalog, provider requirements, contract/implementation edge types, pinned lockfile, graph visualization, change impact.

**Tests:** version and feature incompatibility, ambiguous providers, missing contracts, implementation cycles, deterministic lockfile, contract-change invalidation.

**Exit:** `agentbuild resolve`, `agentbuild graph` and `agentbuild impact` operate on real Observable pilot components.

## PR 3 — Work-package compiler

**Deliverables:** Change-request schema, context selection, immutable work-package format, digest generation, protected-path policy, verification-plan generation.

**Tests:** stable hashes, mandatory context preservation, out-of-scope paths, stale base revision, acceptance-plan tampering.

**Exit:** `agentbuild plan --change <path>` produces reproducible packages without sending unrelated repository content.

## PR 4 — Single-agent executor

**Deliverables:** One CLI agent adapter, dedicated Git worktree and short-lived branch, sandbox/process limits, logs, patch/commit capture, cancellation and cleanup.

**Tests:** timeout, agent failure, unauthorized writes, cleanup after crash, generated-file restrictions, no secret inheritance.

**Exit:** A bounded pilot change can be proposed without changing main or protected specifications. Git worktree isolation must not be presented as a security boundary.

## PR 5 — Independent verifier

**Deliverables:** Fresh-environment verification, diff scope checks, test discovery, requirement-to-evidence mapping, scoped native test adapters, JSON report and rejection reasons. Reuse local-ci and all applicable existing gates rather than substituting new tests.

**Tests:** intentionally false agent success, zero discovered tests, failing acceptance test, modified protected tests, stale evidence, generated-code drift.

**Exit:** `agentbuild verify <work-package>` rejects all seeded violations and records exact source/spec/test-plan digests.

## PR 6 — Parallel scheduler

**Deliverables:** SQLite-backed state machine, ready queue, critical-path priority, ownership/resource leases, bounded retries, crash recovery, separate verification concurrency, stale work invalidation.

**Tests:** parallel disjoint tasks, overlapping edits blocked, contract-only edges not blocking, implementation prerequisites blocking, worker crash/recovery, stale lockfile.

**Exit:** `agentbuild run --parallel 3` safely executes multiple independent work packages.

## PR 7 — Integration and PR evidence

**Deliverables:** Deterministic integration candidate assembly, merge-conflict handling, integrated-tree re-verification, PR evidence summary, human-review handoff.

**Tests:** stale accepted commit, integration conflict, test regression after merge, evidence mismatch, no automatic main merge.

**Exit:** `agentbuild integrate` produces a reviewable branch/PR candidate with verification evidence; human approval is still required.

## PR 8 — Observable pilot and evaluation

Pilot against existing component boundaries rather than an invented demo architecture. Suggested sequence:

1. `observable-query` / `libs/query-core`: bounded pure-code change with isolated tests.
2. `observable-web`: independent UI change with TypeScript, component and browser verification.
3. `observable-process` / storage contracts: contract-dependent change requiring integration/Testcontainers verification.

Select exact requirements and file ownership after PR 1 inventory. Use deliberate failure injections in throwaway branches, never weaken protected regression gates.

**Exit:** Demonstrate deterministic work packages, two parallel tasks, independent rejection of defective output, integrated-tree verification, and a human-reviewed PR.

## Acceptance matrix

| ID | Acceptance condition | Verification |
| --- | --- | --- |
| AC-01 | Conflicting ownership rejected | Schema/ownership fixture |
| AC-02 | Capability incompatibility rejected | Resolver fixture |
| AC-03 | Deterministic work-package digest | Repeat compilation |
| AC-04 | Agent cannot change acceptance baseline | Protected-path/sandbox test |
| AC-05 | Disjoint tasks execute concurrently | Scheduler integration test |
| AC-06 | Implementation dependencies block | DAG fixture |
| AC-07 | Deliberately broken behavior rejected | Independent verifier fixture |
| AC-08 | Zero-test success rejected | Test discovery fixture |
| AC-09 | Contract changes stale affected tasks | Impact test |
| AC-10 | Integrated tree reverified | Integration fixture |
| AC-11 | Existing local CI remains authoritative | Regression evidence review |
| AC-12 | No unreviewed main merge | PR workflow review |

## Operational metrics

Record tokens per accepted requirement, wall-clock time to verified integration, first-pass acceptance, retry count, parallel utilization, merge conflicts, stale-work invalidations, test coverage gaps and verifier runtime. Compare against a single-agent baseline. Do not optimize for raw agent task throughput.

## Rollback

The subsystem is opt-in. Remove the agentbuild invocation and continue with the existing AGENTS.md branch/PR workflow and local-ci. Do not alter production binaries, deployment manifests or service runtime paths as part of the pilot.

## Dependencies and risks

- Physical ownership is not yet fully aligned with ADR-035 target boundaries; inventory before enforcement.
- Shared Cargo workspace files and Modelable generation create resource conflicts; lock explicitly.
- Existing local-ci can be expensive; use narrow checks during iteration, but retain mandatory final checks.
- Worktrees alone do not isolate untrusted agent execution; sandbox before granting tool access.
- Requirement coverage does not prove requirement correctness; require human review of acceptance criteria.

## Formal verification workstream (Alloy + TLA+)

Implement formal models alongside the corresponding executable feature, not as a disconnected final phase. Reuse the conventions and tool-version pinning approach of `spec/19-formal-verification.md`; these are development-only checks.

| Delivery | Formal addition | Executable alignment / exit evidence |
| --- | --- | --- |
| PR 1 — Schema and ownership | Alloy model for unique source ownership, requirements and accepted-test coverage | Intentionally overlapping ownership and uncovered requirements yield counterexamples and validator failures |
| PR 2 — Capability resolution | Alloy model for provider eligibility, unique binding and implementation DAG acyclicity | Resolver fixtures match valid and invalid Alloy examples, including contract-only edges |
| PR 3 — Work packages | Extend Alloy with scoped requirements and immutable evidence references | Work-package compiler rejects mismatched requirement or digest references |
| PR 5 — Independent verifier | TLA+ state transitions for submit, verify, reject, retry and invalidate | TLC catches acceptance without required checks or acceptance with stale/tampered evidence |
| PR 6 — Scheduler | TLA+ concurrency, lease acquisition/expiry, worker crash/recovery, prerequisite and fairness model | TLC catches double writers and premature starts; scheduler integration tests reproduce traces |
| PR 7 — Integration | Extend TLA+ with candidate rebasing, stale evidence and integration gates | TLC catches integration against outdated evidence; executable integration test reproduces it |
| PR 8 — Pilot | Run pinned model suites and cross-check representative real component workflows | Report model bounds, checked properties, negative-case counterexamples and matching regression tests |

### Formal tooling and artifacts

- `agentbuild/formal/alloy/`: `.als` models and bounded valid/invalid fixtures.
- `agentbuild/formal/tla/`: `.tla` models, TLC configurations and expected counterexample scenarios.
- `agentbuild/formal/README.md`: exact pinned Alloy Analyzer and TLC versions, local invocation, bounds, fairness assumptions, limitations and links to executable regression tests.
- Add local commands (for example `agentbuild formal check`) only after reproducible tool invocation is established. Do not silently download unpinned binaries or require production services.

### Exit conditions

1. At least one intentionally invalid ownership/provider graph produces an Alloy counterexample.
2. At least one intentionally broken scheduler/acceptance transition produces a TLC counterexample.
3. Correct bounded models pass with documented scopes, assumptions and state-space limits.
4. Every modeled invariant has an associated executable test.
5. No ordinary source-code change requires formal checking unless it affects the modeled orchestration or formal specifications.
6. Formal results do not bypass `scripts/local-ci.sh`, protected acceptance tests or human PR review.
