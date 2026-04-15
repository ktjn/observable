# Agent Instructions

These instructions are foundational mandates for any AI agent interacting with this repository.

## Core Mandates

- **No Unreviewed Merges:** Nothing can be merged or committed to the main branch without a human review.
- **Branch and PR Every Iteration:** Before changing files, create or switch to a dedicated short-lived branch for the current task. Commit work only to that branch, push it to GitHub, and open a pull request for every iteration.
- **Verification & Testing:** Every change must be thoroughly tested and verified before being considered complete.
- **Clarity Above All:** Nothing can be left unclear. If instructions, requirements, or code changes are ambiguous, the agent must seek clarification before proceeding.
- **Specification Alignment:** All changes must align with the core architectural principles and specifications defined in the `spec/` directory.

## Required Iteration Workflow

1. Inspect `git status --short --branch` before editing.
2. Create or switch to a short-lived branch named for the current task before changing files.
3. If the task depends on an unmerged agent PR, branch from that PR branch and open a stacked PR against it.
4. Keep edits scoped to the task and preserve unrelated user changes.
5. Run the relevant verification for the files changed. For documentation-only changes, review the diff and state that no code tests were run.
6. Commit the completed iteration on the task branch.
7. Push the branch to GitHub and open a pull request before reporting the work complete.
8. In the final handoff, include the branch, commit, PR link, verification performed, and any blocked or skipped checks.

If pushing or PR creation is blocked by credentials, network, or repository permissions, the agent must leave the branch and commit ready locally and report the exact blocker.

Refer to `spec/10-process.md` for the official development process and AI agent guidance.
