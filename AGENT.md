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

You **MUST** run `bash scripts/local-ci.sh` before pushing **ANY** code changes. No exceptions. GitHub CI is disabled — do not push and rely on it to catch errors.

`scripts/local-ci.sh` runs: Rust fmt, clippy, tests, frontend typecheck/lint/build/test, Docker image build, and smoke test.

Use flags to skip stages when Docker or Node are unavailable:
- `--skip-docker` — skip image build and smoke test
- `--skip-frontend` — skip all npm checks
- `--skip-smoke` — build image but skip smoke test

If any check fails, you **MUST** fix it before pushing.
