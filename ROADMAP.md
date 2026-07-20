# Observable Roadmap

> **Status:** Active and authoritative.
>
> This is the sole release-sequencing roadmap for Observable. It describes outcomes and evidence,
> not delivery dates or a complete implementation backlog. Work is decomposed into small GitHub
> issues and pull requests according to [spec/10-process.md](spec/10-process.md).

## Direction

Observable is an open-source, full-stack observability platform. The path to `1.0.0` first makes
Docker Compose evaluation dependable, then establishes Kubernetes operations, governance, and a
complete service-reliability workflow before promising a stable self-hosted contract.

The product aims to provide a credible self-hosted observability core in the same problem space as
Datadog, Dynatrace, and New Relic. `1.0.0` does not imply feature parity with those products.

## Release model

Each release slice has a target outcome, dependencies, scope, exit evidence, and non-goals. A slice
is complete only when the stated evidence exists; the presence of a UI page, API, or workflow alone
does not establish operational maturity.

Release numbers express dependency order rather than dates. Before `1.0.0`, storage schemas, APIs,
configuration, Helm values, and upgrade procedures may change as described in
[VERSIONING.md](VERSIONING.md). Only the latest release is supported.

## 0.1 — Evaluation baseline (shipped)

**Outcome:** An evaluator can run the initial public release and exercise the core observability
journeys in a non-critical environment.

**Shipped capabilities:**

- OTLP trace, log, and supported metric ingestion over gRPC and HTTP, plus Prometheus Remote Write.
- Tenant-aware storage and query paths, OIDC browser authentication, OpenFGA authorization, and
  ingestion-token environment binding.
- Trace, log, metric, service, dashboard, alert, SLO, incident, deployment-event, and administration
  surfaces, with initial cross-signal navigation and webhook notifications.
- Docker Compose evaluation and Helm deployment assets, migrations, backup/restore guidance,
  platform self-observability, smoke/performance checks, and tag-bound release workflows.

**Open release debt:** Published Helm, SBOM/provenance, Compose/Helm installation, and post-release
smoke claims still require evidence against the actual `v0.1.0` artifacts. Workflow implementation
or source-built verification is not equivalent to published-artifact verification.

**Support boundary:** `0.1.0` is for evaluation and small non-critical deployments. It does not
claim production Kubernetes readiness, broad high availability, stable compatibility, or a complete
governance and incident-response contract.

## 0.2 — Effortless self-hosted evaluation

**Outcome:** A new self-hoster can evaluate Observable through Docker Compose without building from
source or discovering undocumented lifecycle steps.

**Depends on:** The `0.1.0` evaluation baseline.

**Scope:**

- Verify fresh installation exclusively from published, versioned artifacts on supported hosts.
- Make first telemetry, health diagnosis, reset, upgrade, backup, and restore repeatable from the
  documented Compose path.
- Publish clear prerequisites, resource expectations, failure messages, and troubleshooting data.
- Close the `0.1.0` release-artifact evidence debt, including signatures and attestations.

**Exit evidence:**

- A clean-host evaluator reaches first telemetry within the documented flow and time budget.
- Automated release-candidate checks install, upgrade, back up, restore, reset, and smoke-test the
  published Compose artifacts.
- Release documentation identifies supported host assumptions and actionable diagnostics for every
  critical dependency.

**Non-goals:** Production support for Compose, broad HA, fleet management, billing, and advanced
telemetry signals.

## 0.3 — Operator-ready Kubernetes

**Outcome:** A platform team can operate one supported Kubernetes topology with explicit capacity,
failure, and lifecycle boundaries.

**Depends on:** Repeatable release artifacts and lifecycle procedures from `0.2`.

**Scope:**

- Verify Helm installation and upgrades from the published OCI chart and versioned images.
- Harden configuration, secrets, ingress, persistent storage, migrations, rollback, and recovery for
  the supported topology.
- Provide capacity guidance, platform health dashboards, alerting, and runbooks for critical paths.
- Exercise dependency interruption, restart, backup/restore, and bounded-degradation behavior in a
  representative Kubernetes environment.

**Exit evidence:**

