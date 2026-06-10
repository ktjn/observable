# Observable — Observability Feature Parity Plan

> **Status:** Active — analysis and roadmap extension document.
> **Date:** 2026-06-04
> **Scope:** User-perspective feature gap analysis and prioritized roadmap extension against
> Datadog, New Relic, and Dynatrace. This document extends the active roadmap in
> `2026-05-07-remaining-roadmap-plan.md` and does not replace it.

---

## 0. How to Read This Document

This plan answers a single question: **from a user's daily workflow perspective, what does
Observable need to do to stand alongside Datadog, New Relic, and Dynatrace as a first-choice
observability platform?**

It is organized in five sections:

1. **Baseline** — what Observable already does well
2. **User workflow gap analysis** — what users try to accomplish that Observable cannot fully
   support today, mapped against the three competitor reference platforms
3. **Prioritized feature roadmap** — sequenced as new phases (P9–P14) that extend the active
   roadmap, each with user stories, acceptance criteria, and implementation notes
4. **Quick-win backlog** — features that are small enough to slot into existing phases or be
   shipped as standalone slices without waiting for a new phase gate
5. **Success metrics** — how to measure progress toward parity

---

## 1. Baseline — What Observable Does Well Today

Before identifying gaps, it is important to record what Observable already delivers. This list
is the foundation that all parity work extends.

| Capability | Observable today | Competitor equivalent |
|---|---|---|
| OTLP ingest (gRPC + HTTP) | ✅ traces, logs, metrics | Datadog (proprietary agent preferred); Dynatrace (secondary path) |
| Service catalog with entity-centric navigation | ✅ first-class Service entity | New Relic entity explorer; Dynatrace Smartscape |
| Distributed trace explorer | ✅ waterfall, span detail, trace comparison | Datadog APM trace view; Dynatrace PurePath |
| Log explorer with live tail | ✅ full-text search, facets, 5 s live tail | Datadog Log Explorer; New Relic Logs |
| Metric explorer | ✅ series browser, cardinality observe | Datadog Metrics Explorer |
| Dashboard builder | ✅ drag-and-drop, templates, react-grid-layout | Datadog Dashboards; Grafana |
| Dashboard-as-code | ✅ serializable artifact from creation | Grafana Git Sync; Datadog Terraform provider |
| Service map / topology | ✅ D3-based topology from spans | Datadog Service Map; Dynatrace Smartscape |
| SLO management + burn-rate alerts | ✅ service-level availability, multi-window | Datadog SLOs; New Relic SLIs/SLOs |
| Incident timeline + topology impact | ✅ correlated events, D3 subgraph | Datadog Incident Management |
| Composite alerts | ✅ two-rule pair evaluation | Datadog composite monitors |
| Notification channels | ✅ Slack, webhook | Datadog; New Relic |
| Runbook attachment to alerts | ✅ runbook_url on alert rules | Datadog; PagerDuty |
| Natural language queries (NLQ) | ✅ NLQ→IR→SQL pipeline | Datadog AI Investigator (partial) |
| Multi-tenancy | ✅ enforced at every layer | Grafana multi-tenant (requires stitching) |
| ReBAC authorization | ✅ OpenFGA, dashboard grants | Datadog Teams/RBAC (coarser) |
| SSO / OIDC | ✅ Zitadel 4.x PKCE flow | All competitors |
| Query workbench (Monaco) | ✅ multi-signal notebook | New Relic Query Builder; Datadog Notebooks |
| Reliability report per service | ✅ SLO, incident, deployment summary | Datadog Service Scorecard |
| Deployment markers + correlation | ✅ service-deployment enrichment | Datadog Change Tracking; New Relic deployments |
| Deployment regression detection | ✅ alert evaluator + deployment enrichment | Datadog Watchdog |
| Alert lifecycle (dedup, pending, resolution) | ✅ full state machine | All competitors |
| Self-observability for all services | ✅ /readyz, OTLP metrics | All competitors |

---

## 2. User Workflow Gap Analysis

This section groups gaps by **user workflow** — the sequence of steps a user performs in a
competing product that they cannot fully complete in Observable today. Each workflow maps to one
or more features.

---

### 2.1 Workflow: Getting Started (First Signal In)

**User goal:** install an agent, send the first trace, and see it in the UI in under 10 minutes.

**Datadog / New Relic experience:** a guided onboarding wizard asks what language/framework the
user's service is written in, provides a copy-paste install command, polls for first data, and
shows the first trace with a celebration state. A persistent "Setup Checklist" in the sidebar
tracks progress.

**Observable gap:** there is no onboarding wizard (listed as a Tier 2 gap in
`spec/00-market-analysis.md`). New users land on the Service Catalog with no guidance. First-time
API key generation requires navigating to Admin. There is no "first signal received" detection
or feedback.

**Impact:** this is a leading source of trial abandonment across all SaaS observability tools.
Observable's OTel-first model actually makes the install command shorter (any OTel SDK just needs
an endpoint and a header), but without a wizard the user does not know what to set.

**Features needed:**
- Onboarding wizard (language picker → install command → first-signal detection → celebrate)
- API key generation as a first-class, discoverable flow
- "Setup progress" indicator in the sidebar until the first trace/log/metric is received

---

### 2.2 Workflow: Reacting to an Exception in Production

**User goal:** an exception fires; the on-call engineer wants to (a) see all occurrences of this
exception grouped together, (b) find the most recent occurrence with a full stack trace and linked
trace, (c) assign it to a team member, (d) mark it resolved after a deploy, and (e) be alerted
if it regresses.

**Datadog / New Relic / Sentry experience:** error tracking products provide automatic
fingerprinting by exception type + stack trace, a grouped "error issue" view, occurrence
history, status (open/resolved/regressed), owner assignment, and automatic regression detection
when the same fingerprint appears after a "resolved" deploy.

