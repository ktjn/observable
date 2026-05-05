# UI Design Research: Grafana Integration and Observability UI Best Practices

**Date:** 2026-04-16  
**Status:** Accepted — drives spec/05-frontend.md revision and ADR-016  
**Author:** Claude Code  

---

## Summary

This document captures research findings that informed the revision of `spec/05-frontend.md` and the creation of `ADR-016`. The key conclusions are:

1. **Use `@grafana/ui` and `@grafana/scenes` (Apache-2.0 npm packages)** for the visualization layer — not the Grafana application itself (AGPL).
2. **Organize navigation around entities (services), not signal types** — the New Relic One model outperforms signal-centric layouts for incident response.
3. **Cross-signal correlation is the differentiating capability** — implement breadcrumbs, related insights panels, and side-by-side trace+log views as first-class features, not afterthoughts.
4. **Dashboard-as-code is the correct default** — provide a UI builder that generates the same JSON artifact, not a separate "export" workflow.
5. **High-cardinality exploration requires a dedicated UI pattern** — BubbleUp-style comparison (Honeycomb model) surfaces anomalies the operator would never find manually.

### 2026-04-18 Suitability Review

The current conclusion remains: **Grafana is suitable as a visualization dependency and optional
interop target, but not suitable as the primary Observable UI.**

Use Grafana in three bounded ways:

1. **Adopt Grafana libraries inside the custom React UI**: continue using `@grafana/ui`,
   `@grafana/data`, and `@grafana/scenes` for panels, DataFrames, time ranges, variables, and
   dashboard-like scene state.
2. **Keep the Observable dashboard format compatible with Grafana concepts**: panels, variables,
   layout, annotations, dashboard JSON, and data frames should remain easy to adapt to Grafana.
3. **Defer a managed Grafana service or Grafana data source plugin to Phase 4+**: only add it if
   enterprise users require existing Grafana dashboards or plugins, and only with explicit
   licensing, auth, tenant isolation, and data-scope review.

Do not replace the Observable React application with Grafana OSS/Enterprise as the main UI in
Phase 1–3. That would move core product workflows outside the platform UX plane defined in
`spec/02-architecture.md` and would weaken the service-first navigation, cross-signal breadcrumbs,
query workbench, admin console, and tenant-aware audit model required by `spec/05-frontend.md`.

---

## 1. Grafana Integration Options

### 1.1 Grafana Application (AGPL — DO NOT USE for SaaS)

The Grafana binary and Docker images are licensed under AGPLv3. Distributing a modified Grafana as a service — or bundling it in a SaaS product — without open-sourcing the full derivative codebase requires a commercial OEM license from Grafana Labs.

**The AGPL network provision is binding**: running Grafana as a hosted service for users qualifies as distribution under AGPLv3 if the application is modified. Even unmodified use as a commercial SaaS may require a commercial license depending on the business model.

**Embedding via iframes** also introduces:
- Cross-origin authentication complexity (cookie samesite policy, CORS headers, JWT forwarding)
- CSP header conflicts with modern React SPAs
- UX fragmentation: iframes cannot participate in the React component tree, preventing unified navigation, breadcrumbs, and cross-signal correlation flows

### 1.2 Grafana npm Libraries (Apache-2.0 — RECOMMENDED)

The following npm packages are separately licensed under Apache-2.0:

| Package | Version at research | Purpose |
|---|---|---|
| `@grafana/ui` | 11.x | Panel components: TimeSeries, BarGauge, Heatmap, Stat, Table, FlameGraph, Logs |
| `@grafana/data` | 11.x | Data frame model, field types, transformations |
| `@grafana/scenes` | 5.x | Dashboard state architecture: SceneObjects, time range, variables, layout |
| `@grafana/scenes-react` | 5.x | React hooks and context providers for Scenes |

Using these packages inside a custom React application does not create AGPL obligations. The Apache-2.0 license permits commercial use, modification, and distribution with only attribution requirements.

**What this gives us:**
- Production-grade time-series panel with threshold bands, anomaly fill, annotation markers, and exemplar scatter points
- Heatmap panel for log volume and latency distribution
- Flame graph panel for profiling (built into `@grafana/ui` as of v10+)
- Stat and bar gauge panels for SLO burn rate and RED golden signal cards
- Structured log list panel with ANSI color rendering
- Table panel with field overrides and link injection
- Full theming system (light/dark) with token-based customization
- The `@grafana/scenes` state model for managing dashboard panels, queries, variables, and time ranges as a serializable tree

### 1.3 Grafana as a Complementary Service (Future Option)