- Automated clean install, upgrade, rollback, and recovery checks pass against published artifacts.
- Measured capacity envelopes and dependency-failure drills have explicit pass/fail thresholds.
- Operators can diagnose ingest, query, storage, queue, identity, and authorization failures using
  supported telemetry and runbooks.

**Non-goals:** Multi-region operation, active-active control planes, arbitrary Kubernetes
distributions, or a broad HA claim beyond the tested topology.

## 0.4 — Governed team adoption

**Outcome:** Multiple teams can share a deployment with enforceable usage, retention, access, and
accountability controls.

**Depends on:** A supportable Kubernetes operational baseline from `0.3`.

**Scope:**

- Enforce tenant-aware ingest limits, quotas, and cardinality budgets with visible rejection behavior.
- Complete retention and deletion workflows across supported signals and storage tiers.
- Mature role and resource scoping, audit coverage, credential lifecycle, and tenant-isolation tests.
- Expose actionable usage, cost, and cardinality diagnostics to operators and tenant administrators.

**Exit evidence:**

- Load and adversarial tests prove limits cannot be bypassed across tenants or environments.
- Retention and deletion have documented completion semantics and verifiable audit records.
- Usage reports reconcile with accepted, rejected, retained, and deleted telemetry within documented
  tolerances.

**Non-goals:** Billing systems, compliance certification, regional residency, bring-your-own-key,
and tenant-isolated product packaging.

## 0.5 — Service reliability workflow

**Outcome:** A service owner can move from detection through triage, notification, and review without
manually reconstructing context across tools.

**Depends on:** Governed team and data boundaries from `0.4`.

**Scope:**

- Mature trace/log/metric, service/topology, infrastructure, deployment, and change-event correlation.
- Complete SLO and error-budget workflows, alert routing, suppression, escalation integrations, and
  incident timelines.
- Connect service ownership, impact context, runbooks, and reliability review views.
- Provide versionable import/export for supported dashboards, alerts, SLOs, and related configuration.

**Exit evidence:**

- End-to-end scenarios prove detect-to-review journeys for latency, errors, saturation, no-data, and
  deployment regressions.
- Notification delivery, deduplication, retries, suppression, and failure visibility meet documented
  expectations.
- Configuration round trips deterministically and validates before application.

**Non-goals:** Replacing dedicated paging products, autonomous remediation, advanced AI incident
management, and parity with every third-party integration ecosystem.

## 1.0 — Stable self-hosted contract

**Outcome:** Adopters can run the supported self-hosted topology with a stable compatibility,
security, performance, upgrade, and support contract.

**Depends on:** Demonstrated evaluation, Kubernetes operations, governance, and reliability
workflows from `0.2` through `0.5`.

**Scope:**

- Define and verify stable public APIs, configuration, Helm values, storage migrations, and upgrade
  compatibility under semantic versioning.
- Complete security review and tenant-isolation evidence for supported identity and deployment paths.
- Publish tested performance/capacity envelopes, operational SLOs, recovery objectives, and runbooks.
- Establish release support boundaries, vulnerability handling, migration policy, and artifact
  provenance for the supported topology.

**Exit evidence:**

- Upgrade suites cover every supported predecessor and preserve documented data and configuration.
- Security, tenant-escape, load, soak, chaos, backup/restore, and disaster-recovery gates pass against
  release candidates with retained evidence.
- Documentation and support policy enumerate supported versions, platforms, dependencies, and known
  limitations without relying on pre-`1.0` instability exceptions.

**Non-goals:** Feature parity with commercial suites, LTS branches unless separately adopted,
multi-region active-active operation, or making every post-`1.0` theme part of the stable core.

## Beyond 1.0 themes

These are unordered candidates, not commitments or blockers for `1.0.0`:

- Browser RUM, mobile observability, session replay, continuous profiling, and synthetics.
- Advanced AI assistance, anomaly models, capacity forecasting, and approval-gated remediation.
- Regional residency, compliance reporting, bring-your-own-key, billing, and marketplace packaging.
- Multi-region operation, broader HA topologies, fleet management, and additional ecosystem adapters.

Accepted but unimplemented ADRs describe architectural direction where one exists; they do not assign
a release. Prioritization happens through reviewed issues and small vertical slices.