**Observable gap:** Observable has alert rules that fire on error rate thresholds and a trace
explorer that shows spans with `status_code = Error`. There is no structured error issue:
exceptions are log events and span attributes, not tracked entities with a lifecycle. This is the
largest single daily-workflow gap for application developers (Tier 3 gap in the market analysis).

**Impact:** development teams use Sentry or Datadog Error Tracking as their primary triage surface
every day. Without it, Observable is an SRE/infra tool, not a developer tool.

**Features needed:**
- Error fingerprinting engine (group by exception type + normalized stack trace)
- Error Issues entity with status, occurrence count, owner, last-seen, first-seen
- Error Issues explorer (search, filter by service/env/status/owner)
- Regression detection: auto-reopen a resolved issue when the fingerprint recurs after a deploy
- Link from error issue to the causal span, trace, and log record

---

### 2.3 Workflow: Answering "Is My Service Healthy Right Now?"

**User goal:** open the product, see all services, immediately understand which services have
elevated error rates, high p99 latency, or degraded SLO burn.

**Datadog / New Relic experience:** the APM Service Overview page shows all services with their
current request rate, error rate, and latency (RED metrics) as colored health indicators. Clicking
a service opens a service detail page with pre-computed sparklines for the last hour.

**Observable gap:** the Service Catalog lists services but the health signal is limited to what
the NLQ or a pre-built dashboard exposes. There is no standardized RED-metrics summary card
per service in the catalog view. The service detail Overview tab exists but does not show a
pre-computed health summary with standard thresholds.

**Features needed:**
- Service health summary row in the Service Catalog (error rate %, p99 latency, request rate, SLO status badge) computed from span data
- Service health status derived automatically from SLO burn rate if an SLO exists, or from error rate threshold if not
- Color-coded health indicators (green/yellow/red) consistent with alert severity levels

---

### 2.4 Workflow: Investigating a Slow / Failed Request

**User goal:** a user reports a slow checkout. The engineer opens the trace for that request,
sees which service contributed the most latency, sees the database query that was slow, and jumps
directly from the span to the relevant log lines from the same request.

**Observable gap:** the trace waterfall and span-to-log correlation exist. The remaining gap is
**Database Monitoring** — Observable surfaces DB spans (query text, duration) but provides no
query plan analysis, no N+1 detection, no connection pool metrics, and no slow query leaderboard.
Datadog's Database Monitoring (DBM) and New Relic's Database UI are among the most-used features
for backend engineers.

**Features needed:**
- Slow query leaderboard per service (top N queries by mean duration, P99, call count) derived from span attributes
- N+1 detection: flag services where the same query appears >N times within a single trace
- Database span enrichment view on the service detail page (dedicated "Database" tab)
- Query normalization for common DB drivers (PostgreSQL, MySQL, Redis)

---

### 2.5 Workflow: Monitoring Infrastructure Resources

**User goal:** the ops team wants to know which hosts are running above 80% CPU, which Kubernetes
pods are crash-looping, and which containers were terminated in the last hour — without waiting
for a service to emit a trace.

**Datadog / New Relic / Dynatrace experience:** a persistent infrastructure catalog shows all
known hosts, pods, and containers derived from both live telemetry AND from the Kubernetes API
server (independent of signal flow). Hosts that have not emitted telemetry recently are still
visible with a "no data" state.

**Observable gap:** Observable derives all infrastructure context from OTel resource attributes on
active signal flow. A silent host is invisible. There is no persistent inventory. The k8s operator
(which could populate a catalog from the k8s API server) is specced but unimplemented.

**Features needed:**
- Infrastructure catalog: persistent, queryable inventory of hosts, pods, containers, namespaces, clusters
- K8s operator that populates the catalog from the Kubernetes API server independent of telemetry
- Infrastructure explorer page: filterable list with CPU, memory, network metrics aggregated per resource
- Host / pod / container detail page: current metrics + active processes + recent log lines + related spans
- "No data" / "stale" state for resources that have not emitted telemetry recently

---

### 2.6 Workflow: Measuring Performance of Real Users

**User goal:** the product team wants to know what page-load performance real users experience,
which JavaScript errors are most frequent, and whether a deploy degraded Core Web Vitals.

**Datadog / New Relic experience:** Browser RUM captures page views, route changes, resource
loading, Core Web Vitals (LCP, CLS, FID/INP), JS errors, and user sessions. Sessions can be
replayed. RUM data links to backend traces via the injected `traceparent`.

**Observable gap:** browser RUM is Phase 6 (P6-S2), not yet started.

**Features needed:**
- Browser SDK: lightweight JavaScript agent (< 15 KB gzip) for Web Vitals, JS errors, route changes, resource timings
- Session model: anonymous session ID injected at page load, propagated through fetch/XHR as `traceparent`
- Web Vitals dashboard: LCP, CLS, INP/FID, TTFB per page, percentile distributions
- JS Error explorer: grouped by message + stack, linked to affected sessions
- Session list view: filter by duration, error count, impacted JS errors, device type
- Session replay (conditional on privacy review — see P6-S6 in the active roadmap)

---

### 2.7 Workflow: Verifying Service Availability from the Outside

**User goal:** the SRE team wants scheduled checks that hit the login endpoint from multiple
locations every 60 seconds and alert immediately when it fails from any location.

**Datadog / New Relic experience:** Synthetic Monitoring runs lightweight scripted checks (HTTP,
multi-step browser, gRPC) from global PoPs. Failures trigger alerts. Checks inject `traceparent`
so the resulting backend trace is linked to the check result.

**Observable gap:** synthetics are Phase 6 (P6-S4), not yet started. Only basic HTTP checks are
referenced; multi-step and browser synthetic checks are Tier 2 gaps.

