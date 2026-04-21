# Dependency Maintenance Policy — Design

**Date:** 2026-04-21  
**Status:** Approved  
**Scope:** `spec/10-process.md` §16.10 (new section) + `CLAUDE.md` (Docker image guidance)

## Problem

The spec and CLAUDE.md cover dependency usage at add-time (use latest stable, check crates.io/npmjs.com) but say nothing about ongoing maintenance: how deps are pinned, how often they are updated, who owns breaking upgrades, and how quickly CVEs must be addressed.

Docker images in `docker-compose.yml` have inconsistent pinning (`postgres:16`, `clickhouse/clickhouse-server:24.3`, `redpandadata/redpanda:v23.3.1`) with no documented rationale or policy.

## Approach

Add `§16.10 Dependency Maintenance Policy` to `spec/10-process.md`. Update `CLAUDE.md` to add Docker image guidance alongside the existing Rust/npm rules. No new ADR — this is operational policy, not an architectural decision.

## Design

### Pinning Rules

| Ecosystem | Cargo.toml / package.json | Lockfile | Production images |
|---|---|---|---|
| Rust crates | `^major.minor` range | `Cargo.lock` committed — this is the exact pin | n/a |
| npm packages | `^major` range | `package-lock.json` committed — this is the exact pin | n/a |
| Docker Compose (local/dev) | `image:major.minor` minimum | n/a | n/a |
| Production Dockerfiles / base images | `image:major.minor.patch` | SHA digest strongly preferred | SHA digest |
| GitHub Actions | `action@vN` (latest major tag) | n/a | n/a |

Lockfiles are always committed. Range specifiers without committed lockfiles are not allowed.

### Update Cadence

- **Routine:** monthly sweep — bump all deps to latest stable within declared range, run `bash scripts/local-ci.sh`, open a dedicated PR.
- **Security (critical/high CVE):** 7-day SLA from public disclosure. Patch-only bumps skip the monthly cycle.
- **Security (medium CVE):** 30-day SLA.
- **Breaking upgrades** (major version bumps, image EOL): treated as a feature slice — requires source spec reference, acceptance target, and rollback note in the PR. Not bundled with routine updates.

### Automation

Dependabot or Renovate is the preferred tool for surfacing routine update PRs. Configuration lives in `.github/dependabot.yml` or `renovate.json`. Not required before Phase 2, but this is the target state.

### Ownership

- PR author verifies the update doesn't break `local-ci.sh` before pushing — no exceptions.
- Routine dependency PRs must state: what changed, whether local-ci passed, and whether any lockfile drift was introduced.

### Files Changed

1. `spec/10-process.md` — add `§16.10 Dependency Maintenance Policy`
2. `CLAUDE.md` — add Docker image guidance under "Before Starting Any Implementation Task"
