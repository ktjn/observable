# Frontend Architecture

## 9. Frontend Architecture

### 9.1 Stack

| Layer | Choice | Rationale |
|---|---|---|
| Framework | React 19.x + TypeScript | Component model scales to complex correlated UIs; strong ecosystem |
| Build | Vite 8 | Sub-second HMR; production-optimized output |
| Server state | TanStack Query | Request dedup, caching, background refresh—essential for polling dashboards |
| Routing | TanStack Router (typed routes) | URL = full application state; enables deep links and compare mode |
| Visualization | `@grafana/ui` + `@grafana/scenes` (Apache-2.0) | Production-grade observability charts; no AGPL obligation (see ADR-016) |
| Query editor | Monaco Editor | Syntax highlighting, autocomplete, schema hints for query workbench |
| Design system | Base UI + Tailwind CSS v4 | Accessibility baseline, "render prop" primitives, high-performance styling |

### 9.2 Directory Architecture (Feature-Based)

To ensure scalability and clean component separation, we follow a feature-based organization. Logic is grouped by domain rather than by technical role.

```text
apps/frontend/src/
├── assets/          # Global static assets (images, fonts)
├── components/      # Shared components
│   ├── ui/          # "Owned" primitive components (Button, Input, Popover) - Shadcn pattern
│   └── shared/      # Shared, domain-agnostic UI components (Layout, Sidebar)
├── features/        # Domain-specific modules
│   ├── tracing/     # Everything related to Trace Explorer
│   │   ├── api/     # Hooks for fetching trace data
│   │   ├── components/ # Components specific to tracing (Waterfall, SpanDetail)
│   │   ├── types/   # TypeScript interfaces for traces
│   │   └── utils/   # Helper functions (duration formatting, etc)
│   ├── logs/        # Everything related to Log Explorer
│   └── metrics/     # Everything related to Dashboards
├── hooks/           # Shared, domain-agnostic custom hooks (useAuth, useLocalStorage)
├── lib/             # Configuration for external libraries (QueryClient, OTel SDK)
├── routes/          # TanStack Router route definitions
├── stores/          # Global state management (Zustand)
├── styles/          # Global styles and Tailwind configuration
├── types/           # Global TypeScript interfaces
└── utils/           # Shared utility functions
```

### 9.3 Design System & Styling

Consistency in an observability UI is critical for reducing cognitive load. **All UI development must prioritize reusable components and minimal duplication.**

- **Component Primitives**: We use **Base UI** for accessible, unstyled primitives (Dialogs, Dropdowns, Tabs). Base UI's render-prop pattern ensures better predictability than traditional `asChild` models.
- **Styling Method**: **Tailwind CSS v4** is used for all styling. Its Rust-based compiler provides the performance required for extremely dense data visualizations without the management overhead of CSS Modules.
- **Design Tokens**: Defined within Tailwind's theme and custom CSS variables in `src/styles/globals.css`.
- **Ownership Model**: We follow the **Shadcn pattern**—component code for primitives (Button, Popover, etc.) is copied into `src/components/ui/` and styled locally. This allows for deep performance optimizations without library lock-in.
- **Visuals**: **Grafana Scenes** are used specifically for data-heavy dashboarding and complex chart interactions.
- **Reusability Mandate**: 
  - Developers MUST check for existing components in `src/components/` and `src/features/**/components/` before creating new ones.
  - Shared UI logic and data patterns MUST be extracted into hooks (in `src/hooks/` or feature-local `hooks/`) or utilities.
  - Duplication of JSX or business logic is considered a blocking review item.

### 9.4 Testing Strategy

We follow the "Testing Trophy" approach, prioritizing integration tests that simulate user behavior.

