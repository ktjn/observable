# Observable Roadmap to 0.1.0

> **Status:** Active and authoritative.
>
> Until `v0.1.0` is released, release readiness takes precedence over new feature work. New product
> features should only be promoted when they remove a release blocker, close a documented
> correctness gap, or are required by the supported 0.1 deployment contract.

## Goal

Release a secure, reproducible, testable evaluation version of Observable that supports the core
observability workflow without claiming production maturity that has not been demonstrated.

Observable 0.1 is intended for evaluation and small non-critical deployments. Storage schemas,
APIs, Helm values, and upgrade procedures may change without backward compatibility before 1.0.

## Priority rule

Work in this order:

1. Security and tenant-isolation blockers.
2. Reproducible installation, migration, backup, and restore.
3. Protocol and data correctness.
4. Operational reliability and self-observability.
5. Core user journeys and documentation.
6. Release engineering and artifact verification.

Do not prioritize saved-view parity, fleet management, billing, asynchronous export, additional
notification adapters, or other broad feature work while a higher-priority 0.1 item remains open.

## 0.1 scope

### Included

- OTLP traces, logs, and supported metric types over gRPC and HTTP.
- Prometheus Remote Write ingestion.
- Trace, log, and metric exploration with documented cross-signal correlation.
- Dashboards, threshold alerts, SLO burn-rate alerts, and webhook notifications.
- API-key ingestion and OIDC browser authentication.
- Tenant and environment isolation.
- One supported Docker Compose evaluation topology.
- One supported Helm deployment topology.
- Documented retention, migration, backup, restore, and failure behavior.
- Platform self-observability.
- Signed, versioned release artifacts with SBOMs.

### Deferred beyond 0.1

- Fleet management.
- Billing workflows.
- Asynchronous export.
- Full saved-view parity across every signal.
- Additional notification-channel adapters.
- Broad HA or large-scale clustering claims.
- Multiple identity-provider combinations beyond the supported reference setup.
- Natural-language querying as a release-critical capability.

## Milestone 1 — Release blockers

- [x] Fail closed when `SESSION_SECRET` is missing outside development mode.
- [x] Add regression tests for missing and empty session secrets.
- [ ] Remove or rotate any historical default secret and scan the complete Git history.
- [ ] Verify Helm and Compose cannot deploy a non-development environment with a default secret.
- [ ] Complete OIDC login, callback, token-exchange failure, cookie, and session regression tests.
- [ ] Complete admin-service authentication and tenant-isolation middleware tests.
- [ ] Complete tests for token issue/revoke and member role mutation handlers.
- [ ] Remove dead unchecked SQL-builder implementations.
- [ ] Reuse HTTP clients in authentication middleware hot paths.
- [ ] Fix all documentation that claims GitHub CI is disabled.

### Exit criteria

- No known critical or high-severity authentication or tenant-isolation defect remains.
- Missing security configuration causes startup or installation to fail closed.
- Cross-tenant query and administration access is covered by automated tests.
- Required GitHub checks are documented and enforced on `main`.

## Milestone 2 — Public repository hygiene

- [ ] Replace internal implementation-plan history with GitHub issues, ADRs, or public design docs.
- [ ] Reduce `AGENTS.md` to repository facts, generated-code rules, commands, and invariants.
- [ ] Remove tracked local configuration such as `.claude/settings.local.json` unless demonstrably
      sanitized and required.
- [ ] Add or verify secret-scanning ignore rules without suppressing real findings.
- [ ] Add `SUPPORT.md`, `CODEOWNERS`, issue templates, and a concise PR template.
- [ ] Document versioning, compatibility, and release support policy.
- [ ] Reconcile README claims with implemented ingestion and metric fidelity.
- [ ] Publish an explicit signal-support and known-data-loss matrix.
- [ ] Decide whether vendored demos remain in the main repository; retain one canonical demo for
      the supported onboarding path.

### Exit criteria

- A new contributor can understand scope, build, test, and contribution rules without reading
  internal agent orchestration material.
- Public product claims match tested behavior.
- Repository and history scans contain no unresolved credential or private-data findings.

## Milestone 3 — Supported installation and upgrades

- [ ] Define an `evaluation` Compose profile with the minimum required components.
- [ ] Define one supported production-like Helm configuration.
- [ ] Document required and optional services, ports, trust boundaries, and TLS assumptions.
- [ ] Validate all required configuration at startup with actionable errors.
- [ ] Document minimum CPU, memory, disk, and expected idle resource use.
- [ ] Test a clean Compose installation from released artifacts.
- [ ] Test a clean Helm installation from released artifacts.
- [ ] Test migration idempotency and restart during migration.
- [ ] Create and test a synthetic pre-0.1 upgrade fixture.
- [ ] Document and verify backup and restore.
- [ ] Document unsupported downgrade behavior.

### Exit criteria

