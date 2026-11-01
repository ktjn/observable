# Observable Roadmap

> **Status:** Active and authoritative.
>
> This is the sole release-sequencing roadmap for Observable. It describes outcomes and evidence,
> not delivery dates or a complete implementation backlog. Work is decomposed into small GitHub
> issues and pull requests according to [spec/10-process.md](spec/10-process.md).

## Direction

Observable is an open-source, full-stack observability platform. The immediate roadmap goal is to
turn the existing multi-service monorepo into a set of build-isolated, logically independent
components with explicit contracts and state ownership, governed by the machine-checkable System
Specification ([spec/system.yaml](spec/system.yaml)).

The decomposition target is defined in
[docs/component-decomposition.md](docs/component-decomposition.md),
[ADR-035](spec/adr/ADR-035-component-independence.md), and
[spec/22-system-specification.md](spec/22-system-specification.md). Full component isolation is
enforced at build time and runtime within the monorepo: contracts, state ownership, runtime boundaries,
isolated build targets, and compatibility checks ensure components evolve independently without
requiring physical repository splits or separate release publishing machinery.

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

**Outcome:** Observable is composed from build-isolated, logically independent components with
explicit contracts and state ownership, and the full product is assembled and verified as a
cohesive distribution following the machine-checkable System Specification ([spec/system.yaml](spec/system.yaml)).

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

- Define source, API, event, PostgreSQL, and ClickHouse ownership for every target component in `spec/system.yaml`.
- Separate cross-component wire contracts from shared Rust implementation.
- Replace the processor-to-storage HTTP path with a durable Redpanda `telemetry.normalized.v1` boundary.
- Remove control-plane PostgreSQL ownership from ingest and query.
- Consolidate tenant/config/dashboard/deployment/change-event state into `observable-control`.
- Consolidate alert/SLO/firing/notification/incident state into `observable-alerting`.
- Establish logical PostgreSQL ownership with separate credentials and no cross-owner SQL (completed across all components; isolated component users `observable_auth`, `observable_control`, `observable_alerting` provisioned with default search paths).
- Make `observable-store-clickhouse` the owner of ClickHouse writes/migrations with idempotent deduplicated batch inserts.
- Build each component independently via service-scoped Docker targets (`component-runtime` with `SERVICE=<component>`) and scoped CI compilation (`scripts/ci_changed_components.py`).
- Maintain a distribution manifest (`distribution/manifest.yaml`) that tracks component versions.
- Enforce component boundaries, acyclic dependencies, and build-time isolation continuously via `spec/system.yaml`.
- Per-component external registry release publishing and physical multi-repository extraction are superseded in favor of monorepo build-time isolation and cohesive distribution release.