**Features needed:**
- HTTP synthetic check: scheduled GET/POST to a URL, configurable headers, assertion on status code / response body / latency
- Multi-step synthetic check: sequential HTTP request chain (e.g., login → API call → assert)
- Synthetic check results explorer: history, success/failure rate, latency, location breakdown
- Alert on synthetic failure: reuse existing alert rule model, synthetic check as a metric source
- W3C traceparent injection on synthetic requests so backend traces are linked

---

### 2.8 Workflow: Finding What Changed When Something Broke

**User goal:** an error rate spiked at 14:35. The engineer wants to know if a deploy, config
change, feature flag flip, or cloud event coincided with the spike.

**Datadog experience:** Change Tracking correlates deployments, feature flag changes, and
database schema migrations with metric anomalies on the same timeline. An "Events" overlay on
any dashboard shows change events at the time of the anomaly.

**Observable gap:** deployment markers exist and deployment-correlation is implemented. Feature
flag change events are not ingested. Generic "change event" types (DB migration, config change)
are not a first-class concept. The timeline overlay on dashboards does not yet include custom
change event types.

**Features needed:**
- Generic change event API: POST a change event (type, service, description, metadata) from any CI/CD pipeline or deployment tool
- Change event types: `deploy`, `config_change`, `feature_flag`, `db_migration`, `infra_change`
- Dashboard change event overlay: render change events as vertical markers on any time series graph
- Change event explorer: searchable list of all change events with filtering by type, service, time range

---

### 2.9 Workflow: Setting Up Alerts Without Writing Code

**User goal:** a non-engineer team member wants to create an alert that fires when the "payment
processing" service error rate exceeds 1% for 5 minutes, and routes it to the payments Slack
channel, with an escalation to PagerDuty if unacknowledged after 15 minutes.

**Datadog / New Relic experience:** alert creation wizard with signal picker (metric, log count,
trace error rate, anomaly), threshold configuration, notification channel multi-select, and
escalation policy builder. All driven by form UI with no query language required.

**Observable gap:**
- Alert routing UI exists for Slack and webhooks, but there is no escalation policy builder
- PagerDuty and Opsgenie integrations are specced but not implemented (Tier 2 gap)
- Deadman alerts and change-detection alerts are specced but not implemented
- Alert inhibition rules (storm suppression) are specced but not implemented

**Features needed:**
- PagerDuty / Opsgenie notification channel adapters (two of the most-requested integrations)
- Escalation policy builder: define primary channel, escalation trigger (N minutes without ack), secondary channel
- Deadman alert type: "fire if this service has emitted no telemetry for N minutes"
- Change-detection alert type: "fire if this metric changes by X% vs the same window N days ago"
- Alert inhibition rules: suppress lower-severity alerts for a service when a critical alert is firing
- On-call schedule view: read-only display of the current on-call responder per team (sourced from PagerDuty/Opsgenie)

---

### 2.10 Workflow: Profiling CPU and Memory Bottlenecks

**User goal:** a service is using 3× more CPU after a recent deploy. The engineer wants a flame
graph showing where time is spent, correlated with the specific trace IDs that were slow.

**Datadog / Pyroscope experience:** Continuous Profiler captures wall-clock / CPU / heap profiles
from running processes at low overhead (< 2% CPU). Profiles are indexed by service, version, and
host. A flame graph viewer allows diff comparison between two time windows. Slow traces are
linked to their profiling context via `profile_id`.

**Observable gap:** continuous profiling is Phase 6 (P6-S1, blocked on P4-S1 warm retention for
object storage). The profiling spec and domain model exist.

**Features needed (when P4-S1 object storage unlocks P6-S1):**
- pprof/OTLP profile ingestion endpoint
- Profile store backed by object storage (S3-compatible)
- Flame graph viewer (icicle / flamechart) with click-to-zoom and function-level detail
- Profile comparison: baseline window vs anomaly window, with differential highlighting
- Profile-to-trace linking: slow spans carry a `profile_id` that opens the profiling context

---

### 2.11 Workflow: Tracking Engineering Velocity and Reliability

**User goal:** the engineering director wants to see deployment frequency, lead time, change
failure rate, and MTTR over the last quarter — the four DORA metrics — per team.

**Datadog / LinearB experience:** DORA metrics are derived from deployment events, incident
records, and alert timelines. Most tools surface them as a report view.

**Observable gap:** DORA metrics are a Tier 3 gap — not specced anywhere. Observable has all the
raw data (deployment markers, incidents, alerts) to compute them once Phase 5 is complete.

**Features needed:**
- DORA metrics report: deployment frequency, lead time (deploy event → alert clearance), change failure rate (deploys that triggered an incident), MTTR (incident open → resolved)
- Team-scoped DORA view: filter by team ownership of services
- DORA trend chart: sparklines over rolling 30/90 day windows
- Export as CSV for engineering leadership reporting

---

### 2.12 Workflow: Managing Telemetry Cost

**User goal:** the platform team is billed for 50M events/day more than last month. They want to
know which service, which attribute, or which log pattern caused the increase, and throttle it
without a code change.

**Datadog experience:** the Data Intake & Costs page shows ingest by product line, top tags by
volume, and an estimated cost impact. Ingestion controls let operators cap custom metric volume
per tag value.

**Observable gap:** the tenant usage report (P4-S7) provides a relative usage index but no
breakdown by attribute, service, or log pattern. Cardinality observation is metric-only (P2-S3a).
Sampling rules cannot be managed from the UI.

**Features needed:**
- Ingest breakdown: top N services/attributes/log patterns by event volume over a time window
- Sampling rules UI: create, preview (simulated volume reduction), and push tail-sampling rules to agents via OpAMP
- Cost impact preview: estimated reduction in events/day when a sampling rule is applied
- Cardinality budget UI: set per-service metric cardinality limits and visualize current usage vs budget
- Rate limit override UI: temporarily raise or lower ingest rate limits per tenant/service

---

### 2.13 Workflow: Migrating from Prometheus / Grafana

