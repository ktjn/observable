# Frontend Design System Modernization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Modernize the Observable frontend's visual language (color/shadow/radius/type tokens, sidebar iconography, styled form controls, table/panel/button polish, and the histogram/topology visualizations) across all three themes (light, dark, vt220), per `docs/superpowers/specs/2026-06-18-frontend-design-system-modernization-design.md`.

**Architecture:** This is a token-and-component-styling pass on the existing React 19 + Tailwind v4 + CSS-custom-property theming system in `apps/frontend/src/styles.css` and `apps/frontend/src/lib/theme.tsx`. No new charting dependency — the histogram is rebuilt as inline SVG following the existing `TimeSeriesGraph` pattern, and the existing D3-force-simulation `TopologyMap` is restyled in place, not replaced.

**Tech Stack:** React 19, TypeScript, Tailwind CSS v4 (`@tailwindcss/vite`), `tailwind-merge`/`clsx` (`cn()` helper), D3 v7 (topology only), Vitest + Testing Library (unit), Playwright (`e2e/visual.spec.ts`, `e2e/navigation.spec.ts` — visual verification).

## Global Constraints

- npm only — never pnpm/yarn/bun (`AGENTS.md`).
- Use the latest stable version of any new dependency (`lucide-react`); check npmjs.com before pinning.
- Run `cd apps/frontend && npm run test:visual` before and after any change that touches layout, CSS classes, component structure, or page-level routing (`AGENTS.md` "UI Visual Verification"). Screenshots write to `apps/frontend/e2e/screenshots/`; review them visually — the suite passes even if the UI looks wrong, so eyeball every changed screenshot.
- Keep all three themes (`light`, `dark`, `vt220`) functioning as distinct, selectable options — do not remove a theme or its `data-theme` selector block.
- Do not change the `Histogram` component's exported prop contract (`buckets`, `categoryOrder`, `categoryColors`, `format`, `onRangeSelect`, `onBucketCountChange`, `ariaLabel`, `title`, `subtitle`) — `TraceSearch.tsx` and `LogSearch.tsx` consume it as-is.
- Preserve the per-segment `title` attribute format `` `${format(bucket.startMs)} ${cat}: ${count}` `` on histogram bars — `TraceSearch.test.tsx:158` and `LogSearch.test.tsx` query it via `histogram.querySelector("[title*='Traces: 1']")`.
- No backend/API changes. No change to `apps/frontend/src/features/nlq/VisualizationPanel.tsx` or Incidents/Workbench pages (out of scope — see spec's Follow-up section).

---

### Task 1: Design tokens — accent, elevation, radius, type scale

**Files:**
- Modify: `apps/frontend/src/styles.css:14-150` (all three `:root` theme blocks + `body`/`h1` rules)

**Interfaces:**
- Produces: new CSS custom properties `--accent`, `--accent-strong`, `--accent-bg`, `--shadow-sm`, `--shadow-md`, `--radius-sm`, `--radius-md` on `:root`, `:root[data-theme="dark"]`, and `:root[data-theme="vt220"]`. All later tasks reference these exact names.

- [ ] **Step 1: Add new tokens to the light theme (`:root`)**

In `apps/frontend/src/styles.css`, inside the `:root { ... }` block (currently ends at line 45 with `font-family: ...;`), add before the closing `}`:

```css
  --accent: #4f46e5;
  --accent-strong: #4338ca;
  --accent-bg: #eef2ff;
  --shadow-sm: 0 1px 2px rgba(17, 17, 17, 0.06);
  --shadow-md: 0 4px 12px rgba(17, 17, 17, 0.12);
  --radius-sm: 4px;
  --radius-md: 8px;
```

Also replace the two existing lines:
```css
  --shadow-panel: none;
  --shadow-control: none;
```
with:
```css
  --shadow-panel: var(--shadow-sm);
  --shadow-control: var(--shadow-sm);
```

- [ ] **Step 2: Add matching tokens to the dark theme**

In the `:root[data-theme="dark"] { ... }` block, add the same four new properties with dark-appropriate values, and update the two `--shadow-*` lines the same way:

```css
  --accent: #6366f1;
  --accent-strong: #818cf8;
  --accent-bg: #1a1a3a;
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.4);
  --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.5);
  --radius-sm: 4px;
  --radius-md: 8px;
```
```css
  --shadow-panel: var(--shadow-sm);
  --shadow-control: var(--shadow-sm);
```

- [ ] **Step 3: Add matching tokens to the vt220 theme (amber accent, not blue)**

In the `:root[data-theme="vt220"] { ... }` block, add (vt220 keeps its amber identity for `--accent` per the approved spec — do not use blue here):

```css
  --accent: #FFB000;
  --accent-strong: #FFD050;
  --accent-bg: #2a2200;
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.5);
  --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.6);
  --radius-sm: 4px;
  --radius-md: 8px;
```
```css
  --shadow-panel: var(--shadow-sm);
  --shadow-control: var(--shadow-sm);
```

- [ ] **Step 4: Bump the base type scale**

Replace:
```css
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-size: 11px;
}
```
with:
```css
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-size: 13px;
}
```

Replace the light-theme `font-family` line inside `:root`:
```css
  font-family: 'IBM Plex Sans Condensed', 'Arial Narrow', Arial, sans-serif;
```
with a standard (non-condensed) UI sans stack:
```css
  font-family: 'Inter', system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif;
```

Leave the dark theme's font-family unset (it inherits the `:root` value) and leave the vt220 theme's `font-family: "Glass TTY VT220", "IBM Plex Mono", monospace;` line untouched — vt220 keeps its monospace identity.

- [ ] **Step 4b: Re-check small label classes for the new non-condensed font**

`.field-label`, `.metric-label`, and `.facet-title` were sized for a condensed font at 9px (`styles.css:199-204`, `:753-760`). With the body font now non-condensed, bump these to stay legible without growing the layout too much. Replace:
```css
.brand-context,
.field-label,
.metric-label {
  color: var(--muted);
  font-size: 9px;
}
```
with:
```css
.brand-context,
.field-label,
.metric-label {
  color: var(--muted);
  font-size: 10px;
}
```
and replace the `.facet-title` rule:
```css
.facet-title {
  color: var(--muted);
  font-size: 9px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin-bottom: 6px;
}
```
with:
```css
.facet-title {
  color: var(--muted);
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.3px;
  margin-bottom: 6px;
}
```
(Letter-spacing reduced slightly since the new font is wider than the condensed one at the same tracking.)

- [ ] **Step 5: Apply elevation + radius to panels**

Replace:
```css
.modern-panel {
  background: var(--surface-raised);
  border: 1px solid var(--border-strong);
}
```
with:
```css
.modern-panel {
  background: var(--surface-raised);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-panel);
}
```

- [ ] **Step 6: Apply tabular-nums to numeric table cells**

Add a new rule directly after the existing `th, td { ... }` block in `styles.css`:

```css
.strong-cell,
td.numeric {
  font-variant-numeric: tabular-nums;
}
```

(This targets the existing `.strong-cell` class already used for emphasized numeric values; `td.numeric` is a hook for any cell a later task wants to opt into tabular alignment — no caller needs to add it yet.)

- [ ] **Step 7: Verify nothing broke and capture a baseline**

Run:
```bash
cd apps/frontend
npm run test
npm run test:visual
```
Expected: all existing Vitest and Playwright tests still pass (this task only adds/edits CSS values, no markup or class-name changes). Review the regenerated screenshots in `apps/frontend/e2e/screenshots/` — panels should now show a subtle shadow and rounded corners; body text should be visibly larger and in a non-condensed font.

- [ ] **Step 8: Commit**

```bash
git add apps/frontend/src/styles.css
git commit -m "feat(frontend): add accent/elevation/radius tokens and bump base type scale"
```

---

### Task 2: Sidebar iconography with lucide-react

**Files:**
- Modify: `apps/frontend/package.json` (add `lucide-react` dependency)
- Modify: `apps/frontend/src/components/TreeNav.tsx`
- Modify: `apps/frontend/src/components/AppShell.tsx:1-58` (`buildNavTree`)
- Modify: `apps/frontend/src/styles.css:234-236` (`.nav-icon` / add `.tree-link-icon`)
- Test: `apps/frontend/src/components/TreeNav.test.tsx`

**Interfaces:**
- Consumes: nothing from earlier tasks (token names from Task 1 are not required here).
- Produces: `NavTreeItem.icon?: LucideIcon` field; later tasks do not depend on this.

- [ ] **Step 1: Install lucide-react**

```bash
cd apps/frontend
npm install lucide-react@latest
```

Confirm the resolved version lands in `package.json` under `dependencies` and `package-lock.json` is updated.

- [ ] **Step 2: Write the failing test**

In `apps/frontend/src/components/TreeNav.test.tsx`, add (near the top, after the existing imports) an icon import and a new test item, then a new test:

```tsx
import { Home } from "lucide-react";
```

Update `testItems` so the `"home"` entry carries an icon:
```tsx
  { id: "home", label: "Home", to: "/", icon: Home },
```

Add a new test at the end of the `describe("TreeNav", ...)` block:
```tsx
  test("renders an icon when item.icon is provided", () => {
    render(<TreeNav items={testItems} pathname="/" />);

    const homeLink = screen.getByText("Home").closest("a")!;
    expect(homeLink.querySelector("svg")).toBeInTheDocument();
  });

  test("renders no icon element when item.icon is omitted", () => {
    render(<TreeNav items={testItems} pathname="/" />);

    const setupLink = screen.getByText("Setup").closest("a")!;
    expect(setupLink.querySelector("svg")).not.toBeInTheDocument();
  });
```

- [ ] **Step 3: Run the test to verify it fails**

```bash
cd apps/frontend
npx vitest run src/components/TreeNav.test.tsx
```
Expected: FAIL — `NavTreeItem` has no `icon` property (TypeScript) and/or `homeLink.querySelector("svg")` is null.

- [ ] **Step 4: Add the `icon` field to `NavTreeItem` and render it in `TreeNode`**

In `apps/frontend/src/components/TreeNav.tsx`, update the type and import:

```tsx
import { Link, useLocation } from "@tanstack/react-router";
import { useState, useCallback } from "react";
import type { LucideIcon } from "lucide-react";

export type NavTreeItem = {
  id: string;
  label: string;
  to?: string;
  icon?: LucideIcon;
  children?: NavTreeItem[];
};
```

In `TreeNode`, render the icon before the label in all three branches (`Link`, `button`, `span`):

```tsx
  const Icon = item.icon;

  return (
    <div className="tree-node">
      <div className="tree-node-row">
        <span className="tree-toggle-area">
          {hasChildren && (
            <button
              type="button"
              className={`tree-toggle${isExpanded ? " expanded" : ""}`}
              onClick={() => onToggle(item.id)}
              aria-expanded={isExpanded}
              aria-label={isExpanded ? "Collapse" : "Expand"}
            />
          )}
        </span>
        {item.to ? (
          <Link to={item.to} className={linkClass}>
            {Icon && <Icon className="tree-link-icon" aria-hidden="true" size={14} />}
            {item.label}
          </Link>
        ) : hasChildren ? (
          <button
            type="button"
            className={linkClass}
            onClick={() => onToggle(item.id)}
          >
            {Icon && <Icon className="tree-link-icon" aria-hidden="true" size={14} />}
            {item.label}
          </button>
        ) : (
          <span className={linkClass}>
            {Icon && <Icon className="tree-link-icon" aria-hidden="true" size={14} />}
            {item.label}
          </span>
        )}
      </div>
```

- [ ] **Step 5: Add the icon layout CSS**

In `apps/frontend/src/styles.css`, replace:
```css
.nav-icon {
  display: none;
}
```
with:
```css
.tree-link-icon {
  flex-shrink: 0;
  margin-right: 6px;
  color: var(--muted);
}

.tree-link.active .tree-link-icon {
  color: var(--accent);
}
```

Also update `.tree-link` (and `.nav-link`, used by the older nav-list system if still referenced anywhere) to lay out as a flex row so the icon and label sit side by side:
```css
.tree-link {
  flex: 1;
  display: flex;
  align-items: center;
  padding: 4px 10px 4px 2px;
  font-size: 11px;
  font-weight: 400;
  color: var(--muted);
  text-decoration: none;
  border-left: 2px solid transparent;
  min-height: 24px;
}
```
(This is the existing rule at `styles.css:318-329` — it's already `display: flex; align-items: center;`, so only confirm it, no change needed if so; otherwise add the two missing properties.)

- [ ] **Step 6: Run the test to verify it passes**

```bash
cd apps/frontend
npx vitest run src/components/TreeNav.test.tsx
```
Expected: PASS.

- [ ] **Step 7: Wire real icons into the nav tree in AppShell**

In `apps/frontend/src/components/AppShell.tsx`, add the import:

```tsx
import {
  Home as HomeIcon,
  Wrench,
  Database,
  Workflow,
  Network,
  LayoutDashboard,
  BellRing,
  Siren,
  Settings,
  Server,
} from "lucide-react";
```

Update `buildNavTree` to attach an icon per top-level/child item:

```tsx
function buildNavTree(showGettingStarted: boolean): NavTreeItem[] {
  const base: NavTreeItem[] = [
    { id: "home", label: "Home", to: "/", icon: HomeIcon },
    {
      id: "setup",
      label: "Setup",
      icon: Wrench,
      children: [
        { id: "setup-ingest", label: "Ingest", to: "/setup" },
        { id: "setup-llm", label: "LLM", to: "/setup/llm" },
        { id: "setup-tokens", label: "Tokens", to: "/setup/tokens" },
      ],
    },
    { id: "workbench", label: "Workbench", to: "/workbench", icon: Database },
    { id: "services", label: "Services", to: "/services", icon: Workflow },
    {
      id: "signals",
      label: "Signals",
      icon: Network,
      children: [
        { id: "traces", label: "Traces", to: "/traces" },
        { id: "logs", label: "Logs", to: "/logs" },
        { id: "metrics", label: "Metrics", to: "/metrics" },
      ],
    },
    { id: "infrastructure", label: "Infrastructure", to: "/infrastructure", icon: Server },
    { id: "dashboards", label: "Dashboards", to: "/dashboards", icon: LayoutDashboard },
    { id: "alerts", label: "Alerts & SLOs", to: "/alerts", icon: BellRing },
    { id: "incidents", label: "Incidents", to: "/incidents", icon: Siren },
    {
      id: "admin",
      label: "Administration",
      icon: Settings,
      children: [
        { id: "admin-overview", label: "Overview", to: "/admin" },
        { id: "admin-config", label: "Tenant configuration", to: "/admin/config" },
        { id: "admin-fleet", label: "Fleet management", to: "/admin/fleet" },
        { id: "admin-identity", label: "Identity", to: "/admin/identity" },
      ],
    },
  ];
  if (showGettingStarted) {
    return [{ id: "getting-started", label: "Getting Started ✦", to: "/getting-started" }, ...base];
  }
  return base;
}
```

(Children items keep no icon — only top-level entries get one, matching the visual density of competitor sidebars without doubling up icons at every depth.)

- [ ] **Step 8: Run the full frontend test suite and the visual suite**

```bash
cd apps/frontend
npm run test
npm run test:visual
```
Expected: all pass. Review screenshots — every top-level sidebar item should now show an icon to the left of its label.

- [ ] **Step 9: Commit**

```bash
git add apps/frontend/package.json apps/frontend/package-lock.json apps/frontend/src/components/TreeNav.tsx apps/frontend/src/components/TreeNav.test.tsx apps/frontend/src/components/AppShell.tsx apps/frontend/src/styles.css
git commit -m "feat(frontend): add lucide-react sidebar icons"
```

---

### Task 3: Styled select controls

**Files:**
- Modify: `apps/frontend/src/styles.css` (`.search-input`, `.select-input`, `.context-pill`)
- Modify: `apps/frontend/src/components/AppShell.tsx:127-187` (theme/time-format/tenant/environment selects)
- Modify: `apps/frontend/src/components/GlobalDateRangePicker.tsx:25-37`
- Test: `apps/frontend/src/components/AppShell.test.tsx`, `apps/frontend/src/components/GlobalDateRangePicker.test.tsx`

**Interfaces:**
- Consumes: `--accent`, `--radius-sm`, `--shadow-control` from Task 1.
- Produces: a `.themed-select` CSS class other tasks/pages can apply to any native `<select>` for consistent appearance (no JS API — pure class).

- [ ] **Step 1: Read the existing tests so the refactor doesn't break them**

```bash
cd apps/frontend
npx vitest run src/components/AppShell.test.tsx src/components/GlobalDateRangePicker.test.tsx
```
Expected: PASS (establish the baseline before changing anything). Note which queries are used (e.g. `getByLabelText("Theme preference")`, `getByLabelText("Global time range")`) so the markup changes in this task preserve those `aria-label`s and `role="combobox"` semantics — native `<select>` stays a `<select>`, only its CSS appearance changes.

- [ ] **Step 2: Add the shared `.themed-select` class**

In `apps/frontend/src/styles.css`, replace the existing `.search-input, .select-input { ... }` block:

```css
.search-input,
.select-input {
  min-height: 28px;
  border: 1px solid var(--border-strong);
  background: var(--surface-raised);
  color: var(--text);
  padding: 0 8px;
  font-family: 'IBM Plex Mono', 'Courier New', monospace;
  font-size: 11px;
}
```

with:

```css
.search-input,
.select-input,
.themed-select {
  min-height: 28px;
  border: 1px solid var(--border-strong);
  background: var(--surface-raised);
  color: var(--text);
  padding: 0 8px;
  font-family: 'IBM Plex Mono', 'Courier New', monospace;
  font-size: 11px;
  border-radius: var(--radius-sm);
}

.themed-select {
  appearance: none;
  padding-right: 24px;
  background-image: linear-gradient(45deg, transparent 50%, var(--muted) 50%),
    linear-gradient(135deg, var(--muted) 50%, transparent 50%);
  background-position:
    calc(100% - 14px) center,
    calc(100% - 9px) center;
  background-size: 5px 5px, 5px 5px;
  background-repeat: no-repeat;
  cursor: pointer;
}

.themed-select:focus-visible {
  outline: none;
  box-shadow: 0 0 0 2px var(--bg), 0 0 0 4px var(--accent);
}
```

(This is a CSS-only chevron built from two gradients — no extra icon asset needed — so it themes automatically via `var(--muted)`/`var(--accent)` per theme.)

- [ ] **Step 3: Apply `.themed-select` and remove inline style overrides in `AppShell.tsx`**

Replace the four `<select>` elements in `apps/frontend/src/components/AppShell.tsx` (theme preference, time display format, tenant, environment) — drop every inline `style={{...}}` prop and use the new class instead. Example for the theme select (lines 127-136):

```tsx
          <select
            aria-label="Theme preference"
            className="themed-select"
            value={preference}
            onChange={(e) => setPreference(e.target.value as ThemePreference)}
          >
            {themeOptions.map((option) => (
              <option key={option.value} value={option.value}>{option.label}</option>
            ))}
          </select>
```

Apply the same pattern (drop `style`, set `className="context-pill themed-select"` since these three already carry `className="context-pill"`) to the time-format, tenant, and environment selects at lines 145-155, 156-172, 173-187 — keep their existing `className="context-pill"` and append `themed-select`, e.g. `className="context-pill themed-select"`, and keep `maxWidth` constraints by moving them into `styles.css` as new modifier classes instead of inline styles:

```css
.context-pill.themed-select[aria-label="Tenant"] {
  max-width: 10rem;
}

.context-pill.themed-select[aria-label="Environment"] {
  max-width: 9rem;
}
```

- [ ] **Step 4: Apply the same treatment in `GlobalDateRangePicker.tsx`**

Replace the `<select>` at lines 26-36:

```tsx
  return (
    <select
      aria-label="Global time range"
      className="context-pill themed-select"
      value={preset}
      onChange={(e) => setPreset(e.target.value as typeof preset)}
    >
      {PRESET_OPTIONS.map((opt) => (
        <option key={opt.value} value={opt.value}>{opt.label}</option>
      ))}
    </select>
  );
```

- [ ] **Step 5: Re-run the existing tests to confirm no regressions**

```bash
cd apps/frontend
npx vitest run src/components/AppShell.test.tsx src/components/GlobalDateRangePicker.test.tsx
```
Expected: PASS — these tests query by `aria-label`/role, which is unchanged; only `className`/`style` changed.

- [ ] **Step 6: Run the visual suite**

```bash
cd apps/frontend
npm run test:visual
```
Expected: all pass. Review screenshots — topbar selects and the sidebar theme picker should show a custom chevron instead of the native OS dropdown arrow, with rounded corners.

- [ ] **Step 7: Commit**

```bash
git add apps/frontend/src/styles.css apps/frontend/src/components/AppShell.tsx apps/frontend/src/components/GlobalDateRangePicker.tsx
git commit -m "feat(frontend): style native select controls with themed chevron and focus ring"
```

---

### Task 4: Primary button accent + table header polish

**Files:**
- Modify: `apps/frontend/src/components/ui/button.tsx`
- Modify: `apps/frontend/src/styles.css` (`thead tr`, `.modern-table-row:hover td`)
- Test: `apps/frontend/src/components/ui/button.test.tsx`

**Interfaces:**
- Consumes: `--accent`, `--accent-strong` from Task 1.
- Produces: no new exports — `Button`'s `primary` variant now visually maps to `--accent` instead of `--brand`; signature unchanged.

- [ ] **Step 1: Write the failing test**

In `apps/frontend/src/components/ui/button.test.tsx`, add:

```tsx
test("primary variant uses the accent color token", () => {
  render(<Button variant="primary">Save</Button>);
  const button = screen.getByRole("button", { name: "Save" });
  expect(button.className).toContain("--accent");
});
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd apps/frontend
npx vitest run src/components/ui/button.test.tsx
```
Expected: FAIL — current `primary` class string contains `--brand`, not `--accent`.

- [ ] **Step 3: Update the `primary` variant**

In `apps/frontend/src/components/ui/button.tsx`, replace:

```tsx
  primary:
    "bg-[var(--brand)] text-[var(--bg)] hover:bg-[var(--brand-strong)] disabled:bg-[var(--surface-subtle)] disabled:text-[var(--muted)]",
```

with:

```tsx
  primary:
    "bg-[var(--accent)] text-white hover:bg-[var(--accent-strong)] disabled:bg-[var(--surface-subtle)] disabled:text-[var(--muted)]",
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
cd apps/frontend
npx vitest run src/components/ui/button.test.tsx
```
Expected: PASS.

- [ ] **Step 5: Soften the table header**

In `apps/frontend/src/styles.css`, replace:

```css
thead tr {
  background: var(--brand);
  color: var(--bg);
}
```

with:

```css
thead tr {
  background: var(--surface-subtle);
  color: var(--text-strong);
  border-bottom: 1px solid var(--border-strong);
}
```

Note this also affects `th.sortable:hover` (`styles.css:658-660`), which blends toward `--brand` — update it to blend toward the new header background instead:

```css
th.sortable:hover {
  background: color-mix(in srgb, var(--accent) 15%, var(--surface-subtle));
}
```

- [ ] **Step 6: Run the full frontend test suite and visual suite**

```bash
cd apps/frontend
npm run test
npm run test:visual
```
Expected: all pass. Review screenshots — table headers should be a tinted gray (not solid black) with dark text, and primary buttons ("Apply query", "New Rule", "Promote to dashboard" wherever they use `<Button variant="primary">`) should show the new accent color. Note: if some of those buttons are currently raw `<button className="...">` rather than `<Button>`, they will not pick up this change — that's expected and out of scope for this task (flagged for the follow-up plan, not a regression).

- [ ] **Step 7: Commit**

```bash
git add apps/frontend/src/components/ui/button.tsx apps/frontend/src/components/ui/button.test.tsx apps/frontend/src/styles.css
git commit -m "feat(frontend): use accent token for primary buttons, soften table header"
```

---

### Task 5: Rebuild the histogram as SVG

**Files:**
- Modify: `apps/frontend/src/components/ui/histogram.tsx`
- Create: `apps/frontend/src/components/ui/histogram.test.tsx`

**Interfaces:**
- Consumes: nothing new from earlier tasks (uses existing `--border`, `--surface`, `--muted`, `--text-strong`, `--surface-inset`, `--surface-subtle` tokens already defined).
- Produces: same exported `Histogram<T>` component and `HistogramProps<T>`/`HistogramBucket<T>` types — signature unchanged. Internal rendering becomes SVG `<rect>` bars instead of CSS grid `<div>`s, but every per-segment element keeps a `title` attribute formatted exactly as `` `${format(bucket.startMs)} ${cat}: ${count}` `` (required by `TraceSearch.test.tsx`/`LogSearch.test.tsx`).

- [ ] **Step 1: Write the failing tests**

Create `apps/frontend/src/components/ui/histogram.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { Histogram, type HistogramBucket } from "./histogram";

type Cat = "ok" | "error";

const buckets: HistogramBucket<Cat>[] = [
  { startMs: 0, endMs: 1000, total: 5, categories: { ok: 5, error: 0 } },
  { startMs: 1000, endMs: 2000, total: 3, categories: { ok: 1, error: 2 } },
];

const categoryColors: Record<Cat, string> = { ok: "fill-[var(--good)]", error: "fill-[var(--bad)]" };

function renderHistogram(onRangeSelect?: (from: number, to: number) => void) {
  return render(
    <Histogram
      buckets={buckets}
      categoryOrder={["ok", "error"]}
      categoryColors={categoryColors}
      format={(ms) => String(ms)}
      onRangeSelect={onRangeSelect}
      ariaLabel="Test histogram"
    />,
  );
}

describe("Histogram", () => {
  test("renders an SVG element", () => {
    renderHistogram();
    const group = screen.getByRole("group", { name: "Test histogram" });
    expect(group.querySelector("svg")).toBeInTheDocument();
  });

  test("renders one bar segment per non-zero category with the expected title", () => {
    renderHistogram();
    const group = screen.getByRole("group", { name: "Test histogram" });
    expect(group.querySelector("[title='0 ok: 5']")).toBeInTheDocument();
    expect(group.querySelector("[title='1000 ok: 1']")).toBeInTheDocument();
    expect(group.querySelector("[title='1000 error: 2']")).toBeInTheDocument();
  });

  test("does not render a segment for a zero-count category", () => {
    renderHistogram();
    const group = screen.getByRole("group", { name: "Test histogram" });
    expect(group.querySelector("[title='0 error: 0']")).not.toBeInTheDocument();
  });

  test("calls onRangeSelect with bucket boundaries on drag", () => {
    const onRangeSelect = vi.fn();
    renderHistogram(onRangeSelect);
    const group = screen.getByRole("group", { name: "Test histogram" });
    const svg = group.querySelector("svg")!;
    const rect = { left: 0, width: 200, top: 0, height: 100, right: 200, bottom: 100, x: 0, y: 0, toJSON() {} };
    svg.getBoundingClientRect = () => rect as DOMRect;
    svg.dispatchEvent(new PointerEvent("pointerdown", { clientX: 10, bubbles: true }));
    svg.dispatchEvent(new PointerEvent("pointermove", { clientX: 190, bubbles: true }));
    svg.dispatchEvent(new PointerEvent("pointerup", { clientX: 190, bubbles: true }));
    expect(onRangeSelect).toHaveBeenCalledWith(0, 2000);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd apps/frontend
npx vitest run src/components/ui/histogram.test.tsx
```
Expected: FAIL — current implementation renders `<div>` bars with `gridTemplateColumns`, not an `<svg>`, so `group.querySelector("svg")` is null.

- [ ] **Step 3: Rewrite `histogram.tsx` as SVG**

Replace the full contents of `apps/frontend/src/components/ui/histogram.tsx`:

```tsx
import { useEffect, useRef, useState } from "react";

export type HistogramBucket<T extends string = string> = {
  startMs: number;
  endMs: number;
  total: number;
  categories: Record<T, number>;
};

export interface HistogramProps<T extends string> {
  buckets: HistogramBucket<T>[];
  categoryOrder: T[];
  categoryColors: Record<T, string>;
  format: (ms: number) => string;
  onRangeSelect?: (fromMs: number, toMs: number) => void;
  onBucketCountChange?: (count: number) => void;
  ariaLabel?: string;
  title?: string;
  subtitle?: string;
}

const PLOT_HEIGHT = 96;
const GAP_PX = 2;

export function Histogram<T extends string>({
  buckets,
  categoryOrder,
  categoryColors,
  format,
  onRangeSelect,
  onBucketCountChange,
  ariaLabel = "Data volume histogram",
  title,
  subtitle,
}: HistogramProps<T>) {
  const max = Math.max(1, ...buckets.map((bucket) => bucket.total));
  const sectionRef = useRef<HTMLElement>(null);
  const [width, setWidth] = useState(400);

  const onBucketCountChangeRef = useRef(onBucketCountChange);
  useEffect(() => {
    onBucketCountChangeRef.current = onBucketCountChange;
  });

  useEffect(() => {
    const el = sectionRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const w = entry.contentRect.width;
      setWidth(Math.round(w));
      const count = Math.round(Math.floor(w / 10) / 5) * 5;
      onBucketCountChangeRef.current?.(Math.max(12, Math.min(100, count)));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const dragRef = useRef<{ startX: number; endX: number } | null>(null);
  const [dragDisplay, setDragDisplay] = useState<{ startX: number; endX: number } | null>(null);

  const barWidth = buckets.length > 0 ? width / buckets.length : 0;

  function xToMs(x: number): number {
    if (buckets.length === 0 || barWidth <= 0) return buckets[0]?.startMs ?? 0;
    const idx = Math.min(buckets.length - 1, Math.max(0, Math.floor(x / barWidth)));
    return buckets[idx].startMs;
  }

  function handlePointerDown(e: React.PointerEvent<SVGSVGElement>) {
    if (!onRangeSelect) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      /* jsdom */
    }
    dragRef.current = { startX: x, endX: x };
    setDragDisplay({ startX: x, endX: x });
  }

  function handlePointerMove(e: React.PointerEvent<SVGSVGElement>) {
    if (!dragRef.current) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    dragRef.current = { ...dragRef.current, endX: x };
    setDragDisplay({ ...dragRef.current });
  }

  function handlePointerUp() {
    const drag = dragRef.current;
    if (drag && onRangeSelect && buckets.length > 0) {
      const fromMs = xToMs(Math.min(drag.startX, drag.endX));
      const toMs = xToMs(Math.max(drag.startX, drag.endX)) + (buckets[1]?.startMs ?? buckets[0].endMs) - buckets[0].startMs;
      onRangeSelect(fromMs, toMs);
    }
    dragRef.current = null;
    setDragDisplay(null);
  }

  const selStartX = dragDisplay ? Math.min(dragDisplay.startX, dragDisplay.endX) : -1;
  const selEndX = dragDisplay ? Math.max(dragDisplay.startX, dragDisplay.endX) : -1;

  return (
    <section
      ref={sectionRef}
      role="group"
      aria-label={ariaLabel}
      className="border border-[var(--border)] bg-[var(--surface)] p-3"
    >
      {(title || subtitle || categoryOrder.length > 0) && (
        <div className="mb-3 flex items-center justify-between gap-3">
          <div>
            {subtitle && <div className="text-xs font-bold uppercase text-[var(--muted)]">{subtitle}</div>}
            {title && <h2 className="m-0 text-sm font-bold text-[var(--text-strong)]">{title}</h2>}
          </div>
          <div className="flex flex-wrap gap-2 text-xs text-[var(--muted)]">
            {categoryOrder.map((cat) => (
              <span key={cat} className="inline-flex items-center gap-1">
                <span className={`h-2 w-2 ${categoryColors[cat]}`} />
                {cat}
              </span>
            ))}
          </div>
        </div>
      )}
      <p className="sr-only">Drag over bars to zoom into a time range.</p>
      <svg
        width="100%"
        height={PLOT_HEIGHT}
        viewBox={`0 0 ${width} ${PLOT_HEIGHT}`}
        aria-hidden="true"
        className="select-none"
        style={{ cursor: onRangeSelect ? "crosshair" : "default" }}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={() => {
          dragRef.current = null;
          setDragDisplay(null);
        }}
      >
        {[0.25, 0.5, 0.75].map((r) => (
          <line
            key={r}
            x1={0}
            y1={PLOT_HEIGHT * r}
            x2={width}
            y2={PLOT_HEIGHT * r}
            stroke="var(--border)"
            strokeWidth={0.5}
          />
        ))}

        {buckets.map((bucket, i) => {
          const x = i * barWidth;
          const isSelected = dragDisplay !== null && x + barWidth / 2 >= selStartX && x + barWidth / 2 <= selEndX;
          let stackedHeight = 0;
          return (
            <g key={bucket.startMs}>
              <rect
                x={x}
                y={0}
                width={Math.max(0, barWidth - GAP_PX)}
                height={PLOT_HEIGHT}
                fill={isSelected ? "var(--surface-subtle)" : "var(--surface-inset)"}
              />
              {categoryOrder.map((cat) => {
                const count = bucket.categories[cat];
                if (count === 0) return null;
                const segHeight = Math.max(2, (count / max) * PLOT_HEIGHT);
                const y = PLOT_HEIGHT - stackedHeight - segHeight;
                stackedHeight += segHeight;
                return (
                  <rect
                    key={cat}
                    x={x}
                    y={y}
                    width={Math.max(0, barWidth - GAP_PX)}
                    height={segHeight}
                    className={categoryColors[cat]}
                    title={`${format(bucket.startMs)} ${cat}: ${count}`}
                  />
                );
              })}
            </g>
          );
        })}
      </svg>
    </section>
  );
}
```

Note: `categoryColors` values are now expected to be Tailwind `fill-[var(--xxx)]` class strings (rather than `bg-[var(--xxx)]`) since they're applied to SVG `<rect>` elements — Task 5's Step 5 below updates the two real call sites (`TraceSearch.tsx`, `LogSearch.tsx`).

- [ ] **Step 4: Run the histogram tests to verify they pass**

```bash
cd apps/frontend
npx vitest run src/components/ui/histogram.test.tsx
```
Expected: PASS.

- [ ] **Step 5: Update the two callers' category color classes**

```bash
cd apps/frontend
grep -rn "categoryColors" src/pages/TraceSearch.tsx src/pages/LogSearch.tsx
```

For each `categoryColors` object literal found, change every value from a `bg-[var(--x)]` string to the equivalent `fill-[var(--x)]` string (same CSS variable, just swap the Tailwind utility prefix from `bg-` to `fill-`).

- [ ] **Step 6: Run the dependent page tests**

```bash
cd apps/frontend
npx vitest run src/pages/TraceSearch.test.tsx src/pages/LogSearch.test.tsx
```
Expected: PASS — `histogram.querySelector("[title*='Traces: 1']")` still matches because the `title` attribute format is unchanged.

- [ ] **Step 7: Run the full frontend test suite and visual suite**

```bash
cd apps/frontend
npm run test
npm run test:visual
```
Expected: all pass. Review the Traces/Logs page screenshots — the "over time" histogram should now render as SVG bars with visible horizontal gridlines instead of plain CSS divs (visually this preserves the same stacked-bar look but is now SVG-based, ready for future axis-label/tooltip additions).

- [ ] **Step 8: Commit**

```bash
git add apps/frontend/src/components/ui/histogram.tsx apps/frontend/src/components/ui/histogram.test.tsx apps/frontend/src/pages/TraceSearch.tsx apps/frontend/src/pages/LogSearch.tsx
git commit -m "feat(frontend): rebuild histogram as SVG with gridlines"
```

---

### Task 6: Topology map theming and empty-state

**Files:**
- Modify: `apps/frontend/src/components/topology/TopologyMap.tsx`
- Create: `apps/frontend/src/components/topology/TopologyMap.test.tsx`

**Interfaces:**
- Consumes: `--surface`, `--surface-raised`, `--border`, `--border-strong`, `--text`, `--muted`, `--accent`, `--bad`, `--radius-md` tokens.
- Produces: same exported `TopologyMap`/`TopologyMapProps` signature — no change. New behavior: renders a legend and an empty-state message when `services.length <= 1`; no functional change to the force simulation, drag, zoom, or click handlers.

- [ ] **Step 1: Write the failing tests**

Create `apps/frontend/src/components/topology/TopologyMap.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { TopologyMap } from "./TopologyMap";
import type { TopologyEdge } from "../../api/services";

const noop = vi.fn();

describe("TopologyMap", () => {
  test("shows an empty-state message when there are 0 services", () => {
    render(
      <TopologyMap
        edges={[]}
        allServices={[]}
        focusedService={null}
        onNodeClick={noop}
        onEdgeClick={noop}
        onBackgroundClick={noop}
      />,
    );
    expect(screen.getByText(/no service dependencies detected yet/i)).toBeInTheDocument();
  });

  test("shows an empty-state message when there is exactly 1 service and no edges", () => {
    render(
      <TopologyMap
        edges={[]}
        allServices={["checkout"]}
        focusedService={null}
        onNodeClick={noop}
        onEdgeClick={noop}
        onBackgroundClick={noop}
      />,
    );
    expect(screen.getByText(/no service dependencies detected yet/i)).toBeInTheDocument();
  });

  test("does not show the empty-state when there are 2+ connected services", () => {
    const edges: TopologyEdge[] = [
      { caller: "checkout", callee: "payments", request_count: 10, error_rate: 0.01, p95_latency_ms: 50 },
    ];
    render(
      <TopologyMap
        edges={edges}
        allServices={["checkout", "payments"]}
        focusedService={null}
        onNodeClick={noop}
        onEdgeClick={noop}
        onBackgroundClick={noop}
      />,
    );
    expect(screen.queryByText(/no service dependencies detected yet/i)).not.toBeInTheDocument();
  });

  test("renders a legend", () => {
    render(
      <TopologyMap
        edges={[]}
        allServices={["checkout", "payments"]}
        focusedService={null}
        onNodeClick={noop}
        onEdgeClick={noop}
        onBackgroundClick={noop}
      />,
    );
    expect(screen.getByText(/error rate/i)).toBeInTheDocument();
  });
});
```

Check `apps/frontend/src/api/services.ts` for the exact `TopologyEdge` field names before running — if they differ from `caller`/`callee`/`request_count`/`error_rate`/`p95_latency_ms`, adjust the test to match the real type (the implementation file already uses these exact field names at `TopologyMap.tsx:241-295`, so they should match).

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd apps/frontend
npx vitest run src/components/topology/TopologyMap.test.tsx
```
Expected: FAIL — no empty-state text or legend exists yet.

- [ ] **Step 3: Add the empty-state and legend, and replace hardcoded colors**

In `apps/frontend/src/components/topology/TopologyMap.tsx`:

a) Replace the hardcoded node colors in `DraggableNode`'s `<circle>`:
```tsx
      <circle
        r={NODE_R}
        fill={isFocused ? "var(--accent-bg)" : "var(--surface-raised)"}
        stroke={isFocused ? "var(--accent)" : "var(--border-strong)"}
        strokeWidth="2"
        opacity={isActive ? 1 : 0.2}
      />
```
and the text fill:
```tsx
      <text
        textAnchor="middle"
        dominantBaseline="middle"
        fontSize="9"
        fontWeight="bold"
        fill="var(--text)"
        opacity={isActive ? 1 : 0.2}
        style={{ pointerEvents: "none", userSelect: "none" }}
      >
```

b) Replace the SVG background and arrow marker colors:
```tsx
      <svg
        ref={svgRef}
        width="100%"
        height="100%"
        style={{ background: "var(--surface)", borderRadius: "var(--radius-md)", display: "block" }}
      >
        <defs>
          <marker
            id="arrowhead"
            markerWidth="10"
            markerHeight="7"
            refX="28"
            refY="3.5"
            orient="auto"
          >
            <polygon points="0 0, 10 3.5, 0 7" fill="var(--muted)" />
          </marker>
          <marker
            id="arrowhead-error"
            markerWidth="10"
            markerHeight="7"
            refX="28"
            refY="3.5"
            orient="auto"
          >
            <polygon points="0 0, 10 3.5, 0 7" fill="var(--bad)" />
          </marker>
        </defs>
```

c) Replace the edge line colors:
```tsx
                <line
                  x1={start.x}
                  y1={start.y}
                  x2={end.x}
                  y2={end.y}
                  stroke={isError ? "var(--bad)" : "var(--muted)"}
                  strokeWidth={Math.max(1, Math.min(5, 1 + Math.log10(edge.request_count + 1)))}
                  markerEnd={isError ? "url(#arrowhead-error)" : "url(#arrowhead)"}
                  opacity={isActive ? 0.6 : 0.15}
                />
```

d) Wrap the returned JSX with a legend and an empty-state branch. Replace the final `return (...)` block:

```tsx
  if (services.length <= 1) {
    return (
      <div
        ref={wrapperRef}
        style={{ width: "100%", height: "100%" }}
        className="flex items-center justify-center border border-[var(--border)] bg-[var(--surface)] text-sm text-[var(--muted)]"
      >
        No service dependencies detected yet.
      </div>
    );
  }

  return (
    <div ref={wrapperRef} style={{ width: "100%", height: "100%", position: "relative" }}>
      <div className="absolute right-2 top-2 z-10 flex items-center gap-3 border border-[var(--border)] bg-[var(--surface-raised)] px-2 py-1 text-[10px] text-[var(--muted)]">
        <span className="inline-flex items-center gap-1">
          <span className="inline-block h-0.5 w-3" style={{ background: "var(--muted)" }} />
          Normal
        </span>
        <span className="inline-flex items-center gap-1">
          <span className="inline-block h-0.5 w-3" style={{ background: "var(--bad)" }} />
          Error rate &gt; 5%
        </span>
      </div>
      <svg
```

(Keep everything from the existing `<svg ref={svgRef} ...>` line through the closing `</svg>` and outer `</div>` exactly as it is today, aside from the color substitutions in steps a-c — only the opening wrapper changes, plus the new early-return branch above it.)

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd apps/frontend
npx vitest run src/components/topology/TopologyMap.test.tsx
```
Expected: PASS.

- [ ] **Step 5: Run the full frontend test suite and visual suite**

```bash
cd apps/frontend
npm run test
npm run test:visual
```
Expected: all pass. Review the Services Topology screenshot (`nav-services-topology.png`) — with the current single-node mock data it should now show the "No service dependencies detected yet." empty state instead of a lone circle on a black canvas. If you want to see the populated graph+legend rendering, temporarily mock 2+ edges in `e2e/navigation.spec.ts`'s topology test data, screenshot, then revert the temporary mock change (do not commit a mock-data change as part of this task unless the spec is updated to require expanded topology test coverage).

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/components/topology/TopologyMap.tsx apps/frontend/src/components/topology/TopologyMap.test.tsx
git commit -m "feat(frontend): theme topology map colors, add legend and empty state"
```

---

### Task 7: Cross-theme visual verification

**Files:**
- None modified — verification-only task.

**Interfaces:**
- Consumes: all visual changes from Tasks 1-6.
- Produces: a confirmation note (no code) that all three themes render correctly; do not proceed to declare the feature done without this.

- [ ] **Step 1: Run the full suite once more for the default theme**

```bash
cd apps/frontend
npm run test
npm run test:visual
```
Expected: all pass.

- [ ] **Step 2: Capture dark-theme screenshots**

The visual suite doesn't currently parameterize by theme. Temporarily force dark theme by adding `await page.addInitScript(() => localStorage.setItem("observable.theme", "dark"));` at the top of the `test.beforeEach` (or equivalent setup) in `apps/frontend/e2e/visual.spec.ts`, then run:
```bash
cd apps/frontend
npm run test:visual
```
Review all six route screenshots for the dark theme: confirm shadows/radius/icons/accent color render correctly against the dark palette, confirm text remains readable (no low-contrast regressions from the new `--accent`/`--shadow-*` values).

- [ ] **Step 3: Capture vt220-theme screenshots**

Change the same line to `localStorage.setItem("observable.theme", "vt220")`, run `npm run test:visual` again, and review all six screenshots for the vt220 theme: confirm the amber `--accent` renders correctly, the monospace font is undisturbed, and shadows/radius look intentional rather than out of place against the CRT aesthetic.

- [ ] **Step 4: Revert the temporary test change**

```bash
cd apps/frontend
git checkout -- e2e/visual.spec.ts
npm run test:visual
```
Expected: back to light-theme screenshots, all passing — confirms the temporary diagnostic edit didn't get committed.

- [ ] **Step 5: Note any theme-specific follow-ups**

If Steps 2-3 reveal a contrast or layout issue specific to dark/vt220 that isn't a quick fix, do not silently skip it — fix it now if small (e.g. a single token value), or stop and flag it to the user with the specific screenshot/issue before considering this plan complete.