Local release tooling now verifies the pinned base/frontend indexes and requested amd64/arm64
manifests against Docker Hub; live checks passed for both recipe families. This is partial registry
evidence, not independent artifact publication. Authenticated history, complete build materials and
publication safety remain open in [Remaining 0.2 work](docs/component-decomposition.md#remaining-02-work).

Web npm material preparation now verifies all locked archive bytes and supplies an isolated native
offline install (421 archives, 434 records). Offline image builds and complete release identity remain
open; see [material acceptance](distribution/README.md#verified-npm-materials-and-offline-installation).
Native committed web compilation now consumes those materials successfully and inventories 11 static
files; [compilation evidence](distribution/README.md#native-web-compilation-from-verified-materials)
keeps network isolation and OCI acceptance explicit and open.

The [offline web OCI build](distribution/README.md#offline-web-oci-image-builds) now passes on amd64 and emulated arm64:
verified npm archives feed a network-disabled, uncached BuildKit install/compile and the immutable
images serve static assets. Native arm64 hardware, Rust materials, authenticated provenance/history and
publication protection remain open.

The [Cargo material stage](distribution/README.md#verified-cargo-materials-and-frozen-resolution)
now verifies all 526 locked registry archives and resolves the full workspace offline in the pinned
Rust container. Auth-service also compiles frozen/offline and passes linkage there without new
apt/rustup inputs. Other native/system materials and deployable runtime acceptance remain open.


The [APT material preparer](distribution/README.md#locked-apt-materials-and-offline-installation) records native signed
Debian resolution for the pinned Rust builder/runtime bases on amd64 and arm64: 16 builder and
18 runtime archives per platform. A committed lock governs later HTTPS fetch and byte-revalidated
staging; generated installers guard base dpkg state, select only locked archives and prohibit
downloads. All four closures installed in network-disabled containers with exact dpkg versions,
tool/CA/linkage checks and unlisted archives ignored; a missing archive failed before installation.
Arm64 used emulation. This supplies native package material evidence, not a completed service image,
reproducibility or publication authority.

The [offline Rust image builder](distribution/README.md#offline-rust-oci-image-builds) now combines Git-only workspace
source with reverified Cargo archives and committed builder/runtime APT closures. Its generated
recipe disables RUN networking and cache reuse, compiles a frozen release and preserves the non-root
runtime/proto/entrypoint contract. Inspected image digests, platform, labels and runtime configuration
must match the local request. Component/platform runtime acceptance is separate; full release identity
and publication authorization remain false. Auth amd64 built successfully and passed non-root
runtime linkage/CA/proto checks, exact runtime package versions and HTTP 200 `/health`/`/readyz`
against an isolated disposable PostgreSQL instance. Other Rust components/platforms, migrations,
OIDC and full cluster acceptance remain open.

The [process image acceptance](docs/component-process-offline-verification.md) additionally exercises native Kafka/CMake compilation and
real log delivery. Broker-loss verification exposed a transient receive error terminating process;
the source fix now preserves the consumer across transport loss, with fatal/other errors still
propagated. Testcontainers stop/restart recovery and focused package checks pass. The corrected
image passed non-root runtime checks, raw-to-normalized log delivery and broker-loss readiness 503
while liveness remained 200. Other component/platform and full durability/integration gates remain open.

The [storage image acceptance](docs/component-store-offline-verification.md) now passes on amd64: the
actual offline-built executable persisted a normalized broker log with all 15 fields preserved
against the four committed ClickHouse migrations. Non-root runtime checks and ClickHouse-loss
readiness 503/liveness 200 also passed. Offset durability, dependency recovery, other signals and
remaining component/platform acceptance remain open; release identity/publication flags stay false.

The [ingest image acceptance](docs/component-ingest-offline-verification.md) passes on amd64 using the
actual offline auth image, migrated PostgreSQL and Redpanda. Missing/invalid tokens returned 401
without advancing the raw topic; authenticated OTLP logs reached the broker with token-bound
tenant/environment and all fields checked. Runtime and broker-loss readiness/liveness checks
passed. Recovery, other signals/platforms and full release authority remain open.

The [control image acceptance](docs/component-control-offline-verification.md) passes on amd64: actual auth,
PostgreSQL persistence and both deployment create/finish broker events were checked. Rejected
writes changed neither rows nor offsets. During broker loss, another deployment still persisted
with 201 and PG-only readiness stayed 200, matching the existing best-effort event policy. Other
control paths, full target independence, recovery/platform and release-authority gates remain open.

The [alerting image acceptance](docs/component-alerting-offline-verification.md) passes on amd64: the actual
background worker created, updated without a duplicate ID and resolved a threshold firing from
real ClickHouse samples. Authenticated tenant-scoped reads, cross-tenant detail 404s, runtime
checks and ClickHouse-loss readiness 503/liveness 200 passed. Other rule types, notifications,
incidents, recovery/platform and release-authority gates remain open.

The [query image acceptance](docs/component-query-offline-verification.md) passes on amd64: actual log
reads round-trip all 15 fields with tenant-scoped search/facets/tail/context, rejected tenant
headers and cross-tenant pivot 404s. PostgreSQL contains the exact 10 query/15 credential audit
events. Runtime and ClickHouse-loss readiness/liveness passed. All seven Rust components now have
[recorded amd64 offline builds](distribution/README.md#rust-amd64-acceptance-inventory) at their listed revisions; this is not a single-revision distribution
or full-runtime acceptance. Rust arm64, remaining runtime/cluster and release-authority gates stay open.

The [release history and publication boundary adapter](distribution/README.md#release-history-and-publication-boundary-adapter)
bridges registry inspection, attestation verification and authoritative release history into
verified observations for the release planner, enforcing live reinspection, writer serialization
and tag overwrite prevention.

The [verified build-input identity](distribution/README.md#verified-build-input-identity)
verifier binds source closure integrity, package manifest version parity, pinned base image manifests,
and locked Cargo/APT/npm materials, yielding authenticated records with identity_verified: true. Live publishing,
Rust arm64 and full cluster gates remain open.

Cross-owner SQL from `observable-query` against `control` tables (`schema_entries`, `semantic_annotations`, `platform_config`) has been completely eliminated. `query-api` retrieves schema metadata and platform configuration via internal HTTP endpoints exposed by `admin-service` (`/internal/schemas/*`, `/internal/config/llm`), preserving component ownership boundaries without direct database access.

**Exit evidence:**

- Every source area, API, event/topic, PostgreSQL table, and ClickHouse migration has one documented
  owner in `spec/system.yaml`.
- No deployable component imports another deployable component's source or relies on an unpublished
  shared implementation contract (enforced by `scripts/check-component-boundaries.sh` with zero allowlist exceptions).
- No component executes SQL against another component's PostgreSQL ownership boundary; isolated
  login roles (`observable_auth`, `observable_control`, `observable_alerting`) are verified.
- Ingest runs with auth + Redpanda only; core query runs without PostgreSQL.
- Processing can continue through a storage outage within Redpanda retention limits and storage can
  recover by consuming the durable normalized backlog.
- Changing one component does not rebuild unrelated component images (enforced by `component-runtime`
  Docker build targets and service-scoped CI).
- The system specification (`spec/system.yaml`) validates cleanly at release stage with all invariants satisfied.
- `observable-distribution` installs the platform and passes Compose/kind E2E, compatibility, and smoke verification.

**Non-goals:** Rewriting working capabilities for style, introducing a service mesh, physical
multi-repository extraction, separate per-component external release publishing, replacing
ClickHouse/Redpanda/PostgreSQL, or performing a big-bang rewrite.


### Formal verification gate

Stateful cross-component correctness in the 0.2 architecture is verified with bounded TLA+/TLC
models in addition to contract, integration, and chaos tests. The implementation specification is
[spec/19-formal-verification.md](spec/19-formal-verification.md).

- [x] Model the `telemetry.normalized.v1` process-to-storage durability boundary, including
  acknowledgement ordering, storage outage, worker crash/recovery, redelivery, duplicates, and
  backpressure.
- [x] Add pinned, bounded TLC execution to PR CI with actionable counterexample traces.
  Runs as the `scripts/local-ci.sh` "Formal verification (TLC)" step (ADR-019 keeps validation off
  GitHub workflows): the tool is pinned by sha256, each model has a wall-clock budget, and
  `.tla/<model>.log` preserves the trace.
- [x] Map formal invariants to concrete Testcontainers/integration/chaos regression boundaries.
  See the "Regression boundaries" table in `formal/tla/README.md`.
- [x] Model component-version, storage-schema, distribution-manifest, upgrade, and rollback
  compatibility as independent component releases become executable.
  `formal/tla/ComponentCompatibility.tla` verifies the composition rules (incompatible manifests are
  rejected; query never serves on an unsupported schema); per-component image publishing remains open,
  so version identity is abstract.

**Exit evidence:** deliberately broken acknowledgement/order transitions produce useful TLC
counterexamples; the committed finite models pass in PR CI; and TLA+ remains development-only with
no production runtime dependency.

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

The evaluation guide ([docs/evaluation-guide.md](docs/evaluation-guide.md)) is synchronized with current Compose ports (frontend `5173`, Zitadel `8082`), standard evaluation credentials, and pinned collector configurations. Evaluators can diagnose prerequisites, port conflicts, container health, and endpoint availability via the platform doctor ([scripts/doctor.sh](scripts/doctor.sh), `make doctor`). Automated backup and restore drills ([scripts/backup-restore-drill.sh](scripts/backup-restore-drill.sh)) verify PostgreSQL component schema persistence (`auth`, `control`, `alerting`) and ClickHouse table datasets. Release-candidate test suites ([scripts/release-candidate-suites.sh](scripts/release-candidate-suites.sh)) sequence doctor diagnostics, backup/restore drills, dependency failure, chaos probes, and pinned distribution installation ([scripts/pinned-install.sh](scripts/pinned-install.sh)). Troubleshooting documentation ([docs/architecture.md](docs/architecture.md)) is aligned with durable Redpanda topics (`telemetry.raw.v1`, `telemetry.normalized.v1`), ClickHouse tables (`observable.spans`), and Zitadel readiness. Component-version compatibility is verified directly from the distribution manifest ([scripts/check_manifest_compatibility.py](scripts/check_manifest_compatibility.py)), enforcing that pinned component versions satisfy all cross-component SemVer capability requirements declared in the machine-checkable specification ([spec/system.yaml](spec/system.yaml)). Release-artifact evidence debt is closed via the release workflow ([.github/workflows/release-artifacts.yml](.github/workflows/release-artifacts.yml)), which produces SLSA build provenance and SPDX SBOM attestations for each individual component image alongside the Helm chart and distribution archive.

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

Operational runbooks ([docs/runbooks/README.md](docs/runbooks/README.md)) cover deployment regressions, ingest queue backpressure ([docs/runbooks/ingest-queue-failure.md](docs/runbooks/ingest-queue-failure.md)), ClickHouse storage recovery and consumer lag ([docs/runbooks/storage-recovery.md](docs/runbooks/storage-recovery.md)), identity and authorization failures ([docs/runbooks/auth-identity-failure.md](docs/runbooks/auth-identity-failure.md)), and dependency outage bounded-degradation procedures ([docs/runbooks/dependency-outage.md](docs/runbooks/dependency-outage.md)). Helm charts support secret hardening and least-privilege scoped database credentials via `valueFrom.secretKeyRef` and `envFrom` across all platform components ([charts/observable/values.production-example.yaml](charts/observable/values.production-example.yaml)). Platform health monitoring and critical-path Prometheus alerting rules and Grafana dashboards are codified in [docs/platform-alerting.md](docs/platform-alerting.md) and [charts/observable/templates/prometheus-rules.yaml](charts/observable/templates/prometheus-rules.yaml).

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
