# Frontend Design System Modernization

**Date:** 2026-06-18
**Status:** Approved (design phase)

## Context

A UX review of the Observable frontend (via the Playwright visual suite — `apps/frontend/e2e/visual.spec.ts` / `navigation.spec.ts`) found that the default "Sys" (light) theme reads as an unstyled internal admin tool rather than a product competitive with Datadog, Grafana, New Relic, or Honeycomb: flat gray palette with no accent color, no shadows/radius, native unstyled form controls, no sidebar iconography, 11px base type on a condensed font, and a CSS-bar histogram / D3 topology view that look unfinished due to styling rather than missing functionality.

The app currently ships three themes (`light` / `dark` / `vt220`) defined in `apps/frontend/src/styles.css`, switched via `apps/frontend/src/lib/theme.tsx`. `dark` and `vt220` are a deliberate green-phosphor / amber CRT-terminal pastiche. This effort modernizes the visual language across **all three** themes while preserving them as three distinct, selectable options (vt220 keeps an amber identity but loses literal CRT styling).

## Goals

- Replace the flat, colorless, shadow-less visual language with a real design-token system (accent color, elevation, radius, type scale) applied consistently across light/dark/vt220.
- Add sidebar and status iconography (currently `.nav-icon { display: none; }`).
- Replace native unstyled form controls (search input, select dropdowns, topbar pickers) with styled equivalents.
- Fix the two visualizations that currently look broken due to styling: the histogram (CSS bars → SVG with axes/tooltips) and the topology map (D3 force-sim engine already works — needs theme-aware styling, legend, and an empty/sparse-state).
- Re-verify visually via the existing Playwright visual suite after each major surface change, across all three themes.

## Non-goals

- No backend/API changes.
- No change to the NLQ `VisualizationPanel` table-only rendering (separate concern, not part of this pass).
- No change to Incidents/Workbench pages beyond what the existing visual suite covers (they are not currently part of `visual.spec.ts`/`navigation.spec.ts`; if this pass touches shared chrome (sidebar, topbar, panel, table primitives) those pages will inherit the change automatically, but no dedicated screenshot coverage will be added for them in this pass).
- No new charting dependency (no recharts, no react-flow) — extend the existing D3/SVG patterns already used by `TimeSeriesGraph`.

## Design

### 1. Design tokens (`apps/frontend/src/styles.css`)

For each of the three `:root[data-theme="..."]` blocks (light, dark, vt220):

