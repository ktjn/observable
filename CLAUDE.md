# Agent Instructions

These instructions are foundational mandates for any AI agent interacting with this repository.

## Core Mandates

- **No Unreviewed Merges:** Nothing can be merged or committed to the main branch without a human review.
- **Branch and PR Every Iteration:** Before changing files, create or switch to a dedicated short-lived branch for the current task. Commit work only to that branch, push it to GitHub, and open a pull request for every iteration.
- **Verification & Testing:** Every change must be thoroughly tested and verified before being considered complete.
- **Clarity Above All:** Nothing can be left unclear. If instructions, requirements, or code changes are ambiguous, the agent must seek clarification before proceeding.
- **Specification Alignment:** All changes must align with the core architectural principles and specifications defined in the `spec/` directory.
- **ADR and Spec Synchronization:** Any change to architecture, technology choices, deployment model, data model, security model, or roadmap scope must update both the relevant ADRs and the affected specs in the same iteration. If no ADR change is needed, state why in the PR.

Refer to `spec/10-process.md` for the official development process and AI agent guidance.

## Before Starting Any Implementation Task

1. **Read `spec/adr/README.md`** to scan the one-line decision summaries. Open and read in full any ADR whose domain overlaps with the task.
2. **Use the latest stable versions** of all dependencies:
   - **Rust crates:** check [crates.io](https://crates.io) for the current stable version before adding or updating a dependency.
   - **npm packages:** check [npmjs.com](https://www.npmjs.com) for the current stable version before adding or updating a dependency.
   - **GitHub Actions:** use the latest release tag of every action (e.g. `actions/checkout@v4`); check the action's release page if uncertain.
   - Do not pin to an older version without a documented reason in the PR description.

## Before Pushing Any Branch

Before running `git push`:

1. Run `cargo fmt --all` — fix any formatting issues before committing.
2. Run `cargo clippy --all-targets --all-features -- -D warnings` — fix all warnings.
3. Run `cargo test --all-targets --all-features` — ensure all tests pass.
4. If Docker is available and the stack is running:
   - Run `bash scripts/migrate.sh` to apply any schema changes.
   - Run `bash scripts/start-services.sh` to start all services (kill any already running first).
   - Run `bash tests/e2e/smoke_test.sh` — all checks must pass.

If any check fails, fix it before pushing. Do not push and rely on CI to catch it.

## CI and Scripts

- All non-trivial CI logic must live in `scripts/` and be runnable locally (see ADR-019).
- Migrations run via `bash scripts/migrate.sh` (uses `docker compose exec`; no host-installed DB clients required).