**User goal:** a team running Prometheus + Grafana wants to migrate to Observable. They have 400
alert rules in Prometheus alerting rule format and 80 Grafana dashboard JSON files.

**Observable gap:**
- Prometheus remote_write receiver is ADR-017-ratified but has no implementation slice
- Prometheus alert rule import endpoint does not exist (Tier 3 gap)
- No Grafana dashboard import path
- No PromQL query support (P8-S7 is in the plan but optional)

**Features needed:**
- Prometheus remote_write receiver: accept Prometheus metric payloads directly at a `/api/v1/write` endpoint and convert to the Observable metric schema
- Prometheus alert rule importer: translate PromQL alerting expressions to Observable alert rule format with a mapping report
- PromQL query façade (P8-S7): accept PromQL expressions in the query workbench and translate to the NLQ IR for execution

---

### 2.14 Workflow: Self-Service Agent Onboarding for Platform Teams

**User goal:** the platform team manages 300 services across 15 teams. They want to see all agent
instances, their versions, buffer states, and last export times — and push a config update to a
fleet segment without touching each host.

**Datadog / Dynatrace experience:** an agent fleet view shows all reporting agents with health
status, version, connection state, and queue depth. Operators can push config changes, trigger
agent upgrades, or restart agents remotely.

**Observable gap:** the Fleet page (`/admin/fleet`) is a read-only contract surface. There is no
live agent inventory, no remote config push UI, and no OpAMP implementation (Tier 2 gap).

**Features needed:**
- Live agent inventory: real-time list of registered agents with health, version, last export time, buffer depth
- Agent detail view: connection history, config version, recent errors, resource consumption
- Remote config push: select a fleet segment, preview config diff, push signed config via OpAMP
- Agent upgrade workflow: schedule or trigger version upgrades across a fleet segment
- Fleet health summary: % of agents reporting, % on the latest config, % on the latest version

---

## 3. Prioritized Feature Roadmap (Phases P9–P14)

This section defines the new phases that implement the workflows in Section 2. Each phase has
clear entry gates, a primary user outcome, and an ordered slice list.

---

### Phase P9 — Developer Experience Essentials

**User outcome:** application developers adopt Observable as their daily triage tool, not just an
SRE infrastructure dashboard.

**Entry gate:** Phase 5 (Reliability Product) is complete and stable.

**Exit gate:** a developer can complete the error-triage workflow (section 2.2) end-to-end inside
Observable without opening Sentry or another error tracker.

#### P9-S1: Onboarding Wizard (Quick Win — could slot into Phase 4 as P4-S10)

**User story:** "As a new user, I want to be guided from zero to my first trace in the UI in
under 10 minutes."

**Acceptance criteria:**
- Step 1: select language/framework (Node.js, Python, Java, Go, Ruby, .NET, other)
- Step 2: display copy-paste install command with the user's ingest endpoint and API key pre-filled
- Step 3: poll `GET /v1/services` every 3 seconds; show a "waiting for first signal" spinner
- Step 4: when a signal arrives, show a success state with a link to the first trace/log
- Accessible from the sidebar as "Getting Started" until the checklist is complete
- API key generation is embedded in the wizard (no separate Admin navigation required)

**Implementation notes:**
- Frontend: `features/onboarding/` with a multi-step wizard component
- Backend: `GET /v1/setup/status` endpoint that checks whether any signal has been received
  for the authenticated tenant; returns `{first_trace: bool, first_log: bool, first_metric: bool}`
- The API key generation form already exists in Admin; the wizard invokes the same API

**Files affected:** `apps/frontend/src/features/onboarding/`, `services/query-api/src/setup.rs`,
`apps/frontend/src/components/shared/Sidebar.tsx`

---

#### P9-S2: Error Tracking Ingestion and Fingerprinting

**User story:** "As a developer, I want exceptions to be grouped automatically so I can see all
occurrences of the same error without reading individual log lines."

**Acceptance criteria:**
- The ingest-gateway extracts error fingerprints from span events with `exception.type` +
  `exception.stacktrace` attributes on `status_code = Error` spans
- Fingerprints are normalized: line numbers and memory addresses are stripped; module paths are
  truncated to the stable prefix
- A new `error_issues` ClickHouse table stores: `fingerprint`, `service_name`, `exception_type`,
  `exception_message_template`, `first_seen`, `last_seen`, `occurrence_count`, `status`
  (open / resolved / regressed), `owner_id`, `deployment_id_first_seen`, `deployment_id_last_seen`
- `GET /v1/errors` returns paginated error issues with filter support
- Error issues are created or updated (last_seen, occurrence_count) at ingest time by the
  stream-processor

**Files affected:** `libs/domain/src/span.rs`, `services/stream-processor/`, `services/query-api/`,
`migrations/`, `contracts/`

---

#### P9-S3: Error Issues Explorer UI

**User story:** "As a developer, I want to browse all open error issues for my service, see the
occurrence trend, and click through to a representative trace."

**Acceptance criteria:**
- New route `/errors` with `features/errors/` feature directory
- Error issue list: service filter, status filter (open/resolved/regressed), sort by last-seen / occurrence count
- Error issue detail: exception type, normalized message, occurrence graph (sparkline), list of
  most recent spans with trace links, first/last deployment seen
- "Assign to me" and "Mark resolved" actions on the issue detail page
- Badge count of open errors shown on the Service Catalog row for each service

**Files affected:** `apps/frontend/src/features/errors/`, `apps/frontend/src/features/services/`

---

#### P9-S4: Error Issue Regression Detection

**User story:** "As a developer, when I mark an error resolved after a deploy, I want to be
automatically notified if the same error appears again in a future deploy."

**Acceptance criteria:**
- When an issue is marked `resolved`, the `resolved_at_deployment_id` is recorded
- If the same fingerprint appears in a span whose `deployment_id > resolved_at_deployment_id`,
  the issue status transitions to `regressed` and a notification is dispatched on the issue's
  notification channel
