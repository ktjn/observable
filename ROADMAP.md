# Observable Roadmap

> **Status:** Active and authoritative.
>
> This is the sole release-sequencing roadmap for Observable. It describes outcomes and evidence,
> not delivery dates or a complete implementation backlog. Work is decomposed into small GitHub
> issues and pull requests according to [spec/10-process.md](spec/10-process.md).

## Direction

Observable is an open-source, full-stack observability platform. The immediate roadmap goal is to
turn the existing multi-service monorepo into a set of independently buildable, versioned,
deployable, and evolvable components before further production-maturity work expands the existing
coupling.

The decomposition target is defined in
[docs/component-decomposition.md](docs/component-decomposition.md) and
[ADR-035](spec/adr/ADR-035-component-independence.md). Repository extraction is deliberately the
last step: contracts, state ownership, runtime boundaries, independent images, and compatibility
tests must be established first so the result is not a distributed monolith.

After that architectural foundation is complete, the roadmap continues through dependable
self-hosted evaluation, Kubernetes operations, governance, and a complete service-reliability
workflow before promising a stable `1.0.0` contract.

The product aims to provide a credible self-hosted observability core in the same problem space as
Datadog, Dynatrace, and New Relic. `1.0.0` does not imply feature parity with those products.

## Release model

Each release slice has a target outcome, dependencies, scope, exit evidence, and non-goals. A slice
is complete only when the stated evidence exists; the presence of a UI page, API, repository, or
workflow alone does not establish the target boundary or operational maturity.

Release numbers express dependency order rather than dates. Before `1.0.0`, storage schemas, APIs,
configuration, Helm values, and upgrade procedures may change as described in
[VERSIONING.md](VERSIONING.md). During the `0.2` decomposition the current lockstep product version
remains transitional; independent component versions are introduced only after contract and artifact
boundaries are ready.

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

## 0.2 — Independent component architecture

**Outcome:** Observable is composed from independently buildable, versioned, deployable components
with explicit contracts and state ownership, and the full product is assembled and verified by a
separate distribution component.

**Depends on:** The `0.1.0` evaluation baseline.

**Target components:**

- `observable-contracts`
- `observable-auth`
- `observable-control`
- `observable-ingest`
- `observable-process`
- `observable-store-clickhouse`
- `observable-query`
- `observable-alerting`
- `observable-web`
- `observable-distribution`

**Scope:**

- Define source, API, event, PostgreSQL, and ClickHouse ownership for every target component.
- Separate cross-component wire contracts from shared Rust implementation and publish versioned
  OpenAPI/event contracts.
- Replace the processor-to-storage HTTP path with a durable Redpanda
  `telemetry.normalized.v1` boundary.
- Remove control-plane PostgreSQL ownership from ingest and query.
- Consolidate tenant/config/dashboard/deployment/change-event state into `observable-control`.
- Consolidate alert/SLO/firing/notification/incident state into `observable-alerting`.
- Establish logical PostgreSQL ownership with separate credentials and no cross-owner SQL.
- Make `observable-store-clickhouse` the owner of ClickHouse writes/migrations and declare a tested
  storage-schema compatibility range for `observable-query`.
- Replace the shared `observable-services` image with one image and release lifecycle per
  deployable component.
- Introduce a distribution manifest that pins a tested set of component versions.
- Extract repositories one at a time only after each component builds and tests without sibling
  source.

**Exit evidence:**

- Every source area, API, event/topic, PostgreSQL table, and ClickHouse migration has one documented
  owner.
- No deployable component imports another deployable component's source or relies on an unpublished
  shared implementation contract.
- No component executes SQL against another component's PostgreSQL ownership boundary.
- Ingest runs with auth + Redpanda only; core query runs without PostgreSQL.
- Processing can continue through a storage outage within Redpanda retention limits and storage can
  recover by consuming the durable normalized backlog.
- Changing one component does not rebuild unrelated component images.
- Every component can release independently with SemVer, changelog, SBOM, and provenance.
- `observable-distribution` installs a pinned component set and passes Compose/kind E2E,
  compatibility, upgrade, and rollback verification.
- The final repositories build from clean checkouts without sibling component source.

**Non-goals:** Rewriting working capabilities for style, introducing a service mesh, physically
splitting every database on day one, replacing ClickHouse/Redpanda/PostgreSQL, or performing a
big-bang repository move.

## 0.3 — Effortless self-hosted evaluation

**Outcome:** A new self-hoster can evaluate the independently packaged Observable distribution
through Docker Compose without building from source or discovering undocumented lifecycle steps.

**Depends on:** Independent component artifacts and distribution ownership from `0.2`.

**Scope:**

- Verify fresh installation exclusively from published, versioned distribution artifacts on
  supported hosts.
