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

## MANDATORY: Before Pushing ANY Code

You **MUST** run the following checks before pushing **ANY** code changes to the repository. No exceptions. Do not push and rely on CI to catch errors.

You can run `make ci` to execute steps 1-3 and build the final Docker image in one command.

1. Run `cargo fmt --all` — fix all formatting issues.
2. Run `cargo clippy --all-targets --all-features -- -D warnings` — fix all warnings.
3. Run `cargo test --all-targets --all-features` — ensure all tests pass.
4. If Docker is available:
   - Run `docker compose up -d` to ensure the stack is running.
   - Run `docker compose up smoke-test --abort-on-container-exit` — all checks MUST pass.

If any check fails, you **MUST** fix it before pushing.
