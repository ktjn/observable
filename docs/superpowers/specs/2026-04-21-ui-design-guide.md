# UI Design Guide

This guide documents the Observable platform's design system: tokens, layout components,
typography, color semantics, interactive controls, and patterns for building new UI.
All values derive from **Tailwind CSS v4** configuration and the requirements in `spec/05-frontend.md`.

---

## 1. Design Tokens

Tokens are managed via Tailwind CSS v4 variables. All components consume these variables using Tailwind classes; avoid raw CSS where possible.

### 1.1 Color Tokens

| Token | Tailwind Class | Purpose |
|---|---|---|
| Background | `bg-background` | Page / app background |
| Surface | `bg-surface` | Card, panel, sidebar, topbar |
| Surface Subtle | `bg-surface-subtle` | Hover state, zebra rows, muted fills |
| Border | `border-border` | All borders and dividers |
| Text | `text-foreground` | Primary body text |
| Muted | `text-muted` | Secondary / supporting text, icon tint |
| Brand | `text-brand` / `bg-brand` | Interactive accent |


### 1.2 Semantic Status Tokens

| Token | Light | Dark | Usage |
|---|---|---|---|
| `--good` | `#15803d` | `#4ade80` | Success text / icon |
| `--good-bg` | `#e8f7ee` | `#123420` | Success badge / tile background |
| `--warn` | `#b45309` | `#fbbf24` | Warning text / icon |
| `--warn-bg` | `#fff4df` | `#3a2a0a` | Warning badge / tile background |
| `--bad` | `#b91c1c` | `#f87171` | Error / incident text / icon |
| `--bad-bg` | `#fdeaea` | `#3a1414` | Error badge / tile background |
| `--info-bg` | `#e8f1ff` | `#12233f` | Informational highlight (no health signal) |

**Color semantics rule (from spec/05-frontend.md §9.9):**

| Color | Meaning |
|---|---|
| Green (`--good`) | Within SLO / no active alerts |
| Amber (`--warn`) | Approaching threshold / minor anomaly |
| Red (`--bad`) | SLO breach / active incident |
| Blue (`--brand`, `--info-bg`) | Informational / no health signal yet |

Apply health color to text, badges, and the top border of metric tiles.
Never use raw color values; always use the semantic token.

---

## 2. Typography

**Font stack:**
```css
font-family: Inter, ui-sans-serif, system-ui, -apple-system,
             BlinkMacSystemFont, "Segoe UI", sans-serif;
```

All form controls (`button`, `input`, `select`) inherit the document font via `font: inherit`.

### Type Scale

| Role | Size | Weight | Class / element |
|---|---|---|---|
| Page title | 28px | — | `<h1>` |
| Topbar title | 18px | 750 | `.topbar-title` |
| Metric value | 24px | 800 | `.metric-value` |
| Empty state title | 22px | 800 | `.empty-title` |
| Body / nav link | inherit (~14px) | 650 | `.nav-link` |
| Label / column header | 12px | 700 | `.field-label`, `th` |
| Secondary / muted | 12px | — | `.brand-context`, `.metric-label` |
| Context pill / small UI | 13px | 650 | `.context-pill`, `.secondary-link` |
| Status badge | 12px | 750 | `.status` |

**Label pattern:** uppercase + `color: var(--muted)` — used for table column headers and field labels.

---

## 3. Spacing & Shape

| Token | Value | Applied to |
|---|---|---|
| Border radius — small | 6px | Inputs, buttons, nav links, pills, segments |
| Border radius — medium | 8px | Cards, panels, brand mark |
| Border radius — pill | 999px | Status badges |
| Content padding | 24px | `.content-shell` |
| Content padding (small screen) | 16px | `.content-shell` at ≤ 560px |
| Sidebar padding | 20px | `.sidebar` |
| Topbar padding | 14px 24px | `.topbar` |
| Metric tile padding | 14px | `.metric-tile` |
| Table cell padding | 12px 14px | `th`, `td` |
| Nav link padding | 10px 12px | `.nav-link` |

All elements use `box-sizing: border-box`.

---

## 4. Layout

### 4.1 App Shell

The outermost layout is a two-column CSS grid:

```css
.app-shell {
  min-height: 100vh;
  display: grid;
  grid-template-columns: 260px minmax(0, 1fr);
}
```

At ≤ 860px the sidebar stacks above the workspace (`grid-template-columns: 1fr`).

### 4.2 Sidebar

- Width: 260px (collapses on mobile)
- Background: `var(--surface)`, right border `var(--border)`
- Internal layout: `flex-direction: column; gap: 24px; padding: 20px`
- `.sidebar-footer` pushes to bottom via `margin-top: auto`