- **Accent color**: add `--accent` and `--accent-strong` (blue family, e.g. `#4F46E5`/`#4338CA`-ish for light; an appropriately shifted blue for dark; vt220 keeps its amber as `--accent` to preserve its identity — i.e. vt220's "accent" stays amber, light/dark get the new blue). This is distinct from `--brand`, which today is overloaded as both "primary action color" and "default text-on-dark color". `--brand`/`--brand-strong` remain for nav-active background; `--accent` becomes the color for primary buttons, links, focused chart series, and any new "primary action" affordance.
- **Elevation**: replace `--shadow-panel: none` and `--shadow-control: none` with real values — one subtle shadow for panels/cards, a slightly stronger one for popovers/dropdowns/the open context panel. Dark/vt220 use a lighter-alpha shadow appropriate to dark backgrounds.
- **Radius**: add `--radius-sm` (4px) and `--radius-md` (8px) tokens; apply to panels, buttons, inputs, badges, cards. (`--radius` already exists as an ad hoc fallback in `.context-pill`/`.secondary-link` — formalize it.)
- **Type scale**: raise `body` base font-size from `11px` to `13px`; raise the smallest labels (currently 9px) to 10-11px. Switch the body font-family from `'IBM Plex Sans Condensed', 'Arial Narrow', Arial, sans-serif` to a standard (non-condensed) UI sans stack (e.g. `'Inter', system-ui, ...` — confirm availability/license before adding a webfont, otherwise use the system-ui stack already implied). Keep `IBM Plex Mono`/condensed usage scoped to dense tabular numeric data only (trace IDs, durations, table numeric columns). Add `font-variant-numeric: tabular-nums` to numeric table cells and metric values.

### 2. Iconography

- Add `lucide-react` as a dependency.
- Re-enable `.nav-icon` (`apps/frontend/src/styles.css:234-236` currently sets `display: none`) and assign one icon per sidebar entry (Traces, Logs, Metrics, Services, Infrastructure, Dashboards, Alerts & SLOs, Incidents, Workbench, Setup/Getting Started, Administration).
- Add status icons (in addition to existing colored text badges) for error/warn/ok/healthy/watch/breach states in tables — icon + existing badge, not a replacement, to keep scanability.

### 3. Form controls

- Restyle `.search-input` / `.select-input` (`apps/frontend/src/styles.css:487-500`) and the topbar time-range/timezone/tenant/environment pickers: keep native `<select>`/`<input>` elements for accessibility/semantics, apply custom appearance (remove native chrome via `appearance: none` + custom chevron icon, themed border/background/focus-ring) rather than building a custom listbox widget.
- Ensure `:focus-visible` uses the existing `--focus-ring` token consistently (audit current usage; native `outline` should not be silently relied upon).

### 4. Component polish

- Table header (`thead tr` in `styles.css:565-568`): replace solid `--brand` (black) background with a tinted-gray header background per theme; verify `.modern-table-row:hover` is visibly applied.
- Panels (`.modern-panel`, `.modern-panel-header`): apply new shadow + radius tokens; tighten header padding/spacing to match the existing context-panel pattern (`SELECTED SPAN` / `Context Properties` panel), which is the best-looking existing surface and should be the reference for retrofitting list/table pages.
- Buttons: introduce a primary button style using `--accent` (currently all buttons appear to use the same neutral/bordered look — "Apply query", "New Rule", "Promote to dashboard" should read as primary actions).

### 5. Visualization fixes

- **Histogram** (`apps/frontend/src/components/ui/histogram.tsx`): rebuild rendering from CSS grid/flex bars to inline SVG, following the existing pattern in `apps/frontend/src/components/ui/time-series-graph.tsx` (axis labels, gridlines, hover tooltips). Preserve existing props contract (`buckets`, `categoryColors`, `categoryOrder`, `onRangeSelect`) and the drag-to-zoom interaction — this is a rendering-layer change, not an API change. Multi-series colors pull from theme tokens (status colors for error/warn/ok categories) rather than hardcoded values.
- **TopologyMap** (`apps/frontend/src/components/topology/TopologyMap.tsx`): keep the existing D3 force-simulation/pan/zoom/drag engine as-is. Fix the SVG background to use `--surface`/`--bg` theme tokens instead of rendering black; add a small legend (node = service, edge color = error rate, per existing edge-coloring logic at lines 277-296); add an explicit empty/sparse-state message when there are 0 or 1 nodes ("No service dependencies detected yet" or similar) so a single isolated node doesn't read as a bug.

### 6. Verification

- After each major surface change (tokens → icons → forms → tables/panels → histogram → topology), run `cd apps/frontend && npm run test:visual` per the existing `AGENTS.md` "UI Visual Verification" mandate.
- Visually inspect the regenerated screenshots in `apps/frontend/e2e/screenshots/` for all 6 main routes (Traces, Logs, Services, Infrastructure, Alerts, Dashboards) plus the panel-open and topology screenshots, across all three themes (the suite currently runs against whichever theme is default at test time — add a theme-loop or manually re-run with each `data-theme` value set, since the suite is not currently parameterized by theme).
- No new automated visual-regression assertions are required beyond the existing "screenshot + eyeball" pattern already documented in `AGENTS.md`.

## Risks / open questions

- The visual suite doesn't currently screenshot all three themes — verifying dark/vt220 will require either a manual theme switch + re-run, or a small addition to the test setup to loop themes. This spec assumes manual verification is sufficient; the implementation plan should flag if that proves too slow and a test parameterization is worth adding.
- Swapping the body font family may require adding a webfont or confirming a system-ui fallback looks acceptable — implementation should confirm before committing to a specific font name.
- `IBM Plex Sans Condensed` is currently set as the *only* body font (not just a fallback); some labels (`.field-label`, `.metric-label`, `.facet-title`) rely on its condensed width at very small sizes — these will need their letter-spacing/size re-checked once the font changes.
