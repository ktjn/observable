# ADR-016: Grafana Visualization Strategy

**Date:** 2026-04-16  
**Status:** Accepted  
**Authors:** Claude Code  
**Deciders:** Project Stakeholders  
**Review date:** 2026-10-16  

## Context

The platform needs production-grade observability visualizations: time-series charts, heatmaps, flame graphs, stat panels, service maps, and log histograms. Building these from scratch (D3.js or similar) represents a significant engineering investment that would delay Phase 1 delivery and produce an inferior result compared to battle-tested alternatives.

Grafana is the industry reference implementation for observability visualization. Three integration models exist:

1. **Embed Grafana application** (iframes, Grafana as a backend service)
2. **Use Grafana's npm libraries** (`@grafana/ui`, `@grafana/scenes`) inside our React app
3. **Build custom charts** from a general-purpose library (Recharts, Nivo, Victory)

The choice has licensing, operational, and UX implications.

### Licensing Context

- **Grafana application** (binary, Docker image): AGPLv3. Distributing or offering a modified Grafana as a service requires either open-sourcing the entire derivative work or purchasing a Grafana Enterprise / OEM license.
- **`@grafana/ui`** (npm): Apache-2.0. No copyleft obligation. Safe to use in commercial SaaS.
- **`@grafana/data`** (npm): Apache-2.0. Same.
- **`@grafana/scenes`** (npm): Apache-2.0. Same.

Using the npm packages inside our own React application does not constitute distributing or modifying the Grafana application; AGPL does not apply.

### Forces in Tension

- **Speed vs. control**: `@grafana/ui` provides immediate access to production-grade panels; building from D3 offers total control at high cost.
- **Vendor risk vs. ecosystem benefit**: Taking a dependency on Grafana's npm packages ties us to their release cadence and breaking changes. The ecosystem is large, well-maintained, and used at massive scale.
- **Operational simplicity vs. feature breadth**: Running Grafana as a sidecar adds operational overhead, requires separate auth plumbing, and introduces iframe UX friction.
- **Licensing risk**: The Grafana application (AGPL) cannot be embedded in a SaaS platform without either open-sourcing the service or purchasing OEM licensing.

## Decision

**Use `@grafana/ui` and `@grafana/scenes` (both Apache-2.0) as the visualization and dashboard state layer within the platform's React application. Do not run a separate Grafana instance. Do not embed Grafana application iframes.**

Specifically:
- All time-series, stat, heatmap, bar gauge, table, and histogram panels are rendered using `@grafana/ui` panel components
- Dashboard state management (time range, variables, panel queries, layout) follows the `@grafana/scenes` architecture pattern
- The platform's query facade is wired to Grafana's `DataSource` adapter interface, making our signals available as a Grafana-compatible data source within the app
- Flame graphs and service map visualizations use purpose-built components (`@grafana/ui` flame graph panel; canvas-based graph for service map)

## Consequences

**Easier:**
- Immediate access to production-quality observability charts: time series with annotations, heatmaps, exemplar scatter plots, stat panels, flame graphs
- Dashboard state model is well-understood and documented; reduces design ambiguity
- When Grafana ships new panel types, adoption is an npm version bump
- Operators familiar with Grafana panels recognize the visualization language

**Harder:**
- Must track `@grafana/ui` and `@grafana/scenes` major versions; these libraries have breaking changes between major versions
- Some Grafana panel components assume a `PanelData` model that requires adapting the query facade response format
- The Grafana npm packages have large transitive dependencies; bundle size must be monitored and tree-shaken aggressively

**Constrained:**
- Custom chart types not available in `@grafana/ui` must be built as wrapper components matching the same `PanelProps` interface, keeping the abstraction consistent
- Visual design customization is constrained to what `@grafana/ui`'s theming system supports (color tokens, typography). Deep restyling is possible but requires maintaining a custom Grafana theme object

## Alternatives Considered

### Option A: Embed Grafana Application (iframes / Grafana as sidecar service)

Run a Grafana instance alongside the platform. Embed dashboard iframes in the React app.

**Rejected because:**
- AGPLv3 obligation: offering a modified Grafana as SaaS requires either open-sourcing the full service codebase or purchasing OEM commercial licensing
- Iframe embedding introduces cross-origin auth complexity (cookie sharing, CORS, CSP headers)
- UX friction: iframe boundaries prevent seamless navigation, deep linking, and cross-signal correlation flows that require full React context
- Operational overhead: separate deployment, health checks, upgrade lifecycle, Grafana config management
- Grafana's data source model requires running a Grafana backend to proxy queries; our query facade would be duplicated

### Option B: Grafana as a Separate Complementary Service (first-class alongside the custom UI)

Offer Grafana as an opt-in dashboard environment for operators who want ecosystem plugins, while the custom React app handles correlation and exploration.

**Rejected (for now) because:**
- Splits the UX surface: operators must context-switch between two applications, losing the correlation breadcrumb model
- Does not eliminate the AGPL licensing question
- Adds integration maintenance: auth sync, data scope enforcement, tenant isolation must be maintained in both systems
- Revisit in Phase 4 if enterprise customers require Grafana-ecosystem plugins that cannot be replicated in the custom UI. An ADR amendment would be required, along with Grafana OEM commercial licensing

### Option C: Build All Charts from D3.js or Recharts/Nivo

Implement every visualization type using a general-purpose charting library.

**Rejected because:**
- Recharts, Nivo, and Victory lack observability-specific panel types (flame graphs, heatmaps with exemplars, log histograms)
- Building production-quality time-series charts with annotation, exemplar, and threshold overlay support from D3 is a multi-sprint effort that delays Phase 1
- Results in an inferior product compared to Grafana-quality panels for months or years
- Does not solve the real problem: the domain-specific visualization problem is already solved by `@grafana/ui`

### Option D: Use Observable Plot or Vega-Lite

General-purpose, grammar-of-graphics libraries with good TypeScript support.

**Rejected because:**
- No observability-specific panels out of the box
- Higher learning curve for contributors unfamiliar with the grammar-of-graphics model
- Same problem as Option C: does not provide flame graphs, exemplar scatter, or log histograms without significant custom work

## Related

- `spec/05-frontend.md` § 9.1 Stack, § 9.7 Dashboard Architecture
- `ADR-006-react-vite-frontend.md` — React/Vite decision; this ADR extends the visualization choice
- `ADR-015-build-vs-buy.md` — Build vs. buy boundary; visualization is "buy" via Apache-2.0 npm packages
- `spec/13-risks-roadmap.md` § Risk 1 (single-engine fantasy) and Risk 7 (no cost model) — visualization library choice has analogous build-vs-buy and lock-in trade-offs; no dedicated chart library risk exists in the risks registry