- The alert evaluator can evaluate an `error_regression` rule type that checks for regressed issues

**Files affected:** `services/stream-processor/`, `services/alert-evaluator/`, `migrations/`

---

#### P9-S5: Service Health Summary in Catalog

> **Status:** Partially implemented. The Service Catalog UI, RED-metric columns, and
> error-rate-threshold health badges already ship (`apps/frontend/src/pages/ServicesPage.tsx`,
> `services/query-api/src/discovery.rs`). `docs/superpowers/plans/2026-06-10-p9-s5-service-catalog-health-signals.md`
> is promoted to close the remaining gaps: real `active_alert_count` (SLO-linked alerts only —
> see that plan's documented limitation), real `latest_deployment`, and an SLO-burn-rate override
> for `health_state`. Still open after that plan: the fast-vs-slow burn red/yellow distinction,
> 30s background-poll refresh, and the open error-issue count (blocked on P9-S2).

**User story:** "As an SRE, I want to open the service catalog and immediately see which services
are unhealthy without clicking into each one."

**Acceptance criteria:**
- Each service catalog row shows: error rate (%), p99 latency (ms), request rate (req/min),
  SLO status badge (green/yellow/red based on burn rate or error rate threshold), open error
  issue count
- Health status color is: green (error rate < 0.1% AND no active SLO burn), yellow (error rate
  0.1–1% OR slow SLO burn), red (error rate > 1% OR fast SLO burn OR active incident)
- Data is refreshed every 30 seconds via TanStack Query background polling
- No additional API queries are fired per service row — one batch API call returns all summaries

**Files affected:** `services/query-api/src/services.rs`, `apps/frontend/src/features/services/`

---

### Phase P10 — Infrastructure and Resource Monitoring

**User outcome:** operations teams can answer "what is the state of my infrastructure?" without
relying entirely on active telemetry signal flow.

**Entry gate:** Phase P9 complete (developer experience stable); K8s operator design agreed.

**Exit gate:** an ops team can navigate from a Kubernetes namespace to its pods, their CPU/memory
metrics, recent logs, and related traces in a single consistent UI flow.

#### P10-S1: Infrastructure Catalog Data Model

**User story:** "As an ops engineer, I want to see all known hosts and pods even when they are
not currently emitting telemetry."

**Acceptance criteria:**
- New `infrastructure_resources` PostgreSQL table: `resource_id`, `resource_type`
  (host/pod/container/namespace/cluster), `name`, `labels` (JSONB), `tenant_id`, `last_seen_at`,
  `status` (active/stale/terminated)
- K8s operator (new service `k8s-operator`) queries the Kubernetes API server and upserts
  resources into the catalog every 30 seconds
- Resources that have not been seen by the API server for more than 5 minutes are marked `stale`;
  after 24 hours they are marked `terminated`
- `GET /v1/infrastructure/resources` accepts filter params: `type`, `cluster`, `namespace`,
  `status`, returns paginated results with latest metric snapshot attached

**Files affected:** `services/k8s-operator/` (new service), `services/query-api/`,
`migrations/`, `contracts/`

---

#### P10-S2: Infrastructure Explorer UI

**User story:** "As an ops engineer, I want a filterable inventory of all infrastructure
resources with their current health state."

**Acceptance criteria:**
- New route `/infrastructure` with `features/infrastructure/` feature directory
- Top-level tabs: Hosts, Kubernetes (Clusters → Namespaces → Pods/Containers)
- Each resource row shows: name, status badge, CPU %, memory %, last-seen, related service count
- Resource detail page: current metrics (time series), recent log lines, related spans, tags

**Files affected:** `apps/frontend/src/features/infrastructure/`

---

#### P10-S3: K8s Operator Deployment

**User story:** "As a platform engineer, I want to deploy the Observable K8s operator via Helm
and have it automatically populate the infrastructure catalog."

**Acceptance criteria:**
- Helm chart includes a `k8s-operator` deployment with the required ClusterRole/ClusterRoleBinding
  to read pods, nodes, namespaces, and deployments
- Operator connects to the Observable control plane using a service-account token (not a
  user-facing API key)
- Operator emits its own OTLP metrics for API request rate, reconcile latency, and error count

**Files affected:** `charts/`, `services/k8s-operator/`

---

### Phase P11 — Advanced Signal Coverage

**User outcome:** Observable covers all signal types that observability leaders use as table
stakes: profiling, browser RUM, and synthetic checks.

**Entry gate:** P4-S1 warm retention / object storage is unblocked (profiling prerequisite);
privacy review for RUM is complete.

**Exit gate:** a full-stack engineer can trace a user session from a browser click through the
backend service into a database query and a CPU flame graph.

#### P11-S1: Continuous Profiling Ingestion (= P6-S1 promoted)

See Phase 6 active roadmap slice. Entry gate is P4-S1 object storage.

#### P11-S2: Flame Graph Viewer

**Acceptance criteria:**
- `/profiling` route with icicle flame graph (clickable nodes, zoom, search)
- Compare mode: select two time windows; differential highlight shows +/- CPU time per node
- Profile-to-trace link: a span with `profile_id` attribute opens the profile context

#### P11-S3: Browser RUM SDK and Ingestion (= P6-S2 promoted)

See Phase 6 active roadmap slice.

#### P11-S4: Web Vitals Dashboard

**Acceptance criteria:**
- `/rum` route showing LCP, CLS, INP per page path as percentile distributions
- Session list with error count, duration, device type, linked JS errors
- Deploy overlay: a vertical marker shows when a deploy happened, allowing before/after comparison

#### P11-S5: HTTP Synthetic Check (= P6-S4 promoted, first step)

**Acceptance criteria:**
- Create a synthetic check from the UI: URL, method, headers, body, assertion (status code, body
  contains, latency < N ms), schedule (1 min / 5 min / 15 min / 1 hour)
- Results stored as a metric series (`synthetic.check.duration`, `synthetic.check.success`)
- Alert rule can reference `synthetic.check.success = 0 for 2 consecutive checks`

#### P11-S6: Multi-Step Synthetic Check

**Acceptance criteria:**
- A check definition contains a sequence of HTTP steps
- Each step can reference response fields from the prior step (e.g., use the token from a login
  response in the next step's Authorization header)
- W3C `traceparent` is injected on each step; resulting traces are linked to the check result

---

### Phase P12 — Operational Completeness

**User outcome:** the on-call workflow is fully covered — from alert to escalation to post-mortem
— without leaving Observable or writing code.

**Entry gate:** Phase 5 (Reliability Product) stable; Phase P9 (developer experience) complete.

#### P12-S1: PagerDuty Notification Channel Adapter

**User story:** "As an SRE, I want critical alerts to create PagerDuty incidents automatically."

**Acceptance criteria:**
- Notification channel type `pagerduty`: `integration_key` field, severity mapping config
- On alert firing: POST to PagerDuty Events API v2 with `dedup_key = incident.dedup_key`
- On alert resolution: POST resolve event to PagerDuty
- Test connection button in the notification channel form

**Files affected:** `services/alert-evaluator/src/notifications.rs`, `apps/frontend/src/features/alerts/`

---

#### P12-S2: Opsgenie Notification Channel Adapter

Same pattern as P12-S1 but targeting the Opsgenie Alert API.

---

#### P12-S3: Deadman Alert Type

**User story:** "As an SRE, I want to be alerted when a service stops emitting telemetry."

**Acceptance criteria:**
- New alert rule type `deadman`: evaluates `last_seen` of a service's spans against a threshold
- If no span from the service has been received in `window_secs`, the rule enters Active state
- UI alert creation form adds a "No data" alert type option

**Files affected:** `services/alert-evaluator/src/evaluator.rs`, `apps/frontend/src/features/alerts/`

---

#### P12-S4: Change-Detection Alert Type

**User story:** "As an SRE, I want to be alerted when a metric changes by more than X% compared
to the same window N days ago (week-over-week or day-over-day)."

**Acceptance criteria:**
- New alert rule type `change_detection`: compares current window avg with a baseline window avg
- Configurable: `metric`, `window_secs`, `baseline_offset_secs`, `threshold_percent`
- Alert message includes the current value, baseline value, and percentage change

**Files affected:** `services/alert-evaluator/src/evaluator.rs`

---

#### P12-S5: Alert Inhibition Rules

**User story:** "As an SRE, I want to suppress warning-level alerts when a critical alert is
already firing for the same service, to prevent alert storms."

**Acceptance criteria:**
- New `alert_inhibition_rules` table: `source_rule_id`, `target_severity`, `match_labels` (JSONB)
- Evaluator skips notification dispatch for inhibited alerts; sets state to `Suppressed`
- Suppressed alerts are still visible in the alerts list with a "Suppressed by: <rule name>" label

**Files affected:** `services/alert-evaluator/src/evaluator.rs`, `migrations/`

---

#### P12-S6: Escalation Policy Builder

**User story:** "As a team lead, I want to define that if a critical alert is not acknowledged
within 15 minutes, it escalates to the secondary on-call channel."

**Acceptance criteria:**
- `escalation_policies` table: `name`, `steps` (JSONB array of `{delay_minutes, channel_id}`)
- An `AlertRule` can reference an `escalation_policy_id`
- Evaluator tracks acknowledgement time per incident; dispatches next escalation step if unacked
- Escalation policy builder UI in the Alerts settings section

**Files affected:** `services/alert-evaluator/`, `migrations/`, `apps/frontend/src/features/alerts/`

---

### Phase P13 — Data Portability and Migration Support

**User outcome:** teams migrating from Prometheus/Grafana can adopt Observable without rewriting
their entire alert and metrics stack from day one.

**Entry gate:** Phase P12 operational completeness complete; query-api NLQ stable.

#### P13-S1: Prometheus Remote Write Receiver

**User story:** "As a platform engineer, I want my Prometheus instances to push metrics to
Observable without reconfiguring every service."

**Acceptance criteria:**
- New endpoint: `POST /api/v1/write` (Prometheus remote write format, snappy-compressed protobuf)
- Converts Prometheus `TimeSeries` to Observable `MetricSeries` with label-to-attribute mapping
- Accepts `X-Tenant-ID` header for tenant routing (same as OTLP ingest)
- Documented in the API spec with example `prometheus.yml` remote_write config

**Files affected:** `services/ingest-gateway/`, `libs/domain/src/metric.rs`, `contracts/`

---

#### P13-S2: Prometheus Alert Rule Importer

**User story:** "As a migrating user, I want to upload my Prometheus alert rules YAML and have
Observable create equivalent alert rules, with a report on what mapped cleanly vs what needs
manual review."

**Acceptance criteria:**
- `POST /v1/alerts/import/prometheus` accepts a Prometheus alerting rules YAML file
- Translates PromQL alert expressions to Observable threshold/change-detection rules where possible
- Returns a mapping report: mapped rules, partially-mapped rules (manual review needed), and
  unmapped rules (unsupported PromQL features) with an explanation per item
- Dry-run mode (default): returns the report without creating rules; `?apply=true` creates them

**Files affected:** `services/query-api/`, new `libs/prometheus-compat/`

---

#### P13-S3: PromQL Compatibility Façade (= P8-S7 promoted with higher priority)

**User story:** "As a migrating user, I want to type PromQL expressions in the query workbench
and get results without learning a new query language."

See P8-S7 in the active roadmap. The Prometheus remote write receiver (P13-S1) raises the
priority of PromQL query support — teams using remote write for ingest will expect PromQL for queries.

---

#### P13-S4: Export APIs

**User story:** "As a data analyst, I want to export log query results as CSV for further
analysis in a spreadsheet."

**Acceptance criteria:**
- `GET /v1/logs/export?format=csv` (and `format=json`) for log search queries
- `GET /v1/traces/export?format=json` for trace search queries
- `GET /v1/metrics/export?format=csv` for metric series queries
- Export is limited to 100,000 rows; beyond that, an async export job is created
- "Export" button in log, trace, and metric explorer UIs

**Files affected:** `services/query-api/`, `apps/frontend/src/features/`

---

### Phase P14 — DORA Metrics and Engineering Intelligence

**User outcome:** engineering leadership can measure and improve delivery performance using
Observable's existing telemetry without a separate analytics tool.

**Entry gate:** Phase 5 (reliability product) and Phase P9 (developer experience) complete;
deployment markers, incident records, and error issues are all stable data sources.

#### P14-S1: DORA Metrics Report

**User story:** "As an engineering director, I want to see the four DORA metrics per team over
the last quarter."

**Acceptance criteria:**
- `GET /v1/reports/dora?from=...&to=...&team=...` computes from existing data:
  - **Deployment frequency:** count of deployment events in window per service
  - **Lead time for changes:** median time from deployment creation to incident clearance following that deploy
  - **Change failure rate:** % of deploys that triggered an incident within 1 hour
  - **MTTR:** median incident duration (open → resolved) for incidents correlated to a deploy
- DORA report page in the UI: four metric tiles with trend sparklines, filterable by team/service
- Export as CSV

**Files affected:** `services/query-api/`, `apps/frontend/src/features/`

---

#### P14-S2: Database Monitoring Layer

**User story:** "As a backend engineer, I want to see the slowest database queries for my
service and detect N+1 query patterns in traces."

**Acceptance criteria:**
- `GET /v1/services/{service}/database-queries` returns the top 20 query patterns by mean
  duration, P99, and call count over the selected time window, derived from span attributes
  (`db.statement`, `db.system`, `db.name`, `db.operation`)
- Query normalization strips literal values from query text to group by query shape
- N+1 detection: flag traces where the same normalized query appears > 10 times
- Service detail page gains a "Database" tab surfacing this data

**Files affected:** `services/query-api/`, `apps/frontend/src/features/services/`

---

#### P14-S3: Saved Views in Explorers

**User story:** "As an SRE, I want to save my log search configuration (filters + time range +
column layout) so I can return to it with one click."

**Acceptance criteria:**
- Saved views stored in `saved_views` PostgreSQL table: `name`, `signal_type`, `filter_state`
  (JSONB), `tenant_id`, `owner_id`, `visibility` (private/shared)
- "Save view" button in Log, Trace, and Metric explorers
- Saved views list accessible from the explorer sidebar or a global "Views" nav item
- Sharing: team members can see shared views; private views are owner-only

**Files affected:** `services/query-api/`, `apps/frontend/src/features/`

---

#### P14-S4: Change Event API and Dashboard Overlay

**User story:** "As a platform engineer, I want CI/CD pipelines to post change events (deploys,
config changes, feature flag flips) and see them as overlays on dashboards."