- **Unit Tests (Vitest)**: For pure utility functions and isolated business logic.
- **Component Tests (Vitest + RTL)**: For shared UI components and domain-specific logic. These use **MSW** (Mock Service Worker) to simulate API responses.
- **E2E Tests (Playwright)**: For critical user journeys (e.g., "Ingest trace -> Search for trace -> View waterfall"). Covers browser compatibility and full-stack integration.
- **Visual Regression**: Use Playwright snapshots for sensitive UI components (e.g., charts) to prevent styling regressions.

### 9.5 Navigation Model: Entity-Centric, Not Signal-Centric

Navigation is organized around **business entities** (services, deployments, environments) rather than signal types (metrics tab, logs tab). This reflects how operators investigate incidents—they start with a service, not with a metric.

The UI must make it easy to move between related operational surfaces without losing project,
environment, tenant, time range, or filter context. The primary information architecture is:

- **Services**: service catalog and service-scoped investigation workspace.
- **Infrastructure**: hosts, Kubernetes clusters, namespaces, pods, containers, and related infrastructure signals.
- **Service Overview**: topology map of services and their observed relationships.
- **Dashboards**: saved and ad-hoc dashboards, backed by dashboard-as-code artifacts.
- **Alerts & SLOs**: alert rules, active incidents, SLO status, and burn-rate investigation.
- **Admin / Fleet / Billing**: platform administration, agent fleet, tenant settings, and billing.

**Primary navigation hierarchy:**

```
Platform
└── Project / Environment
    └── Services
        └── Service Catalog  ←── entry point for most workflows
            └── Service Detail
                ├── Overview            (quick performance, health, SLOs, deployment context)
                ├── Logs                (service-filtered log explorer)
                ├── Metrics             (service-filtered common metrics + custom series)
                ├── Traces              (service-filtered trace explorer)
                ├── Deployments         (timeline, diff, trace correlation)
                └── Alerts / Incidents  (scoped to this service)
    └── Infrastructure
        ├── Hosts
        ├── Kubernetes Clusters
        ├── Namespaces
        ├── Pods / Containers
        └── Infrastructure Dashboards
    └── Service Overview
        └── Service Map                 (topology derived from traces)
    └── Cross-Service Views
        ├── Trace Explorer              (cross-service search)
        ├── Log Explorer                (cross-service search)
        └── Metric Explorer             (cross-service, cardinality browser)
    └── Dashboards                      (as-code + UI builder)
    └── Alerts & SLOs                   (global management)
    └── Admin / Fleet / Billing
```

**Service detail route contract:**

```
Platform
└── Project / Environment
    └── Service Catalog
        └── Service Detail
            ├── Health Overview     (RED metrics, SLIs, error budget, SLO status)
            ├── Traces              (filtered to this service)
            ├── Logs                (filtered to this service)
            ├── Metrics             (filtered to this service)
            ├── Deployments         (timeline, diff, trace correlation)
            └── Alerts / Incidents  (scoped to this service)
```

**Design rationale:**
- Mirrors New Relic's entity synthesis model and Datadog's service catalog
- Reduces context switching: a single incident can be triaged entirely within the service detail view
- Avoids signal-centric anti-pattern where finding a slow service requires visiting three separate tab groups
- Keeps service topology separate from service catalog so operators can switch between inventory and relationship views quickly

### 9.2.1 Required Product Views

#### Services

The Services area is the default operational workspace.

**Service Catalog requirements:**
- List every discovered service entity in the selected project/environment.
- Show quick performance and health columns: request rate, error rate, P95 latency, current SLO state, active alert count, and last deployment.
- Support search, owner/team filter, environment filter, health filter, and sort by health or performance signal.
- Preserve selected project, environment, time range, and filters when navigating into a service and back.

