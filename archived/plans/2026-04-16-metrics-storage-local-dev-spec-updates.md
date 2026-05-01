# Metrics Storage Decision and Local Dev Story — Spec Updates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Update three spec/ADR documents to commit the metrics storage decision to ClickHouse and define the local development workflow.

**Architecture:** Documentation-only changes. No code is written. Three files change: `spec/03-storage.md` (remove metrics storage hedge), `spec/adr/ADR-003-clickhouse-boundary.md` (remove "initially" qualifier, add revisit condition), and `spec/12-deployment.md` (add §19.6 Local Development).

**Tech Stack:** Markdown, Git

---

## File Map

| File | Change |
|------|--------|
| `spec/03-storage.md` | Replace "Two valid options" metrics section with committed ClickHouse decision and revisit condition |
| `spec/adr/ADR-003-clickhouse-boundary.md` | Remove "initially" qualifier from metrics; add explicit revisit condition |
| `spec/12-deployment.md` | Add §19.6 Local Development after §19.5 |

---

## Task 1: Update spec/03-storage.md — commit metrics to ClickHouse

**Files:**
- Modify: `spec/03-storage.md` (lines 13–27, the Metrics subsection of §5.1)

- [ ] **Step 1: Open the file and locate the metrics section**

  In `spec/03-storage.md`, find the `**Metrics**` block inside `### 5.1 Recommended Storage Pattern`. It currently reads:

  ```markdown
  **Metrics**

  Use a time-series optimized engine with:
  - high ingest throughput
  - compression
  - rollups/downsampling
  - histogram support
  - exemplars
  - native label filtering

  Two valid options:
  - keep metrics in ClickHouse if you want fewer moving parts
  - use a dedicated TSDB if you want metric-specific economics and query semantics
  ```

- [ ] **Step 2: Replace the metrics section with the committed decision**

  Replace the entire `**Metrics**` block (from `**Metrics**` through the end of the "Two valid options" bullet list) with:

  ```markdown
  **Metrics**

  Use ClickHouse for metrics in Phase 1 and until a concrete performance or cardinality constraint justifies a dedicated TSDB. ClickHouse supports all required metric workload characteristics:
  - high ingest throughput
  - columnar compression
  - rollups/downsampling via materialized views
  - histogram and exponential histogram storage
  - exemplar support
  - label filtering via WHERE clauses on attribute columns

  **Revisit condition:** If Phase 2 or Phase 3 cardinality testing reveals that ClickHouse cannot meet the P50 < 1 s query target under production-representative label cardinality, open a new ADR to evaluate a dedicated TSDB (e.g., VictoriaMetrics). The query facade abstracts storage engines from clients, so a later migration is contained.
  ```

- [ ] **Step 3: Verify consistency**

  Read `spec/14-domain-model.md` §2 (`MetricSeries` and `MetricPoint` schemas) and confirm the storage spec references are consistent — the domain model already assumes ClickHouse-compatible column layout. No further changes needed there.

- [ ] **Step 4: Commit**

  ```bash
  git add spec/03-storage.md
  git commit -m "docs: commit metrics storage to ClickHouse in Phase 1

  Removes the 'two valid options' hedge. Adds explicit revisit condition
  tied to Phase 2/3 cardinality testing against the P50 < 1s query target.

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 2: Update ADR-003 — remove ambiguity around metrics

**Files:**
- Modify: `spec/adr/ADR-003-clickhouse-boundary.md`

- [ ] **Step 1: Open the file and locate the Decision section**

  In `spec/adr/ADR-003-clickhouse-boundary.md`, find the `## Decision` section. It currently reads:

  ```markdown
  ## Decision

  ClickHouse will be the **primary engine for all high-volume telemetry data (logs, traces, and initially metrics)**.
  - It will NOT be used for transactional metadata, user accounts, or fine-grained configuration (which belong in PostgreSQL).
  - We will adopt a "ClickHouse-first" approach for data plane services but maintain a strict service boundary so that the storage engine can be swapped or augmented if necessary.
  ```

