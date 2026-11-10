# Changelog

All notable changes to the `observable-alerting` component are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `GET /internal/alerting/self-status` (internal token, phase 2 E5, OBS-29-03 input): whether the alert evaluation worker is keeping up, with counts only. `state` is `not_started`, `current` or `stale`; it turns `stale` once the last *successful* pass is older than twice `ALERT_EVAL_INTERVAL_SECONDS`, so a paused, stuck or repeatedly failing evaluator is visible while the process still answers. Reports `last_success`, `last_failure_at` and `consecutive_failures`. Per process, in memory.
- The catalog asset sync registers approved objectives (`alerting.service_objectives`) as `objective` assets by catalog service id, owned by their creator. Drafts and objectives whose approval an edit cleared are left out, so their assets are retired.
- Tenant purge: a background worker (`TENANT_PURGE_INTERVAL_SECONDS`, default 60) pulls pending requests for `alerting` from observable-control (`GET /internal/tenant-purges`, requires `ADMIN_SERVICE_URL`), deletes the tenant's rows in its own schema in one transaction as the scoped login, and acknowledges only after the commit. Covers rules, firings, channels, notification audit, incidents, SLOs and objectives.
- Approved objectives per catalog service and environment class (phase 1 C3, OBS-01-04):
  migration `063_create_alerting_service_objectives.sql` and `POST/GET /v1/objectives`,
  `PATCH /v1/objectives/{id}` (clears approval), `POST /v1/objectives/{id}/approve`
  (tenant admin, a person, the revision read) and `GET /v1/objectives/{id}/compliance`.
  Compliance is `compliant`, `at_risk`, `breached` or `unknown` (missing or stale evidence),
  evaluated on request over spans of the service's bindings in exactly the objective's class.
  Unapproved objectives return 409 `objective_not_approved`. Catalog reads forward the
  caller's credential (`CATALOG_URL`); hidden, missing and retired services are 404.

- Accountable owners for alert rules and SLOs (phase 1 S6, OBS-34-01): migration
  `055_alerting_asset_owners.sql` (`owner_kind`, `owner_id`), an optional `owner` on create
  (defaults to the calling user; API-key callers without one create an ownerless asset),
  `owner_kind`/`owner_id` on rule and SLO responses, and `PUT /v1/admin/alerts/rules/{rule_id}/owner`
  and `PUT /v1/slos/{slo_id}/owner` (tenant_admin; `null` clears). An SLO's burn-rate rule carries
  the SLO's service and owner.
- Rules and SLOs are synced to the catalog asset registry every `CATALOG_SYNC_INTERVAL_SECONDS`
  (default 60) when `CATALOG_URL` is set; rules without a service are left out.

- Extended incident lifecycle endpoints (`POST /v1/incidents/{incident_id}/acknowledge`, `status`, `resolve`, `comments`, and `link-deployment`).
- Migration `045_extend_incidents_lifecycle.sql` for responder fields (`acknowledged_at`, `acknowledged_by`, `resolved_by`, and `related_deployment_id`).
- Testcontainers integration test exercising incident transitions, timeline comments, deployment correlation, and tenant isolation.

## [0.1.0] - 2026-07-18

Initial component baseline extracted from the `0.1.0` platform release.

### Added

- Alert-rule CRUD, silence, and runbook APIs.
- SLO definitions and burn-rate evaluation.
- Notification channels and delivery audit.
- Incident creation and lifecycle.
- `/internal/alerting-enrichment` and `/internal/alerting-correlation` endpoints consumed by query.