**Service Detail requirements:**
- Render an **Overview** tab first, with a compact health and performance dashboard for the selected service.
- Provide direct tabs for **Logs**, **Metrics**, and **Traces**. Each tab opens with the selected service and current time range already applied.
- Provide a full aggregated service log view that combines all workloads for the service, ordered by timestamp, with severity, workload, host/pod, trace, and span correlation columns.
- Provide common service metrics out of the box: request rate, error rate, latency percentiles, saturation/resource usage, and availability/SLO status.
- Provide trace search scoped to the service, with upstream/downstream filters and a path from each trace back to correlated logs and metrics.
- Keep all service tabs deep-linkable and browser-back friendly.

#### Infrastructure

The Infrastructure area provides infrastructure-first views for teams investigating host,
cluster, pod, container, or namespace health.

**Infrastructure view requirements:**
- Provide inventory lists for hosts, Kubernetes clusters, namespaces, pods, and containers when the attributes exist in telemetry or catalog data.
- Show quick health and utilization summaries: CPU, memory, disk, network, restart count, and recent error/log rate where available.
- Link every infrastructure entity to related services, logs, metrics, and traces using OTel resource attributes.
- Support infrastructure dashboards scoped by host, cluster, namespace, pod, and container.
- Preserve context when moving from service detail into infrastructure and back.

#### Service Overview

The Service Overview area is the topology view of the system.

**Service Overview requirements:**
- Render a map of services and their observed relationships.
- Derive service nodes and edges from trace data; do not require manually maintained topology.
- Show relationship health using edge-level request rate, error rate, and latency.
- Let operators click a service node to open the service detail overview.
- Let operators click an edge to open traces/logs filtered to that caller-callee relationship.
- Support an overview mode for the full graph and a focused mode for one service plus direct upstream/downstream dependencies.

### 9.3 Frontend Modules

#### Phase 1 (Internal MVP)
| Module | Purpose |
|---|---|
| Onboarding / Setup | Agent install wizard, API key generation, first signal validation |
| Service Catalog | List all services with health ring, error rate, P95 latency; entry point |
| Service Detail Overview | Compact service dashboard with quick performance, logs, metrics, traces, SLO, deployment marker (see `spec/18-deployment-markers.md`), and alert context |
| Trace Explorer | Full-text + attribute search, waterfall, span detail, field faceting |
| Log Explorer | Structured search, histogram, log detail, live tail, context (surrounding logs) |
| Metric Explorer | Series browser, cardinality inspector, ad-hoc time-series graph with NLQ auto-graphing |
| Basic Dashboards | Fixed-layout panels, time range picker, promote-to-dashboard from explorers |
| Threshold Alerts | Create / edit / silence rules; active incidents list |

These modules define the minimum operator-facing UI bar for the internal MVP. If implementation is
staged, the explorer shell may land first, but Onboarding / Setup, Service Catalog, Service Detail
Overview, service-scoped Logs / Metrics / Traces, one dashboard workflow, and one threshold-alert
workflow must be completed before broader Phase 2–3 UI expansions such as Service Map,
Deployment Timeline, Dashboard Builder, Alert Routing, or Incident Timeline are treated as the
next frontend priority.

#### Phase 2–3
| Module | Purpose |
|---|---|
| Service Map | Interactive topology graph derived from trace data |
| Infrastructure Views | Host, cluster, namespace, pod, and container inventory with linked logs, metrics, traces, and related services |
| Trace Comparison | Compare two traces (e.g. fast vs slow) to identify bottlenecks or path diffs |
| Deployment Timeline | Overlay deployments on metrics/traces; diff environment configs (see `spec/18-deployment-markers.md`) |
| Query Workbench | Monaco-based multi-signal notebook, shareable query URLs |
| Dashboard Builder | Drag-and-drop panel editor generating dashboard-as-code |
| SLO Management | Create SLOs, view burn rate, error budget history |
| Alert Routing | Escalation policies, notification channel management |
| Incident Timeline | Correlated events, responder log, post-mortem export |