**Brand lockup** inside the sidebar:
- `.brand-mark`: 36×36px rounded square (`border-radius: 8px`), `background: var(--brand)`, white text, `font-weight: 800`
- `.brand-name`: `font-weight: 750`
- `.brand-context`: 12px muted text (project / environment name)

### 4.3 Navigation Links

```css
.nav-link {
  border-radius: 6px;
  padding: 10px 12px;
  color: var(--muted);
  font-weight: 650;
}
```

- **Hover / active**: `background: var(--surface-subtle)`, `color: var(--text)`
- **Active only**: `box-shadow: inset 3px 0 0 var(--brand)` (left-side accent bar)
- Nav list gap: 6px between items
- On mobile (≤ 860px): nav list becomes a 2-column grid

### 4.4 Workspace

```css
.workspace {
  min-width: 0;
  display: grid;
  grid-template-rows: auto 1fr;
}
```

- `.topbar`: min-height 72px, `var(--surface)` background, bottom border
  - Left: `.topbar-title` (18px / 750 weight)
  - Right: `.topbar-controls` (flex row, gap 10px, wraps on overflow)
- `.content-shell`: padding 24px; contains `.page-stack`

### 4.5 Page Stack

```css
.page-stack {
  display: grid;
  gap: 18px;
}
```

`.page-header` inside page stack: flex row, `align-items: end`, `justify-content: space-between`.

---

## 5. Components

### 5.1 Metric Tile

Displays a single KPI (request rate, error rate, latency, etc.).

```
┌─────────────────────┐  ← 3px colored top border
│ LABEL               │  ← .metric-label (12px muted)
│ 123ms               │  ← .metric-value (24px / 800)
└─────────────────────┘
```

Classes and border-top colors:

| Class | Border color |
|---|---|
| `.metric-tile` (default) | `var(--brand)` |
| `.metric-tile.good` | `var(--good)` |
| `.metric-tile.warn` | `var(--warn)` |
| `.metric-tile.bad` | `var(--bad)` |

`.metric-grid` arranges tiles in a 4-column responsive grid:
- Default: `repeat(4, minmax(140px, 1fr))`
- ≤ 860px: 2 columns
- ≤ 560px: 1 column

### 5.2 Status Badge

Inline pill indicating operational state.

```css
.status {
  border-radius: 999px;
  padding: 3px 8px;
  font-size: 12px;
  font-weight: 750;
}
```

| Class | Text color | Background |
|---|---|---|
| `.status.good` | `var(--good)` | `var(--good-bg)` |
| `.status.warn` | `var(--warn)` | `var(--warn-bg)` |
| `.status.bad` | `var(--bad)` | `var(--bad-bg)` |

Use badges for service health state, SLO status, alert severity, and deployment state.

### 5.3 Table Panel

```css
.table-panel {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow-x: auto;
}
```

Table inside:
- Full width, `border-collapse: collapse`
- `th`: 12px uppercase muted text; `td`: normal body text
- Cell padding: 12px 14px
- Row separator: `border-bottom: 1px solid var(--border)` on each cell
- `.strong-cell`: `font-weight: 750` for primary identifier columns

### 5.4 Empty Panel

Shown when a panel has no data yet (e.g., no services onboarded).

```css
.empty-panel {
  min-height: 280px;
  display: grid;
  align-content: center;
  justify-items: center;
  gap: 14px;
  padding: 28px;
}
```

- `.empty-title`: 22px / 800 weight
- `.empty-metrics`: flex-wrap row of hint chips, `color: var(--muted)`, 13px
  - Each chip: `border: 1px solid var(--border)`, `border-radius: 6px`, `padding: 6px 8px`

### 5.5 Search & Select Inputs

```css
.search-input,
.select-input {
  min-height: 38px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--surface);
  color: var(--text);
  padding: 0 10px;
}

.search-input { width: min(360px, 100%); }
```

All inputs and buttons inherit the document font (`font: inherit` on `button, input, select`).

### 5.6 Context Pill / Secondary Link

Used in the topbar for environment/project context and for secondary navigation actions.

```css
.context-pill, .secondary-link {
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 7px 10px;
  background: var(--surface);
  color: var(--muted);
  font-size: 13px;
  font-weight: 650;
}

.secondary-link:hover {
  color: var(--text);
  border-color: var(--brand);
}
```

### 5.7 Segmented Control

Used for theme selection and other mutually exclusive options (e.g., Light / Dark / System).