Running Grafana as an opt-in sidecar for enterprise customers who need ecosystem plugins (Kubernetes dashboards, Postgres datasource, etc.) is a viable Phase 4+ option. This would require:
- Grafana OEM commercial licensing
- Tenant-isolated Grafana instances or separate org-per-tenant configuration
- Shared OIDC/SSO integration
- Data scope enforcement at the query facade level for both surfaces

This path is not blocked by the current architecture. The query facade already exposes a query API that can be wired as a Grafana data source plugin. An ADR amendment (ADR-016 revision) would be required if this path is taken.

### 1.4 Grafana Application as Primary UI (2026-04-18 Assessment)

Grafana as a complete UI should be treated as **not suitable for the primary product surface**.

| Dimension | Finding | Suitability |
|---|---|---|
| Product workflow fit | Grafana is dashboard- and data-source-centric; Observable requires entity-centric investigation, service catalog, trace/log/metric correlation, onboarding, admin, fleet, quota, and audit workflows. | Poor |
| Cross-signal correlation | Grafana supports links, annotations, correlations, and Explore-style workflows, but the Observable spec requires URL-owned investigation context, breadcrumbs, split panes, and service-first navigation across all modules. | Partial |
| Tenant isolation | Grafana has organizations, folders, permissions, and Enterprise RBAC, but Observable requires tenant context on every API, queue message, storage write, query, and audit record. That isolation belongs in the platform query facade, not a parallel UI authority. | Partial |
| Auth and embedding | Grafana can authenticate with its own mechanisms and API tokens, but iframe embedding requires enabling `allow_embedding`, changing cookie/CSP behavior, and accepting frame-bound UX and security complexity. | Poor |
| Dashboard-as-code | Grafana has dashboard APIs, provisioning, stable dashboard UIDs, and JSON dashboard models. This is useful as an interop target, not enough to replace the custom dashboard artifact model. | Good |
| Plugin ecosystem | Grafana data source plugins can expose an in-house query API through DataFrames, query editors, and health checks. This is valuable for external compatibility. | Good |
| Licensing | Grafana core projects moved to AGPLv3; commercial/OEM licensing is required if modified network-service use must avoid AGPL obligations. Grafana Enterprise licensing adds active-user and feature constraints. | Risky unless explicitly licensed |
| Operational ownership | Running Grafana introduces another deployable service, database/config lifecycle, plugin lifecycle, SSO/RBAC sync path, and incident surface. | Mixed |

**Conclusion:** Grafana is better used as a library and compatibility surface than as the product
shell. The custom React UI remains necessary because Observable's differentiator is not generic
dashboard rendering; it is tenant-safe, entity-centric, cross-signal operations.

### 1.5 Grafana Plugin or Sidecar Path (Deferred)

If later customer demand requires Grafana ecosystem compatibility, implement it as a separate
Phase 4+ slice:

- Build an Observable Grafana data source plugin that talks only to the Observable query facade.
- Return Grafana DataFrames from the plugin adapter, preserving tenant and auth context at the
  query facade boundary.
- Use Grafana dashboard APIs or provisioning only for import/export compatibility, not as the
  authoritative Observable dashboard store.
- Require Grafana OEM/commercial licensing review before offering a bundled or modified Grafana
  service to customers.
- Require an ADR-016 amendment and updates to `spec/05-frontend.md`, `spec/09-api.md`,
  `spec/10-process.md`, and deployment/security specs before shipping a managed Grafana surface.

This keeps compatibility available without forcing Phase 1–3 product workflows through Grafana's
application model.

---

## 2. New Relic UI Patterns

### 2.1 One Observability Platform — Entity-Centric Navigation

New Relic's core insight: **operators think about services and deployments, not about metric types or log streams**. Their navigation model synthesizes all telemetry into "entities" (services, hosts, APIs, containers) and provides a unified health view per entity.

**Key patterns adopted:**
- Service catalog as the primary entry point (not a metrics browser or log search)
- Per-entity health ring (green/amber/red) derived from SLO status and alert state
- All signals filtered to the selected entity by default, reducing noise

**Patterns NOT adopted:**
- New Relic's NRQL proprietary query language — we use a query facade with an open API
- Entity definitions from external sources (GitHub newrelic/entity-definitions) — our entity model is defined in the platform domain model (spec/14-domain-model.md)

### 2.2 Explorer Views

New Relic's Navigator (density grid), Lookout (anomaly circles), and Explorer (list with context) show that **multiple density modes for the same entity list serve different workflows**:

- High-density grid: incident commander scanning many services simultaneously
- Sorted list with metrics: SRE reviewing SLO burn rates
- Topology map: architect understanding dependencies

The platform should support at minimum a **list view** (Phase 1) and a **service map** (Phase 3) for the service catalog.

---

## 3. Datadog UI Patterns

### 3.1 Trace → Log → Metric Drill-Down

Datadog's standard incident response flow demonstrates the correct drill-down sequence:

```
Alert fires (metric anomaly)
  → Filtered trace list (error traces in alert window)
    → Trace waterfall (specific failing request)
      → Correlated log lines (same trace_id and time window)
        → Metric graph (same service, same time window)
```

Each step preserves the time window and service filter context. The operator never re-enters search criteria after the initial alert.

This is the reference model for the platform's cross-signal breadcrumb system.

### 3.2 Notebooks (Runbooks + Post-Mortems)

Datadog Notebooks combine live graphs, filtered log results, and markdown commentary in a single shareable document. This is the reference model for the platform's **Query Workbench** module (Phase 2–3):
- Monaco editor for queries
- Live panel rendering inline with text
- Shareable URL
- Export to PDF / markdown

### 3.3 Watchdog (AI Anomaly Surface)

Datadog's Watchdog surfaces anomalies within the existing search context (not as a separate "AI" view). Key UX insight: **AI insights should appear where the operator already is**, not require navigation to an AI dashboard.

The platform's Phase 8 AI features should follow this pattern: surface anomaly badges on the service health overview and related insights panel, not require a separate "Watchdog" tab.

---

## 4. Honeycomb UI Patterns

### 4.1 BubbleUp — High-Cardinality Anomaly Surfacing

Honeycomb's BubbleUp is the most influential insight from this research: **the operator should not need to know which dimension caused an anomaly before they can investigate it**.

The workflow:
1. Operator views a heatmap or time series showing an anomaly window
2. Operator brushes/selects the anomalous region
3. BubbleUp automatically compares all field value distributions in the selected window vs. the baseline
4. Top dimensions by statistical divergence are ranked and shown as histograms

The result: a P99 latency spike caused by a single customer's requests (high-cardinality `customer_id` dimension) is surfaced instantly without the operator needing to know to look at `customer_id`.

**Implementation approach for this platform:**
- The query facade already returns trace/log/metric data with full attribute sets
- BubbleUp-style comparison is a GROUP BY + percentage comparison query, not ML
- The UI brushes a time window, sends two queries (anomaly window + baseline window), and renders a ranked list of divergent attribute values
- Available as a component in the Trace Explorer and Log Explorer views

### 4.2 Free-Form Exploration Over Pre-Indexed Dimensions

Honeycomb's event model (one JSON blob per request, all fields queryable) demonstrates that **operators need to query dimensions they didn't plan for at instrumentation time**. Pre-indexed cardinality limits are a product constraint, not a feature.

The platform should:
- Allow filtering on any attribute in trace spans and log records without requiring pre-indexing (ClickHouse supports this via full column scans)
- Autocomplete attribute key suggestions from recent query result sets, not from a static schema registry
- Make cardinality budgets visible (Cardinality Inspector) but not a blocker for ad-hoc exploration

---

## 5. Best Practices Summary

### 5.1 Information Architecture

| Principle | Implementation |
|---|---|
| Inverted pyramid | Service catalog → service detail → signal explorer → item detail |
| Entity-centric, not signal-centric | Navigate from service; signals are filtered views of a service |
| Progressive disclosure | Show health rings at L0; full attribute tables only on demand |
| Color semantics | Green/amber/red for health; blue for neutral; consistent across all views |

### 5.2 Time Range Design

- Unified global time range shared across all panels on a page
- One-click comparison with prior period (essential for SLO burn rate and deployment regression detection)
- Timezone shown explicitly whenever the display timezone differs from UTC
- Time range always in URL; page reload restores exact state

### 5.3 Cross-Signal Correlation

- Every `trace_id` in a log line renders a "View Trace" link (no copy-paste required)
- Every span in a trace waterfall that has correlated logs shows a log badge
- Metric exemplars link to the exact trace_id that produced the data point
- Deployment event markers overlaid on all time-series panels automatically
- Breadcrumb trail preserved across signal boundaries; deep-linkable

### 5.4 Dashboard Architecture

- Every dashboard is a serializable JSON document (dashboard-as-code)
- UI builder and code editor both target the same format — no divergence
- Dashboard API enables CI/CD validation and GitOps deployment
- Panels annotated with deployment events by default; operator opts out, not in

### 5.5 Service Map

- Derived from trace data, not manually configured
- Canvas rendering (not SVG) for performance beyond 100 nodes
- RED metrics overlaid on nodes: rate (RPS), error rate (%), duration (P95 ms)
- Diff mode to compare topology at two time windows

### 5.6 Accessibility and Performance

- WCAG 2.1 AA: keyboard navigation, ARIA roles, sufficient contrast in both light and dark modes
- Virtualized lists for log and trace result tables (≥ 200 rows)
- Skeleton loading states for dashboard panels
- Canvas-based rendering for service map and flame graphs

