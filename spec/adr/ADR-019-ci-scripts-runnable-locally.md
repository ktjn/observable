# ADR-019: CI Workflows Must Delegate to Locally-Runnable Scripts

**Date:** 2026-04-18
**Status:** Accepted
**Authors:** Tommy Alander
**Deciders:** Tommy Alander
**Review date:** 2027-04-18

## Context

GitHub Actions workflows have a tendency to accumulate inline shell logic — multi-line `run:` blocks with tool invocations, loops, and conditionals written directly in YAML. This creates three concrete problems:

1. **No local reproduction.** When a nightly job fails, the only way to debug it is to push a commit and wait for the runner. There is no `make nightly` or equivalent.
2. **Duplication drift.** The same migration loop appeared in the `Makefile` (using `clickhouse-client`) and would need to be reproduced in CI. When one copy changes, the other rots.
3. **Fragile host-tool assumptions.** Inline `run:` steps that call host-installed tools (`clickhouse-client`, `sqlx`) fail silently when those tools are absent on a new runner image, as happened with the nightly smoke test on 2026-04-18.

## Decision

All non-trivial logic in GitHub Actions `run:` steps must live in a versioned shell script under `scripts/`. Workflow YAML steps invoke those scripts with a single line (`bash scripts/foo.sh`). Steps that are truly one-liners (`cargo build --release`, `npm ci`) may remain inline.

Concretely:
- `scripts/migrate.sh` — runs all ClickHouse and PostgreSQL migrations via `docker compose exec`, requiring no host-installed database clients.
- `scripts/start-services.sh` — starts compiled service binaries in the background with a configurable build directory.
- `tests/e2e/smoke_test.sh` — end-to-end pipeline validation (already in place).

The `Makefile` `migrate` target calls `scripts/migrate.sh` so that `make dev && make migrate` remains the local dev workflow.

## Consequences

**Easier:**
- Any developer can reproduce a CI failure locally with `bash scripts/<name>.sh`.
- Script logic is testable, diffable, and searchable like any other code.
- New CI jobs (PR checks, release pipelines) compose from the same scripts without copy-paste.

**Harder:**
- Scripts need to be written defensively (`set -euo pipefail`, explicit paths) so they work both in the repo root and from CI working directories.
- Script authors must consider both local (host tools available, `.env.local` present) and CI (Docker-only, no extra CLI tools) execution contexts.

**Constrained:**
- Host-installed CLI tools (`clickhouse-client`, `sqlx-cli`) must not be assumed present on CI runners. Migrations and other database operations must go through `docker compose exec`.

## Alternatives Considered

### Option A: Install missing tools in CI via apt-get / cargo install
Install `clickhouse-client` and `sqlx-cli` in the workflow before use. Rejected: adds minutes to every run, ties CI to specific tool versions not tracked in the repo, and doesn't solve the reproducibility problem — developers still can't run the exact CI steps locally.

### Option B: Keep all logic in YAML, document runner requirements
Document which tools must be pre-installed on self-hosted runners. Rejected: self-hosted runners are not in scope for Phase 1, and the problem of non-local-reproducibility remains entirely unaddressed.

## Related

- `scripts/migrate.sh` — implements this decision for migrations
- `scripts/start-services.sh` — implements this decision for service startup
- `tests/e2e/smoke_test.sh` — pre-existing example of the pattern
- `.github/workflows/nightly.yml` — first workflow refactored to comply