- A clean environment can install Observable using only versioned release artifacts.
- Upgrade, backup, and restore procedures are executable and covered by tests.
- No required production configuration relies on an insecure default.

## Milestone 4 — Protocol and data correctness

- [ ] Add black-box OTLP gRPC tests using a pinned OpenTelemetry Collector image.
- [ ] Add black-box OTLP HTTP/protobuf tests.
- [ ] Test gzip and supported compression behavior.
- [ ] Test malformed protobuf, request-size limits, rate limiting, and partial-success responses.
- [ ] Test Prometheus Remote Write compatibility and tenant routing.
- [ ] Test resource, scope, and signal attribute preservation.
- [ ] Document histogram, exponential-histogram, and summary fidelity.
- [ ] Test trace/log/metric correlation identifiers end to end.
- [ ] Define and document public API stability expectations.
- [ ] Ensure public HTTP endpoints have machine-readable contracts where practical.
- [ ] Add deterministic generated-code verification that fails on regeneration drift.

### Exit criteria

- Supported clients can send each supported signal through documented paths.
- Rejected or lossy data is reported consistently and documented.
- Generated models reproduce without manual undocumented edits.

## Milestone 5 — Reliability and self-observability

- [ ] Add missing `/metrics` endpoints to ingest-gateway, stream-processor, and alert-evaluator.
- [ ] Add ingest throughput, rejection, queue, processing, and alert-evaluation metrics.
- [ ] Provide a dashboard for Observable's own health.
- [ ] Define measurable ingestion and query latency gates.
- [ ] Run a minimum one-hour ingest/query soak test.
- [ ] Test Redpanda interruption and recovery.
- [ ] Test ClickHouse interruption during ingestion and query.
- [ ] Test PostgreSQL and OpenFGA unavailability with fail-closed behavior where required.
- [ ] Test bounded memory, queue growth, backpressure, and disk-full behavior.
- [ ] Document capacity assumptions and recovery procedures.

### Exit criteria

- The platform exposes enough telemetry to diagnose its own critical paths.
- Soak and dependency-failure tests have explicit pass/fail thresholds.
- No tested dependency outage causes silent cross-tenant exposure or unbounded resource growth.

## Milestone 6 — Core user journeys

- [ ] Add Playwright coverage for login and onboarding.
- [ ] Add an end-to-end first-telemetry flow.
- [ ] Add trace search and trace-to-related-logs navigation coverage.
- [ ] Add metric query and dashboard creation coverage.
- [ ] Add alert creation and triggered-state coverage.
- [ ] Add administrator member-role mutation coverage.
- [ ] Add browser-level tenant-isolation coverage.
- [ ] Run accessibility scans for every major page in CI.
- [ ] Run the maintained visual-regression suite in CI.
- [ ] Add a ten-minute evaluation guide and collector configuration.
- [ ] Add architecture, configuration, security-hardening, and troubleshooting documentation.

### Exit criteria

- Core workflows pass from browser to storage against the supported deployment topology.
- Major views meet the defined accessibility baseline.
- A new evaluator can reach first telemetry without undocumented steps.

## Milestone 7 — Release engineering

- [ ] Establish one repository/product version source of truth.
- [ ] Build release artifacts from immutable tags.
- [ ] Publish multi-architecture container images where supported.
- [ ] Publish the Helm chart.
- [ ] Generate SBOMs and provenance attestations.
- [ ] Sign container images and release artifacts.
- [ ] Generate release notes and a changelog.
- [ ] Verify Compose and Helm installation exclusively from published artifacts.
- [ ] Run post-release smoke tests against the published version.
- [ ] Tag and publish `v0.1.0`.

## Release acceptance criteria

`v0.1.0` is releasable only when all of the following are true:

- No known critical or high-severity authentication or tenant-isolation defect remains.
- No default installation credential can authenticate a user or sign a session.
- Fresh Compose and Helm installations pass from released artifacts.
- Upgrade, backup, and restore verification passes.
- OTLP gRPC, OTLP HTTP, and Prometheus Remote Write compatibility tests pass.
- Core trace, log, metric, dashboard, alert, and administration journeys pass browser E2E tests.
- Cross-tenant query and administration tests pass.
- The defined soak and performance thresholds pass.
- README and support-matrix claims match implemented behavior.
- Required CI checks protect `main`.
- Released artifacts are immutable, signed, and accompanied by SBOMs.

## Current work

The current implementation priority is **Milestone 1 — Release blockers**.

The fail-closed session-secret code is present and tested. The next tasks are:

1. Verify all Helm and Compose secret paths and eliminate deployable non-development defaults.
2. Scan current content and Git history for secrets and local/private configuration.
3. Complete OIDC callback/session and admin-service tenant-isolation regression coverage.
4. Reconcile CI documentation with the active GitHub Actions workflow.

The detailed feature backlog remains useful for post-0.1 planning but does not override this roadmap.