#### Phase 4+
| Module | Purpose |
|---|---|
| Profiling Explorer | Flame graph viewer, differential profiling, service filter |
| Fleet / Agent Management | Agent health, remote config, upgrade campaigns |
| Admin Console | Tenant config, RBAC, data scopes, quota management |
| AI Insights Panel | NLQ entry point: natural language query box, auto-graphing via VisualizationFrame, smart grouping, anomaly surface, query suggestions (advisory only, provenance required) |

### 9.3.1 Query And Filter UX

Natural language query is the primary filter input across service, topology, infrastructure, log,
trace, and metric surfaces. Selector-style filter controls should not be introduced for new
narrowing behavior. The shared query input accepts:

- natural language, translated through `POST /v1/nlq` with `mode: "interpret"` for page filtering;
- raw `NlqIr` JSON as the deterministic fallback when no LLM is configured.

The global date/time range remains a separate global control. Sorting, navigation, and actions
remain explicit controls. Service detail NLQ execution routes returned `VisualizationFrame` data to
the matching Logs, Metrics, or Traces tab below the input.

### 9.3.2 Log Explorer UX

The Log Explorer's **quick filter** (the "Search messages" input next to severity filter pills) supports a regex mode (toggled via the `.*` button next to the input) for pattern matching against log messages, in addition to the default plain-substring mode. This is a client-side filter over already-loaded rows, distinct from the NLQ/shorthand query box above it.

### 9.3.3 Trace Detail Waterfall UX

Each waterfall row shows an explicit "ERROR" badge and a red left-border accent when the span's `status_code` is `ERROR`, independent of the row's service-color-coded bar fill — so error status is legible without relying on color alone.

### 9.4 Cross-Signal Correlation Patterns

Correlation is a first-class UI concern. The following patterns must be implemented consistently across all signal explorers.

#### Trace Context Propagation
Every log line, metric exemplar, and deployment event that carries a `trace_id` or `span_id` must render a **"View Trace"** link. Clicking opens the waterfall in a side panel (no full navigation loss).

#### Breadcrumb Trail
Navigation between signals preserves context in the URL and renders a breadcrumb:

```
Service: checkout-api  >  Trace abc123  >  Log line (span: xyz456)
```

Breadcrumbs are deep-linkable and survive page reload.

#### Related Insights Panels
Service detail and trace detail views render a **"Related"** sidebar showing:
- Metrics anomalies within the trace's time window
- Deployment events within ±5 minutes (see `spec/18-deployment-markers.md`)
- Correlated SLO burn spikes
- Linked incidents

This is data-driven (JOIN via `service_name + time window`), not manually curated.

#### Side-by-Side Correlated Panes
The trace waterfall view supports a split view: trace on the left, correlated logs on the right, synchronized time cursor. Clicking a span highlights correlated log lines.

#### Infrastructure Correlation
Every service detail, trace, and log view must provide links to the underlying infrastructure (host, pod, container) metrics and logs. This is achieved by joining on OTel resource attributes (`host.name`, `k8s.pod.name`, etc.).

Infrastructure detail views must provide the inverse relationship: from a host, pod, container,
namespace, or cluster, the operator can open related services, logs, metrics, and traces without
manually reconstructing filters.

#### Log Context (Surrounding Logs)
When viewing a specific log line in the explorer or detail view, the operator can click "View Context". This opens a view showing logs from the same service and host that occurred immediately before and after the selected log line (±1 minute by default), ignoring other active search filters.

#### Promote to Dashboard
Every query in the Trace, Log, or Metric Explorer must have a "Promote to Dashboard" action. This allows the operator to quickly turn a successful ad-hoc discovery into a permanent monitoring panel.

### 9.5 High-Cardinality Exploration

The platform stores millions of distinct label combinations. The UI must expose these without performance degradation or overwhelming the operator.

#### Cardinality Browser
The Metric Explorer includes a **Cardinality Inspector** that shows:
- Top N label keys by series count
- Series growth trend (detecting cardinality explosions)
- Estimated storage impact per label key