---

## 6. Competitive Positioning

| Capability | Platform (planned) | Grafana | New Relic | Datadog | Honeycomb |
|---|---|---|---|---|---|
| Cross-signal correlation (breadcrumbs) | Yes (Phase 1) | Manual (via links) | Yes (entity synthesis) | Yes (APM) | Partial |
| BubbleUp-style comparison | Yes (Phase 2) | No | No | Outliers only | Yes |
| Dashboard-as-code + UI builder | Yes (Phase 1/2) | Yes (Terraform + UI) | Limited | Yes (Terraform) | No |
| Entity-centric navigation | Yes (Phase 1) | No (signal-centric) | Yes | Partial | No |
| Flame graphs | Yes (`@grafana/ui`) | Yes | Yes | Yes | No |
| Multi-tenant isolation | Yes (spec/04) | Enterprise only | Yes | Yes | Limited |
| Open data model (OTLP) | Yes (ADR-001) | Yes | Partial | Partial | No |

The platform differentiates on **multi-tenant isolation + full OTLP data model + entity-centric correlation** — not on chart variety or AI features.

---

## 7. Files Changed by This Research

| File | Change | Why |
|---|---|---|
| `spec/05-frontend.md` | Full rewrite and expansion | Added 13 detailed sections covering stack, navigation model, correlation patterns, high-cardinality exploration, service map, dashboard architecture, time range system, information density model, performance requirements, state management |
| `spec/adr/ADR-016-grafana-visualization-strategy.md` | New ADR | Documents the decision to use `@grafana/ui` + `@grafana/scenes` (Apache-2.0) and rejects Grafana application embedding (AGPL) |

ADR-006 is NOT amended because the core React/Vite decision is unchanged. ADR-016 extends it with the visualization library decision.

ADR-015 (build vs. buy) is NOT amended because the visualization layer (npm packages) falls within the existing "buy" boundary for the frontend toolchain.

---

## 8. Sources Checked For 2026-04-18 Review

Primary sources checked:

- Grafana Labs licensing FAQ: core Grafana, Loki, and Tempo moved from Apache-2.0 to AGPLv3;
  plugins, agents, and some libraries remain Apache-licensed; commercial licensing is the route
  for modified network-service use that must avoid AGPL obligations.
  <https://grafana.com/licensing/>
- Grafana Enterprise license documentation: Enterprise features and active-user license limits
  require a purchased and activated Enterprise license.
  <https://grafana.com/docs/grafana/latest/enterprise/license/license-restrictions/>
- Grafana HTTP API reference: Grafana exposes APIs for dashboards, folders, data sources,
  organizations, service accounts, alerting, and related resources; new-generation APIs under
  `/apis` are replacing legacy `/api` routes.
  <https://grafana.com/docs/grafana/latest/developer-resources/api-reference/http-api/>
- Grafana dashboard API documentation: dashboard create, update, get, list, permission scopes,
  unique dashboard UIDs, and legacy dashboard endpoints support dashboard-as-code interop.
  <https://grafana.com/docs/grafana/latest/http_api/dashboard/>
- Grafana configuration documentation: `allow_embedding` defaults to `false` and otherwise sets
  `X-Frame-Options: deny`; cookie SameSite, CSP, CSRF, and other security settings affect embedded
  deployments.
  <https://grafana.com/docs/grafana/latest/installation/configuration/>
- Grafana security documentation: data source proxy and request-security behavior create an
  additional server-side request boundary that must be controlled when Grafana can reach internal
  services.
  <https://grafana.com/docs/grafana/latest/setup-grafana/configure-security/>
- Grafana data source plugin tutorial: custom data source plugins can query an in-house API,
  return DataFrames, provide a query editor, and implement health checks.
  <https://grafana.com/developers/plugin-tools/tutorials/build-a-data-source-plugin>
- Grafana DataFrame guide: DataFrames are the shared columnar shape for plugin query results and
  panel rendering.
  <https://grafana.com/developers/plugin-tools/how-to-guides/data-source-plugins/create-data-frames>
- Grafana Scenes core concepts: Scenes model dashboards as object trees containing data, time
  ranges, variables, layout, and visualizations, which matches the dashboard state architecture
  already adopted in `spec/05-frontend.md`.
  <https://grafana.com/developers/scenes/core-concepts>
- npm package metadata for `@grafana/ui`: current public package metadata lists Apache-2.0
  licensing and shows active publication of the React component library.
  <https://www.npmjs.com/package/@grafana/ui>

ADR/spec sync: no ADR or spec change is required for this review because it does not change the
accepted architecture. It documents that the existing ADR-016 choice remains suitable: use Grafana
libraries in the custom React application, while rejecting Grafana-as-primary-UI for now.