- [ ] **Step 2: Replace the Decision section**

  Replace the entire `## Decision` section with:

  ```markdown
  ## Decision

  ClickHouse is the **primary engine for all high-volume telemetry data: logs, traces, and metrics**.

  - It will NOT be used for transactional metadata, user accounts, or fine-grained configuration (those belong in PostgreSQL).
  - We adopt a "ClickHouse-first" approach for data plane services and maintain a strict service boundary so the storage engine can be swapped or augmented if necessary.
  - Metrics use the `MetricSeries` + `MetricPoint` table design defined in `spec/14-domain-model.md`. Rollups are implemented as ClickHouse materialized views.

  **Revisit condition for metrics:** If Phase 2 or Phase 3 cardinality testing shows ClickHouse cannot meet the P50 < 1 s query latency target under production-representative label cardinality, open a new ADR to evaluate a dedicated TSDB (e.g., VictoriaMetrics). The query facade already abstracts storage engines from clients.
  ```

- [ ] **Step 3: Update the Related section**

  Add a reference to `spec/14-domain-model.md` in the `## Related` section at the bottom of the file:

  Current:
  ```markdown
  ## Related

  - `spec/03-storage.md` (Storage Strategy)
  - `spec/adr/ADR-002: Polyglot Storage vs Single Engine`
  ```

  Replace with:
  ```markdown
  ## Related

  - `spec/03-storage.md` (Storage Strategy)
  - `spec/14-domain-model.md` (MetricSeries and MetricPoint schemas)
  - `spec/adr/ADR-002-polyglot-storage.md` (Polyglot Storage vs Single Engine)
  ```

- [ ] **Step 4: Commit**

  ```bash
  git add spec/adr/ADR-003-clickhouse-boundary.md
  git commit -m "docs: remove 'initially' qualifier from ADR-003 metrics decision

  Metrics are now a committed part of the ClickHouse boundary, not a
  provisional 'initial' choice. Adds revisit condition and MetricSeries
  schema reference.

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 3: Add §19.6 Local Development to spec/12-deployment.md

**Files:**
- Modify: `spec/12-deployment.md` (insert after §19.5 Disaster Recovery, before `## 20. Tooling`)

- [ ] **Step 1: Open the file and locate the insertion point**

  In `spec/12-deployment.md`, find the end of `### 19.5 Disaster Recovery`:

  ```markdown
  ### 19.5 Disaster Recovery

  - multi-AZ mandatory
  - multi-region optional at first release
  - restore drills quarterly
  - RPO/RTO defined per retention tier

  ---

  ## 20. Tooling and Framework Recommendations
  ```

- [ ] **Step 2: Insert §19.6 between §19.5 and §20**

  Insert the following block between the `---` separator after §19.5 and `## 20. Tooling`:

  ```markdown
  ### 19.6 Local Development

  Local development uses Docker Compose for all external dependencies and Rust services. The React frontend runs natively on the developer's machine.

  **Quick start**

  ```bash
  make dev                       # start Docker Compose stack (including services and migrations)
  npm run dev                    # run the React frontend (from apps/frontend)
  ```

  For advanced control, individual steps are available:
  - `bash scripts/migrate.sh` — explicitly run ClickHouse/Postgres setup and migrations.
  - `bash scripts/start-services.sh` — explicitly start Rust services.

  **Dependency services**

  | Service    | Image                        | Ports      | Purpose                      |
  |------------|------------------------------|------------|------------------------------|
  | clickhouse | clickhouse/clickhouse-server | 8123, 9000 | Telemetry store              |
  | redpanda   | redpandadata/redpanda        | 9092, 9644 | Durable queue / stream       |
  | postgres   | postgres:16                  | 5432       | Control plane metadata store |
  | openfga    | openfga/openfga              | 8080       | Fine-grained auth store      |

  **Application services**

  | Service          | Ports | Purpose              |
  |------------------|-------|----------------------|
  | auth-service     | 4318  | Tenant validation    |
  | ingest-gateway   | 4317  | OTLP ingest endpoint |
  | storage-writer   | 4320  | ClickHouse write API |
  | stream-processor | n/a   | Redpanda consumer    |
  | query-api        | 8090  | Telemetry query API  |

  **Configuration**

  - Copy `.env.local.example` (committed) to `.env.local` (gitignored) at the repo root.
  - Each Rust service reads config from environment variables supplied by Docker Compose. `.env.local` supplies local defaults pointing to the Compose services above.
  - No production secrets are required for local development.

  **Schema migrations**

  - In local mode, migrations run automatically via `docker-compose.yml` setup containers when the stack starts.
  - For manual execution, run `bash scripts/migrate.sh` after starting the Compose dependency stack.
  - In CI and production, migrations are explicit pipeline steps and do not run automatically on service startup.

  **Rules**

  - `docker compose up` must start cleanly from scratch with no manual seed steps. Setup and migrations are automated via `clickhouse-setup`, `postgres-setup`, and `redpanda-setup` containers.
  - Do not bake credentials into `docker-compose.yml`; read all values from environment variables or `.env.local`.
  - Local ports must not conflict across services: ClickHouse 8123/9000, Redpanda 9092/9644, Postgres 5432, OpenFGA 8080, auth-service 4318, ingest-gateway 4317, storage-writer 4320, query-api 8090.
  - `make dev` must be documented in the repo root README as the single starting point for new contributors.
  ```

