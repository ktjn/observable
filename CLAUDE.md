# Agent Instructions

These instructions are foundational mandates for any AI agent interacting with this repository.

## Core Mandates

- **No Unreviewed Merges:** Nothing can be merged or committed to the main branch without a human review.
- **Branch and PR Every Iteration:** Before changing files, create or switch to a dedicated short-lived branch for the current task. Commit work only to that branch, push it to GitHub, and open a pull request for every iteration.
- **Verification & Testing:** Every change must be thoroughly tested and verified before being considered complete.
- **Clarity Above All:** Nothing can be left unclear. If instructions, requirements, or code changes are ambiguous, the agent must seek clarification before proceeding.
- **Specification Alignment:** All changes must align with the core architectural principles and specifications defined in the `spec/` directory.
- **Implementation Plan Adherence:** All tasks must follow the latest implementation plans and iteration documents located in `docs/superpowers/plans/`.
- **ADR and Spec Synchronization:** Any change to architecture, technology choices, deployment model, data model, security model, or roadmap scope must update both the relevant ADRs and the affected specs in the same iteration. If no ADR change is needed, state why in the PR.

Refer to `spec/10-process.md` for the official development process and AI agent guidance.

## Before Starting Any Implementation Task

1. **Read `spec/adr/README.md`** to scan the one-line decision summaries. Open and read in full any ADR whose domain overlaps with the task.
2. **Use the latest stable versions** of all dependencies:
   - **Rust crates:** check [crates.io](https://crates.io) for the current stable version before adding or updating a dependency.
   - **npm packages:** check [npmjs.com](https://www.npmjs.com) for the current stable version before adding or updating a dependency.
   - **GitHub Actions:** use the latest release tag of every action (e.g. `actions/checkout@v4`); check the action's release page if uncertain.
   - **Docker images (Compose/local):** pin to `image:major.minor` at minimum. For production Dockerfiles and base images, use `image:major.minor.patch`; SHA digest is strongly preferred.
   - Do not pin to an older version without a documented reason in the PR description.

## MANDATORY: Before Pushing ANY Code

You **MUST** run `bash scripts/local-ci.sh` before pushing **ANY** code changes. No exceptions. GitHub CI is disabled — do not push and rely on it to catch errors.

**Note:** Pure documentation changes (files under `docs/`, `spec/`, or any `.md` files) are exempt.

`scripts/local-ci.sh` runs: Rust fmt, clippy, tests, frontend typecheck/lint/build/test, Docker image build, and smoke test.

Use flags to skip stages when Docker or Node are unavailable:
- `--skip-docker` — skip image build and smoke test
- `--skip-frontend` — skip all npm checks
- `--skip-smoke` — build image but skip smoke test

If any check fails, you **MUST** fix it before pushing.

## Regression Gate Stewardship

- Treat `scripts/local-ci.sh`, `tests/e2e/smoke_test.sh`, `scripts/perf-smoke.sh`, and Docker Compose verification services as protected regression gates.
- Before changing a regression gate, state the current coverage it provides and the exact coverage that will exist after the change.
- Never delete, weaken, skip, or quarantine a regression assertion unless the PR includes a replacement signal, linked issue, owner, expiry date, and explicit reviewer approval.
- Regression-gate changes must preserve existing build and product functionality. Run the narrowest affected check first, then the required local gate for the touched surface.
- Performance-sensitive changes must run `docker compose up perf-smoke --abort-on-container-exit` or explain why the performance gate is not relevant.

## CI and Scripts

- All non-trivial CI logic must live in `scripts/` and be runnable locally (see ADR-019).
- Migrations and smoke tests are handled automatically by Docker Compose.
- Run `docker compose up -d` to start the system and run migrations.
- Run `docker compose up smoke-test --abort-on-container-exit` to verify.
