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
