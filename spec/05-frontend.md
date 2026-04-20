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
| Design system | Custom tokens + Radix UI primitives | Accessibility baseline, dark mode, consistent component contracts |

See ADR-006 for the React/Vite decision rationale and ADR-016 for the visualization library choice.

For local development setup, Vite dev server configuration, environment variables, mock strategy,
production build output, hosting model, and Playwright E2E setup see `spec/15-frontend-local-dev.md`.

### 9.2 Navigation Model: Entity-Centric, Not Signal-Centric

Navigation is organized around **business entities** (services, deployments, environments) rather than signal types (metrics tab, logs tab). This reflects how operators investigate incidents—they start with a service, not with a metric.

**Primary navigation hierarchy:**

```
Platform
└── Project / Environment
    └── Service Catalog  ←── entry point for most workflows
        └── Service Detail
            ├── Health Overview     (RED metrics, SLIs, error budget, SLO status)
            ├── Traces              (filtered to this service)
            ├── Logs                (filtered to this service)
            ├── Metrics             (filtered to this service)
            ├── Deployments         (timeline, diff, trace correlation)
            └── Alerts / Incidents  (scoped to this service)
    └── Cross-Service Views
        ├── Service Map             (topology derived from traces)
        ├── Trace Explorer          (cross-service search)
        ├── Log Explorer            (cross-service search)
        └── Metric Explorer         (cross-service, cardinality browser)
    └── Dashboards                  (as-code + UI builder)
    └── Alerts & SLOs               (global management)
    └── Admin / Fleet / Billing
```

**Design rationale:**
- Mirrors New Relic's entity synthesis model and Datadog's service catalog
- Reduces context switching: a single incident can be triaged entirely within the service detail view
- Avoids signal-centric anti-pattern where finding a slow service requires visiting three separate tab groups

### 9.3 Frontend Modules

#### Phase 1 (Internal MVP)
| Module | Purpose |
|---|---|
| Onboarding / Setup | Agent install wizard, API key generation, first signal validation |
| Service Catalog | List all services with health ring, error rate, P95 latency; entry point |
| Trace Explorer | Full-text + attribute search, waterfall, span detail, field faceting |
| Log Explorer | Structured search, histogram, log detail, live tail, context (surrounding logs) |
| Metric Explorer | Series browser, cardinality inspector, ad-hoc PromQL-style graph |
| Basic Dashboards | Fixed-layout panels, time range picker, promote-to-dashboard from explorers |
| Threshold Alerts | Create / edit / silence rules; active incidents list |

#### Phase 2–3
| Module | Purpose |
|---|---|
| Service Map | Interactive topology graph derived from trace data |
| Trace Comparison | Compare two traces (e.g. fast vs slow) to identify bottlenecks or path diffs |
| Deployment Timeline | Overlay deployments on metrics/traces; diff environment configs |
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
| AI Insights Panel | Smart grouping, anomaly surface, query suggestions (advisory only) |

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
- Deployment events within ±5 minutes
- Correlated SLO burn spikes
- Linked incidents

This is data-driven (JOIN via `service_name + time window`), not manually curated.

#### Side-by-Side Correlated Panes
The trace waterfall view supports a split view: trace on the left, correlated logs on the right, synchronized time cursor. Clicking a span highlights correlated log lines.

#### Infrastructure Correlation
Every service detail, trace, and log view must provide links to the underlying infrastructure (host, pod, container) metrics and logs. This is achieved by joining on OTel resource attributes (`host.name`, `k8s.pod.name`, etc.).

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

**Implementation strategies:**
- **Virtualization**: Use windowed rendering for log lines and trace span tables (> 200 rows)
- **Progressive loading**: Render dashboard panels independently; skeleton states while queries run
- **Request deduplication**: TanStack Query deduplicates identical in-flight requests (critical for dashboards with shared variables)
- **Canvas rendering**: Service map and flame graphs rendered on canvas, not SVG
- **Query result streaming**: Stream log search results using chunked HTTP responses; render rows as they arrive

### 9.11 UX Requirements

- **Deep links everywhere**: Every view, filter, time range, selected trace, and open panel must produce a shareable URL
- **Saved views**: Named bookmarks for search configurations (filter set + time range + column selection), scoped per user or shared within project
- **Keyboard-driven query UX**: Tab-complete in search bars, keyboard shortcuts for common actions (time range, toggle panels, expand detail)
- **Export APIs**: All data visible in the UI must be exportable (CSV, JSON, OTLP). Export respects current filter and time range
- **Dark mode**: Design tokens support dark and light themes; user preference persists in profile
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
| User preferences (theme, timezone, default project) | Server-persisted profile | Consistent across devices |
| Dashboard configuration | Platform config API | Version-controlled, CI/CD deployable |

#### 9.14 Live Tail and Streaming

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
