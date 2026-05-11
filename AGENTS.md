# Agent Instructions

These instructions are foundational mandates for any AI agent interacting with this repository.

## Core Mandates

- **No Unreviewed Merges:** Nothing can be merged or committed to the main branch without a human review.
- **Branch and PR Every Iteration:** Before changing files, create or switch to a dedicated short-lived branch for the current task. Commit work only to that branch, push it to GitHub, and open a pull request for every iteration.
- **Verification & Testing:** Every change must be thoroughly tested and verified before being considered complete.
- **Reusable UI Components:** The UI must be built using reusable components with minimal duplication. Always check for existing components in `apps/frontend/src/components/` and `apps/frontend/src/features/**/components/` before creating new ones. Shared logic should be extracted into hooks or utilities.
- **Testcontainers for Real Dependencies:** Backend changes that touch PostgreSQL, ClickHouse, Redpanda/Kafka-compatible brokers, object storage, OpenFGA, or another real containerized dependency boundary must add or update the narrowest applicable Testcontainers integration test unless the slice explicitly requires Docker Compose, kind, browser, or external-provider verification instead. If Testcontainers is not applicable, state why in the PR and name the replacement signal.
- **HTTP Integration Tests for Handler Changes:** Any change that adds a new code path to an existing HTTP handler — new routing logic, a new early-return branch, a fallback, or changed response shape — must include an HTTP integration test in `services/query-api/tests/http_api_integration.rs` (or a focused sibling file) that exercises the new path end-to-end via `tower::ServiceExt::oneshot`. Unit tests for pure functions are not sufficient on their own; the handler path itself must be covered so that refactoring or reordering handler logic is caught by the test suite. Use `mode: "interpret"` to avoid needing ClickHouse when the new path exits before query execution.
- **Clarity Above All:** Nothing can be left unclear. If instructions, requirements, or code changes are ambiguous, the agent must seek clarification before proceeding.
- **Specification Alignment:** All changes must align with the core architectural principles and specifications defined in the `spec/` directory.
- **Implementation Plan Adherence:** All tasks must follow the latest implementation plans and iteration documents located in `docs/superpowers/plans/`.
- **Finished Plan Archiving:** When a detailed task plan is completed, move that finished plan from `docs/superpowers/plans/` to `archived/plans/` in the same iteration and update all active-plan and agent-context links that pointed at it.
- **ADR and Spec Synchronization:** Any change to architecture, technology choices, deployment model, data model, security model, or roadmap scope must update both the relevant ADRs and the affected specs in the same iteration. If no ADR change is needed, state why in the PR.

Refer to `spec/10-process.md` for the official development process and AI agent guidance.

## Agent Role Model

This repository uses a coordinator-plus-specialists advisory role model to reduce context noise.
See `.github/agents/README.md` for routing rules, escalation triggers, and role definitions.
When starting a new task or orchestrating work across multiple surfaces, use the **Coordinator** agent
defined in `.github/agents/coordinator.agent.md`.

Any AI agent working in this repository must treat `.github/agents/README.md` as the routing index
for repository role guidance. For new tasks, first load `.github/agents/coordinator.agent.md`. When a
task matches a routing rule, load the relevant specialist `.agent.md` file and follow it as the active
role prompt or review checklist. If the runtime supports subagents, invoke the specialist as a
subagent; otherwise, apply the specialist instructions manually in the current session.

## Phase Plan Status

- **Phase 1 is closed:** treat `archived/plans/2026-04-17-phase1-internal-mvp.md` as a historical implementation record, not as an active backlog to reopen or continue.
- **Active roadmap work starts after Phase 1:** use `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` for follow-on slices unless the user explicitly asks to revise the historical Phase 1 document. Keep `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md` as the historical Phases 2-8 closure reference and `archived/plans/2026-05-09-finish-started-work-plan-rf0-complete.md` as the started-work closure record.
- If a Phase 1 item appears unfinished in the old plan text, check the reconciliation notes in that document and the follow-on slices in the Phases 2–8 plan before proposing or making changes.

## GitHub Issues Workflow

This is the primary entry point for backlog work. Multiple agent instances can run concurrently —
each claims one issue and drives it independently to a merged PR.

### Step-by-step

1. **Scan** open, unassigned issues:
   ```
   gh issue list --assignee="" --state=open --limit=50
   ```
   Prefer `bug` labels over `enhancement` over unlabelled.

2. **Claim** the issue before doing anything else:
   ```
   gh issue edit <NUMBER> --add-assignee @me --add-label "in-progress"
   ```
   Self-assignment is the concurrency lock. If the issue was already assigned between scan and
   claim, pick the next one.

3. **Branch** immediately and push so the branch is visible:
   ```
   git checkout -b fix/issue-<NUMBER>-<slug>   # bugs
   git checkout -b feat/issue-<NUMBER>-<slug>  # features
   git push -u origin <branch-name>
   ```