#### BubbleUp-Style Comparison
Inspired by Honeycomb's BubbleUp: when viewing a time series anomaly or a set of error logs, the operator can select the anomalous window. The UI automatically compares attribute value distributions in that window against the baseline period and surfaces the dimensions (service version, region, customer tier, host) that are statistically over-represented in errors.

This is implemented as a query against the query facade—no ML required—using GROUP BY + percentage comparison.

#### Faceting and Field Statistics
The Log and Trace explorers include a sidebar showing the distribution of common fields (facets) for the current result set (e.g., status codes, log levels, service names). This allows for rapid narrowing of search results without typing complex queries.

#### Free-Form Field Exploration
Log and trace explorers support arbitrary attribute filtering without pre-indexing. The autocomplete suggests attribute keys from the current result set (streamed from the query facade).

### 9.6 Service Map

The service map is derived from distributed trace data, not manually configured.

**Rendering requirements:**
- Nodes represent services (`service_name` dimension)
- Edges represent observed upstream→downstream call relationships
- Edge weight encodes request volume (last 1 hour by default)
- Node color encodes error rate: green / amber / red health states
- Node label shows P95 latency and RPS at zoom ≥ 50%
- Clicking a node navigates to that service's detail view
- Clicking an edge shows the filtered trace list for that service pair

**Interaction modes:**
- **Overview**: full dependency graph, auto-layout (dagre/ELK)
- **Focused**: one selected service + its direct upstream/downstream dependencies
- **Diff mode**: compare topology at two time ranges to detect new dependencies or topology regressions

**Implementation note:** Render with a canvas-based force-directed library (e.g., `@antv/g6` or `cytoscape.js`); SVG-based rendering degrades beyond ~100 nodes.

### 9.7 Dashboard Architecture

Dashboards follow a **hybrid model**: operators can build dashboards through the UI builder, but every dashboard is stored and distributed as a serialized artifact (JSON/YAML). This enables version control and CI/CD review.

#### Dashboard State Model (Grafana Scenes-inspired)

Each dashboard is represented as a tree of scene objects:

```
Dashboard
├── TimeRange           (global time range, compare mode toggle)
├── Variables           (template variables: service, environment, region)
├── Layout              (grid | rows | tabs)
│   ├── Panel           (query + visualization binding)
│   │   ├── DataQuery   (signal type, query string, time override)
│   │   └── Visualization (timeseries | bar | heatmap | stat | table | flamegraph)
│   └── ...
└── Annotations         (deployment events, incidents overlaid on all panels)
```

All state (time range, variable values, panel queries) is reflected in the URL for deep linking and sharing.

#### Dashboard-as-Code

Dashboards are stored as JSON documents in the platform's config store. They can be:
- Checked into a git repository alongside application code
- Validated and deployed via CI/CD (dashboard linting, broken query detection)
- Versioned with diff support in the UI
- Templated with variables for multi-service reuse

The platform exposes a **Dashboard API** (see spec/09-api.md) for programmatic create/update/diff.

#### UI Builder (Drag-and-Drop)

The UI builder generates the same JSON format. It provides:
- Panel library (all visualization types)
- Query editor with autocomplete
- Variable management UI
- Preview before save

Dashboards created in the UI builder are immediately available as code artifacts. No separate "export" step.

### 9.8 Time Range System

The time range system is a unified, first-class component shared across all views.

**Requirements:**
- Global time range picker in the top navigation bar; all panels default to it
- Per-panel time range override (absolute offset or relative shift)
- **Compare mode**: overlay the same query from a previous period (prior hour, prior day, prior week) as a faded series
- Preset quick ranges: 15m, 30m, 1h, 3h, 6h, 12h, 24h, 2d, 7d, 30d
- Custom absolute range input with explicit timezone selection
- Time range is always reflected in the URL (ISO 8601 or relative notation)
- **Time travel**: navigate to a past snapshot of a dashboard's data without changing saved configuration

