# Contributing to Observable

Thanks for your interest in contributing. This repository follows a specific process to keep
the codebase reviewable and regression-free — please read this before opening a PR.

## Before you start

1. Read [AGENTS.md](AGENTS.md) — it defines the mandatory process for any change to this
   repository (human or AI-assisted): branch-per-iteration, verification requirements, and
   spec/ADR synchronization rules.
2. Read [spec/adr/README.md](spec/adr/README.md) for the index of architecture decisions, and
   open any ADR whose domain overlaps with your change.
3. Check [ROADMAP.md](ROADMAP.md) to see if the change you're proposing is already scoped,
   in progress, or intentionally deferred.

## Workflow

1. **Branch.** Create a short-lived branch named for your change before editing any files.
2. **Write tests first.** Bug fixes need a failing test committed before the fix. Features need
   tests covering the acceptance criteria before implementation.
3. **Implement the smallest coherent change.** Prefer a thin end-to-end slice over a broad
   partial rewrite.
4. **Run local CI before pushing any code:**
   ```bash
   bash scripts/local-ci.sh
   ```
   GitHub CI also runs on pull requests and pushes to `main`, but local-ci catches issues
   before they reach CI. See flags to skip stages (`--skip-docker`, `--skip-frontend`,
   `--skip-helm`, `--skip-smoke`) if a dependency isn't available locally, and state which
   stages you skipped in your PR description.
5. **For any Rust change**, run `cargo fmt --all` before staging and committing — Rust
   formatting is enforced inside the Docker build.
6. **Keep docs in sync.** If your change affects architecture, the data model, security, or
   roadmap scope, update the relevant ADR/spec in the same PR (or state why not).
7. **Open a PR** describing what changed, why, and how you verified it.

## Code organization

- `services/` — Rust backend services (one per bounded context: ingest, auth, query, alerting,
  admin, stream processing, storage writing).
- `libs/` — shared Rust crates.
- `apps/frontend/` — the React/TypeScript frontend. Reuse existing components under
  `apps/frontend/src/components/` and `apps/frontend/src/features/**/components/` before
  creating new ones.
- `spec/` — the product/platform specification, organized by numbered topic files, plus
  `spec/adr/` for architecture decision records.

## Getting help

Open a GitHub issue if you're unsure whether a change fits the project's direction before
investing significant effort in a PR.