- Make first telemetry, health diagnosis, reset, upgrade, backup, and restore repeatable from the
  documented Compose path.
- Publish clear prerequisites, resource expectations, failure messages, and troubleshooting data.
- Close the `0.1.0` release-artifact evidence debt, including signatures and attestations.
- Verify component-version compatibility from the distribution manifest rather than relying on one
  source commit.

**Exit evidence:**

- A clean-host evaluator reaches first telemetry within the documented flow and time budget.
- Automated release-candidate checks install, upgrade, back up, restore, reset, and smoke-test the
  published distribution artifacts.
- Release documentation identifies supported host assumptions and actionable diagnostics for every
  critical dependency.

**Non-goals:** Production support for Compose, broad HA, fleet management, billing, and advanced
telemetry signals.

## 0.4 — Operator-ready Kubernetes

**Outcome:** A platform team can operate one supported Kubernetes topology with explicit capacity,
failure, and lifecycle boundaries.

**Depends on:** Repeatable distribution artifacts and lifecycle procedures from `0.3`.

**Scope:**

- Verify Helm installation and upgrades from the published distribution chart and versioned
  component images.
- Harden configuration, secrets, ingress, persistent storage, migrations, rollback, and recovery for
  the supported topology.
- Provide capacity guidance, platform health dashboards, alerting, and runbooks for critical paths.
- Exercise dependency interruption, restart, backup/restore, and bounded-degradation behavior in a
  representative Kubernetes environment.

**Exit evidence:**

- Automated clean install, upgrade, rollback, and recovery checks pass against published artifacts.
- Measured capacity envelopes and dependency-failure drills have explicit pass/fail thresholds.
- Operators can diagnose ingest, query, storage, queue, identity, authorization, and component
  compatibility failures using supported telemetry and runbooks.

**Non-goals:** Multi-region operation, active-active control planes, arbitrary Kubernetes
distributions, or a broad HA claim beyond the tested topology.

## 0.5 — Governed team adoption

**Outcome:** Multiple teams can share a deployment with enforceable usage, retention, access, and
accountability controls.

**Depends on:** A supportable Kubernetes operational baseline from `0.4`.

**Scope:**

- Enforce tenant-aware ingest limits, quotas, and cardinality budgets with visible rejection
  behavior.
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

## 0.6 — Service reliability workflow

**Outcome:** A service owner can move from detection through triage, notification, and review
without manually reconstructing context across tools.

**Depends on:** Governed team and data boundaries from `0.5`.

**Scope:**

- Mature trace/log/metric, service/topology, infrastructure, deployment, and change-event
  correlation.
- Complete SLO and error-budget workflows, alert routing, suppression, escalation integrations, and
  incident timelines.
- Connect service ownership, impact context, runbooks, and reliability review views.
- Provide versionable import/export for supported dashboards, alerts, SLOs, and related
  configuration.

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

**Depends on:** Demonstrated component independence, evaluation, Kubernetes operations, governance,
and reliability workflows from `0.2` through `0.6`.

**Scope:**

- Define and verify stable public APIs, event contracts, component compatibility, configuration,
  Helm values, storage migrations, and upgrade compatibility under semantic versioning.
- Complete security review and tenant-isolation evidence for supported identity and deployment paths.
- Publish tested performance/capacity envelopes, operational SLOs, recovery objectives, and runbooks.
- Establish release support boundaries, vulnerability handling, migration policy, and artifact
  provenance for the supported topology.

**Exit evidence:**

- Distribution upgrade suites cover every supported predecessor and preserve documented data and
  configuration.
- Contract and storage-schema compatibility suites cover every supported component-version range.
- Security, tenant-escape, load, soak, chaos, backup/restore, and disaster-recovery gates pass
  against release candidates with retained evidence.
- Documentation and support policy enumerate supported versions, platforms, dependencies, component
  combinations, and known limitations without relying on pre-`1.0` instability exceptions.

**Non-goals:** Feature parity with commercial suites, LTS branches unless separately adopted,
multi-region active-active operation, or making every post-`1.0` theme part of the stable core.

## Beyond 1.0 themes

These are unordered candidates, not commitments or blockers for `1.0.0`:

- Browser RUM, mobile observability, session replay, continuous profiling, and synthetics.
- Advanced AI assistance, anomaly models, capacity forecasting, and approval-gated remediation.
- Regional residency, compliance reporting, bring-your-own-key, billing, and marketplace packaging.
- Multi-region operation, broader HA topologies, fleet management, and additional ecosystem
  adapters.

Accepted but unimplemented ADRs describe architectural direction where one exists; they do not
assign a release. Prioritization happens through reviewed issues and small vertical slices.