```css
.segmented-control {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  border: 1px solid var(--border);
  border-radius: 6px;
  overflow: hidden;
}

.segment {
  border: 0;
  border-right: 1px solid var(--border);
  background: var(--surface);
  color: var(--muted);
  min-height: 34px;
  cursor: pointer;
}

.segment.active {
  background: var(--brand);
  color: white;
}
```

The last segment omits the right border.

### 5.8 Trace Explorer Panel

Full-bleed bordered panel used for the trace search view and other explorer pages.

```css
.trace-explorer-page {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 20px;
}
```

Use this pattern for any full-width explorer surface (log explorer, metric explorer, etc.).

---

## 6. Theming

Theme is controlled by the `data-theme` attribute on `<html>`:

| `data-theme` value | Result |
|---|---|
| _(absent)_ | Light mode (`:root` defaults) |
| `"dark"` | Dark mode (`[data-theme="dark"]` overrides) |

The `ThemeProvider` in `apps/frontend/src/lib/theme.tsx` manages this:
- User preference (`"light"` | `"dark"` | `"system"`) stored in `localStorage` under `observable.theme`
- `"system"` resolves via `prefers-color-scheme` and updates when the OS preference changes
- `data-themePreference` on the root element reflects the stored preference

The sidebar footer renders a segmented control (`Light / Dark / System`) that calls `setTheme()`.

Do not read `localStorage` directly in components — consume the `useTheme()` hook from `lib/theme.tsx`.

---

## 7. Responsive Breakpoints

| Breakpoint | Behavior changes |
|---|---|
| ≤ 860px | Sidebar stacks above workspace; topbar becomes vertical (`flex-direction: column`); nav list and metric grid become 2-column |
| ≤ 560px | Content shell padding reduces to 16px; nav list and metric grid collapse to 1 column |

---

## 8. Information Density Model

Apply the **inverted pyramid** (spec/05-frontend.md §9.9) — show minimum information to assess health,
expand on demand:

| Level | View | What to show |
|---|---|---|
| L0 | Service Catalog row | Health ring, error rate %, P95 latency, incident badge |
| L1 | Service health overview | RED metrics, SLO burn, last deployment, alert count |
| L2 | Signal explorer | Time series + filterable results table |
| L3 | Item detail | Full attribute set, raw payload, correlated signals |

**Avoid** showing all signal types simultaneously on a single page. Use progressive disclosure:
hide detail behind tabs, side panels, or drill-down navigation.

---

## 9. Cross-Signal Correlation Patterns

These interaction patterns must be consistent across all explorer and detail views:

| Pattern | Implementation |
|---|---|
| "View Trace" link | Render whenever a `trace_id` or `span_id` is present; opens waterfall in a side panel |
| Breadcrumb trail | `Service: checkout-api  >  Trace abc123  >  Log line (span: xyz456)`; deep-linkable, survives reload |
| Related insights panel | Sidebar on service/trace detail: metrics anomalies, deployments ±5 min, SLO burn, incidents |
| Side-by-side panes | Trace waterfall (left) + correlated logs (right) with synchronized time cursor |
| Log context | "View Context" opens ±1 min surrounding logs from same service/host, ignoring current filters |
| Promote to Dashboard | Available on every ad-hoc query result in any explorer |

---

## 10. Accessibility Baseline

Target: WCAG 2.1 AA (spec/05-frontend.md §9.11).

- All interactive elements must be keyboard-reachable and show a visible focus ring (`focus-visible:ring-2`).
- Custom components must carry ARIA roles where the semantic HTML element is absent.
- Color is never the **sole** indicator of status — pair color with a text label or icon.
- Sufficient contrast: `text-foreground` on `bg-background` must meet 4.5:1 in both themes.
- Use **Base UI** primitives (as specified in the stack) for complex components (dialogs, dropdowns, tooltips) to inherit accessibility behavior. We follow the **Shadcn pattern** of owning the component code in `src/components/ui`.

---

## 11. Anti-Patterns

| Do not | Instead |
|---|---|
| Hard-code hex values in component code | Use Tailwind classes or token variables |
| Read `localStorage` for theme directly | Use `useTheme()` from `lib/theme.tsx` |
| Build custom complex interactive logic | Use **Base UI** primitives |
| Show all signal types on one page simultaneously | Use tabs, drill-down, or side panels |

---

## 12. Adding New Components

Checklist when adding a component to `apps/frontend/src/components/`:

1. Use Tailwind CSS v4 classes for all styling.
2. Test the component in both light and dark themes.
3. Verify at 860px and 560px breakpoints.
4. Add ARIA roles / labels if not using a native HTML element.
5. Use **Base UI** for complex interactive logic.
6. Co-locate a `.test.tsx` file using `@testing-library/react`.