- [ ] **Step 3: Verify the section renders correctly**

  Read the updated `spec/12-deployment.md` and confirm:
  - §19.6 appears between §19.5 and §20 with correct heading level (`###`)
  - The port table has no formatting issues
  - No existing content was accidentally removed

- [ ] **Step 4: Commit**

  ```bash
  git add spec/12-deployment.md
  git commit -m "docs: add local development workflow to deployment spec

  Defines Docker Compose dependency and application services, quick start commands,
  configuration via .env.local, migration flag convention, and
  canonical local port assignments.

  Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
  ```

---

## Task 4: Open pull request

- [ ] **Step 1: Push the branch**

  ```bash
  git push -u origin docs/metrics-storage-local-dev
  ```

- [ ] **Step 2: Open the PR**

  ```bash
  gh pr create \
    --title "docs: commit metrics storage to ClickHouse and define local dev workflow" \
    --body "$(cat <<'EOF'
  ## Summary

  Resolves two open gaps identified in spec review pass (see design doc: `docs/superpowers/specs/2026-04-16-metrics-storage-and-local-dev-design.md`):

  - **spec/03-storage.md**: Removes the \"two valid options\" hedge for metrics storage. Commits to ClickHouse for Phase 1 with an explicit revisit condition tied to Phase 2/3 cardinality testing.
  - **spec/adr/ADR-003-clickhouse-boundary.md**: Removes the \"initially\" qualifier. Metrics are now a committed part of the ClickHouse boundary. Adds revisit condition and MetricSeries schema reference.
  - **spec/12-deployment.md**: Adds §19.6 Local Development — Docker Compose dependency and application services, quick start commands, configuration via `.env.local`, migration flag convention, and canonical local port assignments.

  ## ADR/spec sync

  ADR-003 and spec/03-storage.md are updated together in this PR. No new ADR is required — this PR ratifies the existing ADR-003 decision by removing residual ambiguity.

  ## Verification

  Slice type: Spec/ADR only — minimum verification is diff review and Markdown consistency check.

  ```
  Checks run: manual diff review, cross-reference check against spec/14-domain-model.md
  Baseline: n/a (doc-only change)
  Result after change: consistent — ADR-003, spec/03-storage.md, and spec/14-domain-model.md all agree on ClickHouse for metrics
  New errors introduced: none
  Skipped checks: no automated Markdown link checker configured yet
  Known pre-existing failures: none
  Follow-up test debt: none
  ```

  ## Next slice

  Gap #2 from review (queue/stream choice — ratify ADR-009 with a concrete tool selection) or begin Phase 1 monorepo scaffold.
  EOF
  )"
  ```

- [ ] **Step 3: Confirm PR is open and linked to the branch**

  ```bash
  gh pr view --web
  ```

---

## Self-Review

**Spec coverage check:**
- Design doc §Decision 1 (ClickHouse for metrics) → covered by Tasks 1 and 2 ✓
- Design doc §Decision 2 (Docker Compose local dev) → covered by Task 3 ✓
- Design doc §Affected files table → all three files have tasks ✓

**Placeholder scan:** No TBDs, TODOs, or vague instructions present.

**Consistency check:**
- `make dev` is referenced in Task 3 and mentioned as needing a README note — the README note is inside the §19.6 spec text itself ("must be documented in the repo root README"). The actual README update is out of scope for this doc-only pass and correctly deferred to the monorepo scaffold task.
- ADR-003 Related section now uses consistent filename format (`ADR-002-polyglot-storage.md`) matching the actual file `spec/adr/ADR-002-polyglot-storage.md`.
- Port assignments in §19.6 are consistent with no known conflicts.