**Timezone handling:**
- All data stored in UTC
- Display timezone configurable per user (stored in user preferences)
- Explicit UTC indicator shown whenever the display timezone differs from browser local time

### 9.9 Information Density Model

Apply the **inverted pyramid**: show the minimum information needed to assess health, expand on demand.

| Level | View | Content |
|---|---|---|
| L0 | Service Catalog list | Health ring, error rate %, P95 latency, active incident badge |
| L1 | Service health overview | RED golden signals, SLO burn rate, last deployment, alert count |
| L2 | Signal explorer (traces/logs/metrics) | Time series + filterable results table |
| L3 | Item detail (span/log/metric series) | Full attribute set, raw payload, correlated signals |

**Color semantics** (used consistently across all views):
- Green: within SLO / no active alerts
- Amber: approaching threshold / minor anomaly
- Red: SLO breach / active incident
- Blue: informational / no health signal

**Avoid:** Showing all signal types simultaneously on a single page. The cognitive load of a "wall of charts" is a known anti-pattern in observability tools. Use progressive disclosure.

### 9.10 Performance Requirements

| Interaction | Target | Measurement |
|---|---|---|
| Service catalog load | < 2s | P95 from page navigation |
| Hot-data query result | < 1s | P50 from query submit |
| Dashboard initial load | < 3s | P95 including all panel renders |
| Trace waterfall render | < 500ms | P95 for traces with ≤ 1000 spans |
| Log search results | < 2s | P95 for last-24h hot window |
| Service map initial render | < 3s | P95 for ≤ 500 service nodes |
| Infrastructure inventory load | < 2s | P95 from page navigation for ≤ 5000 entities |

**Implementation strategies:**
- **Virtualization**: Use windowed rendering for log lines and trace span tables (> 200 rows)
- **Progressive loading**: Render dashboard panels independently; skeleton states while queries run
- **Request deduplication**: TanStack Query deduplicates identical in-flight requests (critical for dashboards with shared variables)
- **Canvas rendering**: Service map and flame graphs rendered on canvas, not SVG
- **Query result streaming**: Stream log search results using chunked HTTP responses; render rows as they arrive

### 9.11 UX Requirements

- **Deep links everywhere**: Every view, filter, time range, selected trace, and open panel must produce a shareable URL
- **Saved views**: Named bookmarks for search configurations (filter set + time range + column selection), scoped per user or shared within project. **Shipped for the logs explorer** (`LogSearch.tsx`) — see `docs/superpowers/plans/2026-07-03-saved-views-logs.md` and `docs/superpowers/specs/2026-07-03-saved-views-logs-design.md`. Traces and metrics explorers are follow-up slices.
- **Keyboard-driven query UX**: Tab-complete in search bars, keyboard shortcuts for common actions (time range, toggle panels, expand detail)
- **Export APIs**: All data visible in the UI must be exportable (CSV, JSON, OTLP). Export respects current filter and time range
- **Themes**: Design tokens must support light, dark, and system themes. The system theme follows the browser/OS `prefers-color-scheme` value, and the resolved theme updates when the system preference changes.
- **Theme persistence**: Explicit light/dark/system preference persists in the user profile; anonymous or pre-login screens may use local storage until profile sync is available.
- **Accessibility baseline**: WCAG 2.1 AA: keyboard navigation, focus management, ARIA roles on custom components, sufficient color contrast

### 9.12 Frontend Anti-Patterns

**Never:**
- Couple UI components directly to storage (ClickHouse SQL in the browser). All data access via the query facade API
- Diverge query syntax per page (log explorer and trace explorer must use the same filter language)
- Duplicate time range state machines (one global time range context, per-panel overrides only)
- Lock in a chart library without an adapter layer (all visualization goes through `@grafana/ui` panel wrappers)
- Build service map topology from a manually maintained CMDB — derive from traces only
- Allow dashboards to exist only in the UI store — all dashboards must be serializable as code artifacts