**Acceptance criteria:**
- `POST /v1/events/changes` accepts: `type` (deploy/config_change/feature_flag/db_migration),
  `service`, `description`, `metadata` (JSONB), `occurred_at`
- Dashboard time-series panels render a vertical dashed marker at change event timestamps when
  the "Show change events" toggle is on
- Change event explorer page: filterable list by type/service/time range

**Files affected:** `services/query-api/`, `migrations/`, `apps/frontend/src/features/dashboards/`

---

## 4. Quick-Win Backlog

These are features that can be shipped as standalone slices within existing phases without
waiting for a phase gate. Each is small enough (≤ 1 week of implementation) to be promoted
immediately.

| Item | User value | Phase target |
|---|---|---|
| Saved views in explorers | Reduces repetitive filter setup; high developer satisfaction | P9 or standalone |
| Export APIs (CSV/JSON) | Unblocks analyst use cases; zero architectural risk | P13 or standalone |
| PagerDuty adapter | Unblocks the most-requested alerting integration | P12-S1 (promote now) |
| Deadman alert type | "Is my service silent?" is asked daily | P12-S3 (promote now) |
| Prometheus remote write | Single biggest migration enabler | P13-S1 |
| Service health summary in catalog | Dramatically improves time-to-triage on the home screen | P9-S5 |
| Error issue count badge on service rows | Developers see their service's error state at a glance | P9-S3 |
| Change event API | CI/CD integration with 2 hours of backend work | P14-S4 |
| Onboarding wizard | Leading source of trial abandonment in all competitive analyses | P9-S1 |