4. **For bugs — write the failing test first.**
   Commit the reproducing test before writing any fix. The commit message must identify the issue:
   ```
   git commit -m "test(issue-<NUMBER>): reproduce <description>"
   ```
   Then fix the bug. The test must pass without modification after the fix.

5. **For features** — write tests covering the acceptance criteria, then implement.

6. **Run local CI** before pushing any code: `bash scripts/local-ci.sh`
   Fix every failure. No exceptions (docs-only changes are exempt per the CI section below).

7. **Open a PR** that closes the issue:
   ```
   gh pr create --title "fix(issue-<NUMBER>): ..." --body "Closes #<NUMBER> ..."
   ```
   Remove `in-progress`, add `ready-for-review` on the issue.

Full role prompt with all constraints: `.github/agents/issue-worker.agent.md`

## Before Starting Any Implementation Task

1. **Read `spec/adr/README.md`** to scan the one-line decision summaries. Open and read in full any ADR whose domain overlaps with the task.
2. **Read `docs/agent-context.md`** for the current living codebase map, active source-of-truth pointers, and agent-maintained gotchas. This guide does not replace inspecting the actual files relevant to the task.
3. **Inspect the actual code every time** before editing. Read the relevant implementation, tests, scripts, specs, and docs for the requested slice; do not rely only on summaries or prior memory.
4. **Keep the agent context current:** if the change affects repo layout, ownership boundaries, active roadmap guidance, required verification, architectural assumptions, or future agent gotchas, update `docs/agent-context.md` in the same PR. If no update is needed, state why in the PR description.
5. **Use the latest stable versions** of all dependencies:
   - **Rust crates:** check [crates.io](https://crates.io) for the current stable version before adding or updating a dependency.
   - **npm packages:** check [npmjs.com](https://www.npmjs.com) for the current stable version before adding or updating a dependency.
   - **GitHub Actions:** use the latest release tag of every action (e.g. `actions/checkout@v4`); check the action's release page if uncertain.
   - **Docker images (Compose/local):** pin to `image:major.minor` at minimum. For production Dockerfiles and base images, use `image:major.minor.patch`; SHA digest is strongly preferred.
   - Do not pin to an older version without a documented reason in the PR description.

## MANDATORY: Before Pushing ANY Code

You **MUST** run `bash scripts/local-ci.sh` before pushing **ANY** code changes. No exceptions. GitHub CI is disabled — do not push and rely on it to catch errors.

**Note:** Pure documentation changes (files under `docs/`, `spec/`, or any `.md` files) are exempt.

For any Rust code change, run `cargo fmt --all` explicitly before pushing, even though `scripts/local-ci.sh` also runs Rust formatting. Fix formatting drift before staging or pushing.

`scripts/local-ci.sh` runs: Rust fmt, clippy, tests, frontend typecheck/lint/build/test, Helm lint, Docker image build, and smoke test.

Use flags to skip stages when Docker or Node are unavailable:
- `--skip-docker` — skip image build and smoke test
- `--skip-frontend` — skip all npm checks
- `--skip-helm` — skip Helm chart lint
- `--skip-smoke` — build image but skip smoke test

If any check fails, you **MUST** fix it before pushing.

## Regression Gate Stewardship

- Treat `scripts/local-ci.sh`, `tests/e2e/smoke_test.sh`, `scripts/perf-smoke.sh`, and Docker Compose verification services as protected regression gates.
- Before changing a regression gate, state the current coverage it provides and the exact coverage that will exist after the change.
- Never delete, weaken, skip, or quarantine a regression assertion unless the PR includes a replacement signal, linked issue, owner, expiry date, and explicit reviewer approval.
- Regression-gate changes must preserve existing build and product functionality. Run the narrowest affected check first, then the required local gate for the touched surface.
- Testcontainers tests are protected regression signals once introduced. Do not replace them with mocks, shared local databases, or broad smoke tests unless the PR explains the lost coverage and includes a reviewer-approved replacement.
- Performance-sensitive changes must run `docker compose up perf-smoke --abort-on-container-exit` or explain why the performance gate is not relevant.

## NLQ Quality Gate

Any change that affects the NLQ→IR→SQL pipeline — the system prompt, IR schema (`NlqIr`),
SQL templates, metadata injection, IR parser, repair loop, or eval test cases — must:

1. Include or update cases in `tests/nlq/cases.json` covering the changed behavior.
2. Run `python3 scripts/nlq-eval.py` against a running cluster and record the pass/fail
   summary in the PR description.
3. Show that the changed behavior now passes and no previously-passing case has regressed.

The eval harness is a protected regression gate. Do not weaken assertions without a
replacement signal and reviewer approval. See `spec/08-ai-ml.md §13.4` for the full
operation reference, design rationale, and feedback loop.

## CI and Scripts

- All non-trivial CI logic must live in `scripts/` and be runnable locally (see ADR-019).
- Migrations and smoke tests are handled automatically by Docker Compose.
- Run `docker compose up -d` to start the system and run migrations.
- Run `docker compose up smoke-test --abort-on-container-exit` to verify.