### 9.13 State Management Boundaries

| State type | Owner | Rationale |
|---|---|---|
| Server data (queries, results) | TanStack Query | Caching, dedup, background refresh |
| URL state (time range, filters, selected entity) | TanStack Router | Deep links, compare mode, browser back |
| UI-local state (panel expand/collapse, tooltip hover) | React useState | No sharing required |
| User preferences (theme, timezone, default project) | Server-persisted profile | Consistent across devices; theme values are `light`, `dark`, or `system` |
| Dashboard configuration | Platform config API | Version-controlled, CI/CD deployable |

### 9.15 Error Handling & Resilience

A production-grade UI must remain functional even when parts of the backend are failing or the user's connection is unstable.

#### Error Boundaries
- **Global Error Boundary**: Catch-all for unexpected crashes, rendering a "Something went wrong" page with a reload action and a link to report the issue.
- **Feature-Level Boundaries**: Use `react-error-boundary` to wrap major modules (e.g., Service Catalog, Trace Explorer). If one feature crashes, the rest of the app remains usable.
- **Component-Level Boundaries**: Wrap risky components like Monaco Editor or complex Canvas charts.

#### Resilience Patterns
- **TanStack Query Retries**: Default to 3 retries with exponential backoff for network errors.
- **Stale-While-Revalidate**: Show cached data while fetching updates to reduce perceived latency.
- **Offline Mode**: Detect offline status and show a banner. Prevent destructive actions (Create/Edit) while offline, but allow read-only exploration of cached data.
- **Partial Content**: If a dashboard panel fails, show an error state for that panel only, not the entire dashboard.

### 9.16 Client-Side Observability

We "eat our own dogfood" by instrumenting the frontend with OpenTelemetry.

- **Tracing**: Capture user interactions (navigation, searches, dashboard loads) as traces. Link these to backend traces via `traceparent` headers to provide end-to-end visibility.
- **Exceptions**: Automatically report unhandled exceptions and React error boundary catches to the platform's own log/trace ingest.
- **Performance Vitals**: Track Core Web Vitals (LCP, FID, CLS) and custom metrics like "Time to First Chart" using the OTel Web SDK.
- **User Context**: Attach project, tenant, and anonymized user IDs to all client-side telemetry.

### 9.17 Production Readiness Checklist

Before any UI module is considered "Production Grade," it must satisfy:

- [ ] **Accessibility**: Passes `playwright-axe` automated scans with 0 critical violations.
- [ ] **Performance**: Initial load < 2s; query response < 1s; no layout shifts during data load.
- [ ] **Observability**: Error boundaries implemented; OTel instrumentation active; meaningful logs emitted.
- [ ] **Resilience**: Retries configured; loading/error/empty states for every query.
- [ ] **Security**: No secrets in bundle; no `dangerouslySetInnerHTML` without sanitization; valid CSP.
- [ ] **UX**: Deep links work; browser back/forward supported; responsive across common screen sizes.
- [ ] **Documentation**: Module-specific README in `features/` explaining state and API usage.

#### 9.18 Live Tail and Streaming

Real-time visibility is critical for verifying deployments and debugging active incidents.

**Requirements:**
- **Live Tail Mode**: In the Log Explorer, a "Live" toggle initiates a tailing session.
- **Auto-scroll**: New log lines are appended to the bottom and the view scrolls automatically (can be paused).
- **Sampling**: If ingest volume exceeds a threshold, the UI samples the live stream to maintain performance, with a clear indicator.
- **Consistency**: Filters applied in Live Tail mode must use the same syntax as historical search.
- **Transport**: The initial live tail client may poll `GET /v1/logs/tail` with a timestamp cursor
  at a fixed interval so tenant headers remain explicit; future streaming transports must preserve
  the same filter and cursor semantics.
- **Latency**: End-to-end latency from ingest to UI display should be < 2s for live tail.
