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
- [x] Remove or rotate any historical default secret and scan the complete Git history.
- [x] Verify Helm and Compose cannot deploy a non-development environment with a default secret.
- [x] Complete OIDC login, callback, token-exchange failure, cookie, and session regression tests.
- [x] Complete admin-service authentication and tenant-isolation middleware tests.
- [x] Complete tests for token issue/revoke and member role mutation handlers.
- [x] Remove dead unchecked SQL-builder implementations.
- [x] Reuse HTTP clients in authentication middleware hot paths.
- [x] Fix all documentation that claims GitHub CI is disabled.

### Exit criteria

- No known critical or high-severity authentication or tenant-isolation defect remains.
- Missing security configuration causes startup or installation to fail closed.
- Cross-tenant query and administration access is covered by automated tests.
- Required GitHub checks are documented and enforced on `main`.

## Milestone 2 — Public repository hygiene

- [x] Replace internal implementation-plan history with GitHub issues, ADRs, or public design docs.
- [x] Reduce `AGENTS.md` to repository facts, generated-code rules, commands, and invariants.
- [x] Remove tracked local configuration such as `.claude/settings.local.json` unless demonstrably
      sanitized and required.
- [x] Add or verify secret-scanning ignore rules without suppressing real findings.
- [x] Add `SUPPORT.md`, `CODEOWNERS`, issue templates, and a concise PR template.
- [x] Document versioning, compatibility, and release support policy.
- [x] Reconcile README claims with implemented ingestion and metric fidelity.
- [x] Publish an explicit signal-support and known-data-loss matrix.
- [x] Decide whether vendored demos remain in the main repository; retain one canonical demo for
      the supported onboarding path.

### Exit criteria

- A new contributor can understand scope, build, test, and contribution rules without reading
  internal agent orchestration material.
- Public product claims match tested behavior.
- Repository and history scans contain no unresolved credential or private-data findings.

## Milestone 3 — Supported installation and upgrades

- [x] Define an `evaluation` Compose profile with the minimum required components.
- [x] Define one supported production-like Helm configuration.
- [x] Document required and optional services, ports, trust boundaries, and TLS assumptions.
- [x] Validate all required configuration at startup with actionable errors.
- [x] Document minimum CPU, memory, disk, and expected idle resource use.
- [ ] Test a clean Compose installation from released artifacts.
- [ ] Test a clean Helm installation from released artifacts.
- [ ] Test migration idempotency and restart during migration.
- [ ] Create and test a synthetic pre-0.1 upgrade fixture.
- [x] Document and verify backup and restore.
- [x] Document unsupported downgrade behavior.

### Exit criteria

- A clean environment can install Observable using only versioned release artifacts.
- Upgrade, backup, and restore procedures are executable and covered by tests.
- No required production configuration relies on an insecure default.

## Milestone 4 — Protocol and data correctness

- [ ] Add black-box OTLP gRPC tests using a pinned OpenTelemetry Collector image.
- [ ] Add black-box OTLP HTTP/protobuf tests.
- [x] Test gzip and supported compression behavior.
- [x] Test malformed protobuf, request-size limits, rate limiting, and partial-success responses.
- [x] Test Prometheus Remote Write compatibility and tenant routing.
- [x] Test resource, scope, and signal attribute preservation.
- [x] Document histogram, exponential-histogram, and summary fidelity.
- [x] Test trace/log/metric correlation identifiers end to end.
- [x] Define and document public API stability expectations.
- [ ] Ensure public HTTP endpoints have machine-readable contracts where practical.
- [x] Add deterministic generated-code verification that fails on regeneration drift.

### Exit criteria

- Supported clients can send each supported signal through documented paths.
- Rejected or lossy data is reported consistently and documented.
- Generated models reproduce without manual undocumented edits.

## Milestone 5 — Reliability and self-observability

- [x] Add missing `/metrics` endpoints to ingest-gateway, stream-processor, and alert-evaluator.
- [x] Add ingest throughput, rejection, queue, processing, and alert-evaluation metrics.
- [x] Provide a dashboard for Observable's own health.
- [x] Define measurable ingestion and query latency gates.
- [x] Run a minimum one-hour ingest/query soak test.
- [x] Test Redpanda interruption and recovery.
- [x] Test ClickHouse interruption during ingestion and query.
- [x] Test PostgreSQL and OpenFGA unavailability with fail-closed behavior where required.
- [x] Test bounded memory, queue growth, backpressure, and disk-full behavior.
- [x] Document capacity assumptions and recovery procedures.

### Exit criteria

- The platform exposes enough telemetry to diagnose its own critical paths.
- Soak and dependency-failure tests have explicit pass/fail thresholds.
- No tested dependency outage causes silent cross-tenant exposure or unbounded resource growth.

## Milestone 6 — Core user journeys

- [x] Add Playwright coverage for login and onboarding.
- [x] Add an end-to-end first-telemetry flow.
- [x] Add trace search and trace-to-related-logs navigation coverage.
- [x] Add metric query and dashboard creation coverage.
- [x] Add alert creation and triggered-state coverage.
- [x] Add administrator member-role mutation coverage.
- [x] Add browser-level tenant-isolation coverage.
- [x] Run accessibility scans for every major page in CI.
- [x] Run the maintained visual-regression suite in CI.
- [x] Add a ten-minute evaluation guide and collector configuration.
- [x] Add architecture, configuration, security-hardening, and troubleshooting documentation.

### Exit criteria

- Core workflows pass from browser to storage against the supported deployment topology.
- Major views meet the defined accessibility baseline.
- A new evaluator can reach first telemetry without undocumented steps.

## Milestone 7 — Release engineering

- [x] Establish one repository/product version source of truth.
- [ ] Build release artifacts from immutable tags.
- [ ] Publish multi-architecture container images where supported.
- [ ] Publish the Helm chart. (workflow implemented; mark complete after the first release
      tag verifies the chart can be pulled and installed from `oci://ghcr.io/ktjn/charts/observable`)
- [ ] Generate SBOMs and provenance attestations. (workflow implemented for the container image;
      mark complete after the first release tag verifies both attestations are present and
      `gh attestation verify` succeeds)
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

The current implementation priority is **Milestone 7 — Release engineering**.

Milestones 1–6 are complete. The next items are versioned release artifacts, container image
signing, Helm chart publishing, and post-release verification.

The detailed feature backlog remains useful for post-0.1 planning but does not override this roadmap.
