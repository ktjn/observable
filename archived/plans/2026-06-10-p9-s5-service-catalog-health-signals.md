# P9-S5 Completion: Service Catalog Health Signals (active alerts, latest deploy, SLO-aware health)

> **Status:** COMPLETED 2026-06-18 — promoted from `docs/superpowers/plans/2026-06-04-observability-feature-parity-plan.md` §P9-S5.
> **For agentic workers:** REQUIRED SUB-SKILL: `superpowers:subagent-driven-development` or
> `superpowers:executing-plans`. Steps use checkbox (`- [ ]`) syntax for tracking. Each task is
> its own commit/PR per `AGENTS.md` "Branch and PR Every Iteration".

**Goal:** Finish P9-S5 ("Service Health Summary in Catalog") by replacing the two hardcoded
placeholder fields in the service catalog summary — `active_alert_count: 0` and
`latest_deployment: None` — with real data, and by making `health_state` SLO-aware as the
acceptance criteria require.

---

## 1. Why This Slice, Why Now

### Repo state check (2026-06-10)

- The active roadmap is `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`, extended by
  `docs/superpowers/plans/2026-06-04-observability-feature-parity-plan.md` (Phases P9-P14).
- `P9-S1` (Onboarding Wizard) shipped via PR #397.
- The modelable type-mapping migration (`2026-06-08-modelable-type-mapping-migration-plan.md`)
  is mid-flight on its pilot domain (PR #399, "Phase 2 step 1-2"); steps 1.4-1.8 and 2.4 onward
  are blocked on an upstream `1.2a` scope decision in `github.com/ktjn/modelable`. There is
  nothing actionable here in this repo until that upstream decision lands — do not restart this
  track speculatively (see that plan's own warning).
- Two open issues, **#388 (Trace Comparison)** and **#389 (Query Workbench)**, describe work that
  is **already implemented and merged** (`apps/frontend/src/pages/TraceCompare*.tsx`,
  `apps/frontend/src/features/workbench/`, `apps/frontend/src/pages/QueryWorkbenchPage.tsx`, both
  referenced as complete in `docs/agent-context.md`). These should be closed as already-done in a
  short follow-up — not part of this plan's scope, called out here so it isn't lost.

### P9-S5 is partially done, not done

`P9-S5: Service Health Summary in Catalog` (feature-parity plan §3, Phase P9, "Quick Win") asks for:

- a service health summary row in the Service Catalog (error rate, p95 latency, request rate,
  SLO status badge), and
- **"Service health status derived automatically from SLO burn rate if an SLO exists, or from
  error rate threshold if not."**

Looking at the actual code:

- `apps/frontend/src/pages/ServicesPage.tsx` already renders the full health summary: stat cards
  (`Services`, `Active Alerts`, `Avg P95`, `Avg Error Rate`), a sortable/filterable table with
  `HealthStatus`, `ErrorRateCell`, `LatencyCell`, an `Active Alerts` column, and a `Latest Deploy`
  column. **The UI side of P9-S5 is done.**
- `services/query-api/src/discovery.rs` already defines the full `ServiceSummary` shape
  (`health_state`, `active_alert_count`, `latest_deployment`) and `spec/09-api.md` §"Service
  Detail Summary" already documents these as current response fields.
- But `service_summary_from_row()` (discovery.rs:614-630) **hardcodes**:
  ```rust
  active_alert_count: 0,
  latest_deployment: None,
  ```
  and `health_state()` (discovery.rs:632-640) is purely `error_rate`-threshold based — it never
  considers SLO burn-rate state, so the "derived from SLO burn rate if an SLO exists" half of the
  acceptance criteria is unmet.

So today the Service Catalog always shows 0 active alerts and `--` for latest deploy, regardless
of reality, and a service with a healthy error rate but an actively-firing SLO burn-rate alert
shows as "Healthy". This is a misleading "is my service healthy right now" signal — exactly the
workflow P9-S5 exists to fix (feature-parity plan §2.3).

This is a small, self-contained, additive change: no new tables, no new ingest path, one
ClickHouse-querying handler gets a Postgres enrichment step using patterns that already exist
elsewhere in this codebase (`slos.rs::list_slos`'s `EXISTS`/`alert_firings` join,
`deployments.rs::list_deployments`'s `deployment_markers` query).

---

## 2. Design

### 2.1 Scope and known limitation (state this in the PR)

`alert_rules` has no `service_name`/`environment` column (documented in `docs/agent-context.md`
as a known simplification — the same reason `dedup_key` is `rule_id`-only today). A general
"count all active alerts for this service" is therefore not possible without a schema migration,
which is out of scope for this slice.

**This slice scopes `active_alert_count` and the SLO-aware part of `health_state` to alerts
reachable via `slo_definitions.service_name`** — i.e. `alert_type = 'slo_burn_rate'` rules whose
`condition->>'slo_id'` matches an SLO for that service (the same join `slos.rs::list_slos` already
uses). Threshold/composite alerts not tied to an SLO are not counted. Document this explicitly in
the PR description and as a doc-comment in the new code, mirroring the existing
`docs/agent-context.md` "Known simplification" note for `dedup_key`. A follow-up to add
`alert_rules.service_name`/`environment` (enabling a full count) should be filed as a separate
backlog item, not bundled here.

### 2.2 Health state rule

- If the service has **no** `slo_definitions` row (for the requested environment filter, if any):
  `health_state` = existing error-rate-threshold result (unchanged behavior).
- If the service has **at least one** `slo_definitions` row and **any** of them has an active
  `slo_burn_rate` firing: `health_state` = `"breach"` (SLO breach overrides the error-rate
  result — an SLO breach is the stronger signal).
- If the service has SLO definitions but **none** are firing: `health_state` = existing
  error-rate-threshold result (SLO existing-but-healthy doesn't suppress an error-rate-driven
  "watch"/"breach" — error rate remains a useful secondary signal).

This matches "derived from SLO burn rate if an SLO exists, or from error rate threshold if not"
without discarding the error-rate signal when the SLO itself is fine.

### 2.3 Latest deployment

`latest_deployment` becomes the `service_version` of the most recent `deployment_markers` row for
that `service_name` (optionally filtered by `environment`, matching the existing `environment`
query param), ordered by `started_at DESC`. This matches what `ServicesPage.tsx`'s "Latest
Deploy" column expects to render (a short version string, e.g. `v1.2.0`).

### 2.4 Where the enrichment lives

Add one new Postgres query function in `discovery.rs` (co-located with `service_summary_from_row`,
since both are catalog-summary concerns):

```rust
struct ServiceCatalogEnrichment {
    active_alert_count: u64,
    slo_breaching: bool,
    latest_deployment: Option<String>,
}

async fn fetch_service_catalog_enrichment(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    environment: Option<&str>,
) -> Result<HashMap<String, ServiceCatalogEnrichment>, sqlx::Error>
```

Two queries inside it (both filtered by `tenant_id` and optional `environment`):

1. **SLO + active alert count per service:**
   ```sql
   SELECT sd.service_name,
          COUNT(af.firing_id) FILTER (WHERE af.state = 'active') AS active_alert_count,
          BOOL_OR(af.state = 'active') AS slo_breaching
   FROM slo_definitions sd
   LEFT JOIN alert_rules ar
       ON ar.tenant_id = sd.tenant_id
      AND ar.alert_type = 'slo_burn_rate'
      AND ar.condition->>'slo_id' = sd.slo_id::text
   LEFT JOIN alert_firings af ON af.rule_id = ar.rule_id
   WHERE sd.tenant_id = $1
     AND ($2::TEXT IS NULL OR sd.environment = $2)
   GROUP BY sd.service_name
   ```

2. **Latest deployment per service:**
   ```sql
   SELECT DISTINCT ON (service_name) service_name, service_version
   FROM deployment_markers
   WHERE tenant_id = $1
     AND ($2::TEXT IS NULL OR environment = $2)
   ORDER BY service_name, started_at DESC
   ```

Merge both result sets into the `HashMap<String, ServiceCatalogEnrichment>` keyed by
`service_name` (default `active_alert_count: 0, slo_breaching: false, latest_deployment: None`
for services with no row in either query).

`service_summary_from_row` keeps its current signature and pure error-rate-only behavior (its
existing unit test `summary_row_derives_red_metrics_and_health` stays valid unchanged). A new
small function applies the enrichment on top:

```rust
fn apply_catalog_enrichment(mut summary: ServiceSummary, enrichment: Option<&ServiceCatalogEnrichment>) -> ServiceSummary {
    if let Some(e) = enrichment {
        summary.active_alert_count = e.active_alert_count;
        summary.latest_deployment = e.latest_deployment.clone();
        if e.slo_breaching {
            summary.health_state = "breach".to_string();
        }
    }
    summary
}
```

Both `list_service_summaries` and `get_service_summary` call
`fetch_service_catalog_enrichment` once (single batched query, not per-service) and apply it to
each row.

---

## 3. Tasks

### Task 1: Backend enrichment + unit tests

**Files:**
- Modify: `services/query-api/src/discovery.rs`

- [x] **Step 1: Write failing unit tests** for:
  - `apply_catalog_enrichment` overrides `health_state` to `"breach"` when `slo_breaching = true`,
    even if the error-rate-derived state was `"healthy"`.
  - `apply_catalog_enrichment` leaves `health_state` as the error-rate result when
    `slo_breaching = false` (SLO exists but not firing).
  - `apply_catalog_enrichment` passes through `active_alert_count` and `latest_deployment` from
    the enrichment map.
  - `apply_catalog_enrichment` with `enrichment = None` leaves the summary unchanged (no SLO
    defined for that service — existing placeholder defaults `active_alert_count: 0,
    latest_deployment: None` remain correct in this case).

- [x] **Step 2: Run `cargo test -p query-api discovery::tests -- --nocapture`** — confirm new
  tests fail (function doesn't exist yet).

- [x] **Step 3: Implement** `ServiceCatalogEnrichment`, `fetch_service_catalog_enrichment`, and
  `apply_catalog_enrichment` as designed in §2.4. Wire both into `list_service_summaries` and
  `get_service_summary`, replacing the direct `service_summary_from_row(row, duration_secs)` call
  with `apply_catalog_enrichment(service_summary_from_row(row, duration_secs), enrichment.get(&row.service_name))`.

- [x] **Step 4: Run the unit tests again** — confirm pass. Run
  `cargo test -p query-api discovery::` to confirm no existing test broke.

- [x] **Step 5: `cargo fmt --all`**, then commit:
  ```
  git commit -m "feat(query-api): wire SLO-aware health, active alert count, and latest deploy into service summaries"
  ```

### Task 2: HTTP integration test (mandatory per AGENTS.md)

This adds a new code path (Postgres enrichment branch) to existing handlers
`list_service_summaries` / `get_service_summary`, so per AGENTS.md "HTTP Integration Tests for
Handler Changes" this needs end-to-end coverage in
`services/query-api/tests/http_api_integration.rs`.

**Files:**
- Modify: `services/query-api/tests/http_api_integration.rs`

- [x] **Step 1: Write a failing integration test** that, against the Testcontainers
  Postgres+ClickHouse stack:
  - Inserts spans for service `checkout` with a low error rate (so error-rate health = `"healthy"`).
  - Inserts an `slo_definitions` row for `checkout` (any `sli_type`/`target`).
  - Inserts an `alert_rules` row with `alert_type = 'slo_burn_rate'` and
    `condition = {"slo_id": "<the slo_id>"}`.
  - Inserts an `alert_firings` row for that rule with `state = 'active'`.
  - Inserts a `deployment_markers` row for `checkout` with `service_version = 'v2.3.1'`.
  - Calls `GET /v1/services/summary` and asserts the `checkout` entry has
    `health_state = "breach"`, `active_alert_count >= 1`, `latest_deployment = "v2.3.1"`.
  - Also inserts a second service `billing` with **no** SLO/alert/deployment rows and asserts it
    keeps `active_alert_count = 0`, `latest_deployment = null`, and its error-rate-derived
    `health_state`.

- [x] **Step 2: Run** `cargo test -p query-api --test http_api_integration service_summary -- --nocapture`
  — confirm it fails before Task 1's implementation (or run before Task 1's Step 5 commit if doing
  both in one branch).

- [x] **Step 3: Confirm it passes** once Task 1 is implemented.

- [x] **Step 4: Commit:**
  ```
  git commit -m "test(query-api): cover SLO-aware health and deploy enrichment in service summary HTTP API"
  ```

### Task 3: Frontend check (no code change expected, verify only)

`apps/frontend/src/pages/ServicesPage.tsx` already renders `active_alert_count`,
`latest_deployment`, and `health_state` (Section 1 above). No frontend code change should be
needed.

- [x] **Step 1:** Run `npm test -- --run src/pages` (or the existing `ServicesPage` test if one
  exists) to confirm the page still renders correctly with non-zero `active_alert_count` and a
  populated `latest_deployment` — add a small assertion to an existing `ServicesPage` test if one
  exists and doesn't already cover the "Active Alerts" / "Latest Deploy" columns with non-default
  values. If no such test exists, add one mocking `listServiceSummaries` to return a row with
  `active_alert_count: 2, latest_deployment: "v2.3.1", health_state: "breach"` and assert the
  table renders `2`, `v2.3.1`, and the `Breach` badge.

- [x] **Step 2:** `npm run lint && npm test -- --run` for the touched files.

- [x] **Step 3:** Commit if a test was added:
  ```
  git commit -m "test(services-page): cover active alert count and latest deploy columns"
  ```

### Task 4: Spec sync

Per AGENTS.md "ADR and Spec Synchronization": this changes how documented response fields are
computed (data-model-adjacent), so update:

- [x] `spec/08-ai-ml.md` — the line "The `health_state` field is computed post-query from
  `error_rate` and cannot be filtered server-side" (around line 462) is now only half true. Update
  it to describe the SLO-burn-rate override and that it's still computed post-query (not
  filterable server-side) so NLQ-side assumptions stay accurate.
- [x] `spec/09-api.md` §"Service Detail Summary" — add one sentence noting `active_alert_count`
  and `latest_deployment` are now populated (SLO-linked alerts and `deployment_markers`
  respectively) and link to the known limitation in §2.1 of this plan (alerts not linked to an
  SLO are not counted).

No ADR is needed — this does not change architecture, technology choice, deployment model, or
data model (no migration). State this explicitly in the PR description.

### Task 5: Update `docs/agent-context.md`

- [x] Add a short note (pattern: see existing "Known simplification" note for `dedup_key`) stating:
  - `GET /v1/services/summary` and `GET /v1/services/{service_name}/summary` now populate
    `active_alert_count` (active `slo_burn_rate` firings linked via `slo_definitions.service_name`
    only — not all alert types, due to `alert_rules` lacking `service_name`) and
    `latest_deployment` (latest `deployment_markers.service_version`).
  - `health_state` is `"breach"` if any linked SLO is currently breaching, else the existing
    error-rate threshold result.
- [x] Update the P9-S5 status note in `docs/superpowers/plans/2026-06-04-observability-feature-parity-plan.md`
  to record that this plan's scope (active alert count, latest deployment, SLO-breach health
  override) is done, while the fast/slow-burn distinction, 30s polling refresh, and error-issue
  count remain open (the latter blocked on P9-S2). Do not mark P9-S5 fully complete.

---

## 4. Verification

- `cargo fmt --all` (mandatory before any Rust commit per AGENTS.md).
- `cargo test -p query-api` (unit + the new integration test, requires Docker for Testcontainers).
- `bash scripts/local-ci.sh` before the final push of each task's branch — no skip flags needed
  (this slice touches both Rust and frontend, and Docker is available for Testcontainers).
- Manual spot check (optional but recommended): `docker compose up -d`, seed data via
  `scripts/seed/`, open `/services` in the frontend, confirm a service with a firing SLO shows
  "Breach" and a non-zero "Active Alerts" count, and a deployed service shows its version under
  "Latest Deploy".

---

## 5. Rollback

Fully additive and read-only:
- No migrations, no new tables, no ingest-path changes.
- If the enrichment query causes a regression (e.g. unexpected latency on `/v1/services/summary`
  for tenants with many SLOs), revert the single commit from Task 1 — `service_summary_from_row`
  and the route signatures are otherwise unchanged, so the revert is a clean function-body
  reversion with no follow-on cleanup.

## 6. Telemetry / Tenancy / Retention Impact

- **Telemetry impact:** none — no new spans/logs/metrics emitted; this only changes how an
  existing read endpoint composes its response.
- **Auth/tenancy impact:** none — the new Postgres queries are scoped by `ctx.tenant_id` exactly
  like every other query in `discovery.rs`, `slos.rs`, and `deployments.rs`.
- **Data retention impact:** none — no new storage, no schema change.

---

## 7. After This Slice — Next Promotion Candidates

Per the roadmap's "Promotion Rules" (promote one at a time, write a detailed plan before
implementing), once this slice merges the following are the most ready next candidates from
`2026-06-04-observability-feature-parity-plan.md`:

1. **Housekeeping (no plan needed):** close issues #388 and #389 — both describe already-shipped
   features (Trace Comparison, Query Workbench). Confirm against `docs/agent-context.md` and the
   actual `apps/frontend/src/pages/TraceCompare*.tsx` / `apps/frontend/src/features/workbench/`
   code, then close with a short comment pointing at the merging PRs (#39x range).

2. **P12-S3: Deadman alert type** (Quick Win, "promote now" per §6 sequencing) — small,
   self-contained `alert-evaluator` addition (alert fires when a service stops sending data for
   N minutes); no new infra, reuses the existing alert rule/firing model. Good size for a single
   detailed plan + PR.

3. **P14-S4: Change Event API** (Quick Win, "~2 hours of backend work" per §4) — small `query-api`
   addition extending the deployment-marker model to generic change events; pairs naturally with
   the `deployment_markers` work touched in this slice.

4. **P9-S2: Error Tracking Ingestion and Fingerprinting** — the largest single workflow gap per
   §2.2 of the feature-parity plan, but it's a multi-PR phase (new `error_issues` ClickHouse
   table, `stream-processor` fingerprinting, new `query-api` endpoints, migrations). This slice
   (P9-S5) and a Deadman/Change-Event quick win are better-sized "next" promotions; P9-S2 should
   get its own multi-task detailed plan once promoted, broken into ingestion/fingerprinting →
   `GET /v1/errors` → frontend explorer → regression detection, mirroring how P5-S1 (incident
   timeline) was sliced.

5. **Modelable migration**: do not resume Phase 2 steps 2.4+ until upstream `1.2a` (Rust/SQL-DDL
   scope decision in `github.com/ktjn/modelable`) is resolved — re-check
   `docs/superpowers/plans/2026-06-08-modelable-type-mapping-migration-plan.md` periodically for
   that upstream signal rather than re-deriving it here.