---

## 5. Success Metrics

Progress toward parity should be measured on three axes:

### 5.1 Feature Coverage

Track the percentage of "Tier A" user workflows (section 2) that are fully completable inside
Observable without using a competing tool:

| Workflow | Current | P9 target | P12 target | P14 target |
|---|---|---|---|---|
| First signal in (onboarding) | 30% | 100% | 100% | 100% |
| Error triage | 25% | 90% | 95% | 100% |
| Service health at a glance | 60% | 95% | 100% | 100% |
| Root-cause (trace + log + DB) | 70% | 70% | 80% | 95% |
| Infrastructure inventory | 20% | 30% | 90% | 95% |
| Browser RUM / Web Vitals | 0% | 0% | 60% | 90% |
| Synthetic monitoring | 0% | 0% | 60% | 85% |
| Change event correlation | 50% | 70% | 85% | 100% |
| Alert routing + escalation | 40% | 50% | 90% | 95% |
| Profiling + flame graphs | 0% | 0% | 60% | 85% |
| DORA metrics | 0% | 0% | 20% | 100% |
| Prometheus migration support | 10% | 10% | 10% | 90% |
| Cost/cardinality management | 40% | 50% | 60% | 80% |

### 5.2 Time-to-Value Benchmarks

These should be measurable by timing a first-time user session:

- **Time to first trace visible in UI:** target < 10 minutes after installing the OTel SDK
- **Time to first alert firing:** target < 15 minutes after first service is sending data
- **Time to identify the root cause of a synthetically injected error:** target < 5 minutes
  using trace + log correlation

### 5.3 Competitive Positioning by Buyer Segment

| Buyer segment | Current Observable position | Target position after P9–P14 |
|---|---|---|
| Developer tooling teams (Sentry alternative) | Weak (no error tracking) | Strong (error issues + RUM) |
| SRE / platform teams (Datadog alternative) | Moderate (alerts, SLOs, topology) | Strong (full alert lifecycle, k8s, profiling) |
| Migration from Prometheus/Grafana | Weak (no remote write, no PromQL) | Strong (remote write, PromQL, rule importer) |
| Enterprise (regulated industries) | Moderate (auth, RBAC, self-hosted) | Strong (SCIM, compliance, BYOK in P7) |
| Engineering leadership (DORA, cost visibility) | Weak (no DORA, basic cost report) | Strong (DORA metrics, ingest breakdown, sampling UI) |

---

## 6. Sequencing and Dependencies

```
P9 (Dev Experience)
  └── P9-S1: Onboarding wizard          ← no external dependency; promote immediately
  └── P9-S2/S3/S4: Error tracking       ← requires stream-processor, migrations
  └── P9-S5: Service health summary     ← requires query-api span aggregation

P10 (Infrastructure)
  └── P10-S1/S2/S3: K8s catalog + UI    ← new k8s-operator service; requires Helm chart

P11 (Advanced Signals)
  └── P11-S1: Profiling                 ← requires P4-S1 object storage (currently deferred)
  └── P11-S3: Browser RUM               ← requires privacy review
  └── P11-S5/S6: Synthetic checks       ← standalone; promote after P12 (alert integration)

P12 (Operational Completeness)
  └── P12-S1/S2: PagerDuty/Opsgenie    ← small adapters; promote immediately as quick wins
  └── P12-S3: Deadman alerts            ← promote immediately
  └── P12-S4: Change detection          ← promote immediately
  └── P12-S5: Alert inhibition          ← low-risk evaluator addition
  └── P12-S6: Escalation policies       ← requires P12-S1/S2

P13 (Migration Support)
  └── P13-S1: Prometheus remote write   ← ingest-gateway addition; no breaking changes
  └── P13-S2: Alert rule importer       ← depends on P13-S1
  └── P13-S3: PromQL façade             ← depends on P8-S6 NLQ (complete)
  └── P13-S4: Export APIs               ← standalone; promote immediately

P14 (Intelligence / DORA)
  └── P14-S1: DORA metrics              ← requires Phase 5 + P9 error tracking data
  └── P14-S2: Database monitoring       ← query-api span aggregation; no new infra
  └── P14-S3: Saved views               ← small; promote immediately
  └── P14-S4: Change event API          ← small; promote immediately
```

---

## 7. ADR and Spec Sync Requirements

The following specs and ADRs need updates or new entries when implementing this plan:

| New feature area | Required spec / ADR change |
|---|---|
| Error tracking | New section in `spec/14-domain-model.md` for ErrorIssue entity; new section in `spec/05-frontend.md` for Errors explorer |
| Infrastructure catalog | New section in `spec/14-domain-model.md` for InfrastructureResource entity; new section in `spec/06-agents.md` for k8s operator catalog population |
| DORA metrics | New section in `spec/07-alerting-slo.md` for DORA definitions and data sources |
| Database monitoring | New section in `spec/05-frontend.md` for the Database service tab; new section in `spec/08-ai-ml.md` for N+1 detection heuristic |
| Change event API | New ADR or extension of `spec/18-deployment-markers.md` to cover non-deploy change event types |
| Prometheus remote write | `ADR-017` is already accepted — create the implementation spec section in `spec/09-api.md` |
| Sampling rules UI | New section in `spec/05-frontend.md` §9.4; new section in `spec/09-api.md` for the rules management API |
| Export APIs | Add to `spec/05-frontend.md` §9.11 (already referenced as a Tier 2 gap) |
| Saved views | Add to `spec/05-frontend.md` §9.11 |
| Escalation policies | Extend `spec/07-alerting-slo.md` §11.4 |
| Deadman + change detection alerts | Already in `spec/07-alerting-slo.md` §11.1; implementation spec needed in `spec/09-api.md` |

---

## 8. What This Plan Does Not Cover

The following capabilities are intentionally left out of this parity plan. Each has a documented
rationale.

| Capability | Rationale for exclusion |
|---|---|
| Multi-region active-active | Deferred in `spec/13-risks-roadmap.md §24.2` until single-region, multi-AZ operations are proven. No change to that decision. |
| Security monitoring (SIEM) | Out of product scope for now; requires separate trust model, compliance certification, and data classification layer that would dominate a phase. |
| AI-driven autonomous remediation | Explicitly excluded by `ADR-014`; advisory-only AI policy is a core Observable differentiator. |
| CI/CD test visibility | Valuable but out of scope until error tracking (P9) is stable; test observability builds on the same error fingerprinting infrastructure. |
| Full mobile SDK (P6-S3) | Left in the Phase 6 roadmap without acceleration; browser RUM (P11-S3) must be stable first. |
| Session replay (P6-S6) | Privacy review is a hard prerequisite; not accelerated here. |

---

*End of document. Next action: promote P9-S1 (Onboarding Wizard) and P12-S1 (PagerDuty adapter)
as the first two slices from this plan by writing detailed implementation plan documents per the
Promotion Rules in `2026-05-07-remaining-roadmap-plan.md §15`.*
