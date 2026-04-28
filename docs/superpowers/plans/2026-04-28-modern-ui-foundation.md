# Modern UI Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Modernize the Observable frontend with a refined operational shell, shared layout primitives, and one migrated services surface without changing backend contracts.

**Architecture:** Keep the current React/Vite/TanStack Router app and the existing light/dark/system theme contract. Add small owned UI primitives under `apps/frontend/src/components/ui/`, then migrate `ProductAreaPage` to consume those primitives as the first proof point. Preserve the existing service-centric information architecture and dense observability UI model from `spec/05-frontend.md`.

**Tech Stack:** React 19, TypeScript, Vite 8, TanStack Query, TanStack Router, Base UI-owned primitive pattern, Tailwind CSS v4 utilities, Vitest + React Testing Library, Playwright + axe.

---

## Scope

This plan is the first modern-UI slice. It is intentionally not a full redesign.

In scope:

- richer shared visual tokens in `apps/frontend/src/styles.css`
- modernized `AppShell` navigation and topbar treatment
- shared `Panel`, `Badge`, `Toolbar`, `MetricCard`, and `EmptyState` components
- migration of the Services product page in `ProductAreaPage`
- focused component tests and accessibility coverage
- documentation update to the UI design guide

Out of scope:

- backend API changes
- dashboard builder work
- charting library changes
- full feature-directory migration
- broad rewrite of trace, log, infrastructure, or alert pages
- ADR changes unless implementation changes the approved design-system stack

## File Structure

- Modify `apps/frontend/src/styles.css`: add modern neutral/elevation tokens and shell/table utility classes while preserving current tokens.
- Modify `apps/frontend/src/components/AppShell.tsx`: add icon-based nav metadata, stronger active state, and richer global context controls.
- Create `apps/frontend/src/components/ui/badge.tsx`: owned semantic status badge with `good`, `warn`, `bad`, and `info` tones.
- Create `apps/frontend/src/components/ui/badge.test.tsx`: render and theme-contract coverage for `Badge`.
- Create `apps/frontend/src/components/ui/panel.tsx`: shared bordered panel wrapper with optional header/action area.
- Create `apps/frontend/src/components/ui/panel.test.tsx`: verifies heading, body, and action rendering.
- Create `apps/frontend/src/components/ui/toolbar.tsx`: shared responsive toolbar row for filters and actions.
- Create `apps/frontend/src/components/ui/toolbar.test.tsx`: verifies accessible grouping and child rendering.
- Create `apps/frontend/src/components/ui/metric-card.tsx`: shared KPI tile/card with semantic tone.
- Create `apps/frontend/src/components/ui/metric-card.test.tsx`: verifies label/value/tone rendering.
- Create `apps/frontend/src/components/ui/empty-state.tsx`: shared empty/loading-like content block with optional actions.
- Create `apps/frontend/src/components/ui/empty-state.test.tsx`: verifies title, description, metadata, and actions.
- Modify `apps/frontend/src/pages/ProductAreaPage.tsx`: consume the new primitives for Services and existing placeholder product areas.
- Modify `apps/frontend/e2e/accessibility.spec.ts`: add a Services page axe check with mocked service and environment responses.
- Modify `docs/superpowers/specs/2026-04-21-ui-design-guide.md`: document the modernized shell and new primitives.

---

### Task 1: Shared Modern UI Tokens

**Files:**

- Modify: `apps/frontend/src/styles.css`

- [ ] **Step 1: Add non-breaking visual tokens**

Append these variables to the existing `:root` block without removing any existing token:

```css
  --surface-raised: #ffffff;
  --surface-inset: #f0f3f7;
  --border-strong: #b8c2d0;
  --text-strong: #0f1720;
  --shadow-panel: 0 1px 2px rgb(15 23 42 / 0.08), 0 8px 24px rgb(15 23 42 / 0.06);
  --shadow-control: 0 1px 1px rgb(15 23 42 / 0.06);
  --brand-bg: #e8f1ff;
```

Append these variables to the existing `:root[data-theme="dark"]` block:

```css
  --surface-raised: #202631;
  --surface-inset: #12161d;
  --border-strong: #465163;
  --text-strong: #ffffff;
  --shadow-panel: 0 1px 2px rgb(0 0 0 / 0.35), 0 12px 32px rgb(0 0 0 / 0.28);
  --shadow-control: 0 1px 1px rgb(0 0 0 / 0.3);
  --brand-bg: #12233f;
```

- [ ] **Step 2: Add reusable modern utility classes**

Add this block near the existing shared component styles:

```css
.modern-panel {
  background: var(--surface-raised);
  border: 1px solid var(--border);
  border-radius: 8px;
  box-shadow: var(--shadow-control);
}

.modern-panel-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 14px 16px;
  border-bottom: 1px solid var(--border);
}

.modern-panel-body {
  padding: 16px;
}

.modern-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
}

.modern-table-row:hover td {
  background: var(--surface-subtle);
}
```

- [ ] **Step 3: Run frontend typecheck**

Run:

```bash
npm --prefix apps/frontend run typecheck
```

Expected: PASS with no TypeScript errors.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/styles.css
git commit -m "style: add modern ui tokens"
```

---

### Task 2: Shared Badge Primitive

**Files:**

- Create: `apps/frontend/src/components/ui/badge.tsx`
- Create: `apps/frontend/src/components/ui/badge.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `apps/frontend/src/components/ui/badge.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { afterEach } from "vitest";
import { Badge } from "./badge";

afterEach(() => {
  delete document.documentElement.dataset.theme;
});

test("renders badge content", () => {
  render(<Badge tone="good">Healthy</Badge>);
  expect(screen.getByText("Healthy")).toBeInTheDocument();
});

test("marks status badges with a status role", () => {
  render(<Badge tone="bad">Breach</Badge>);
  expect(screen.getByRole("status")).toHaveTextContent("Breach");
});

test("renders under the dark theme contract", () => {
  document.documentElement.dataset.theme = "dark";
  render(<Badge tone="warn">Watch</Badge>);
  expect(screen.getByText("Watch")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
npm --prefix apps/frontend run test -- src/components/ui/badge.test.tsx --run
```

Expected: FAIL because `./badge` does not exist.

- [ ] **Step 3: Implement the primitive**

Create `apps/frontend/src/components/ui/badge.tsx`:

```tsx
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

type BadgeTone = "good" | "warn" | "bad" | "info" | "neutral";

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: BadgeTone;
  children: ReactNode;
}

const toneClasses: Record<BadgeTone, string> = {
  good: "bg-[var(--good-bg)] text-[var(--good)]",
  warn: "bg-[var(--warn-bg)] text-[var(--warn)]",
  bad: "bg-[var(--bad-bg)] text-[var(--bad)]",
  info: "bg-[var(--info-bg)] text-[var(--brand-strong)]",
  neutral: "bg-[var(--surface-subtle)] text-[var(--muted)]",
};

export function Badge({ tone = "neutral", className, children, ...props }: BadgeProps) {
  return (
    <span
      role="status"
      className={cn(
        "inline-flex min-h-6 items-center rounded-full px-2 text-xs font-bold",
        toneClasses[tone],
        className
      )}
      {...props}
    >
      {children}
    </span>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
npm --prefix apps/frontend run test -- src/components/ui/badge.test.tsx --run
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/components/ui/badge.tsx apps/frontend/src/components/ui/badge.test.tsx
git commit -m "feat: add badge primitive"
```

---

### Task 3: Shared Panel Primitive

**Files:**

- Create: `apps/frontend/src/components/ui/panel.tsx`
- Create: `apps/frontend/src/components/ui/panel.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `apps/frontend/src/components/ui/panel.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { Panel } from "./panel";

test("renders title and children", () => {
  render(
    <Panel title="Service health">
      <div>Latency summary</div>
    </Panel>
  );

  expect(screen.getByRole("heading", { name: "Service health" })).toBeInTheDocument();
  expect(screen.getByText("Latency summary")).toBeInTheDocument();
});

test("renders optional actions", () => {
  render(
    <Panel title="Services" actions={<button type="button">Refresh</button>}>
      Body
    </Panel>
  );

  expect(screen.getByRole("button", { name: "Refresh" })).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
npm --prefix apps/frontend run test -- src/components/ui/panel.test.tsx --run
```

Expected: FAIL because `./panel` does not exist.

- [ ] **Step 3: Implement the primitive**

Create `apps/frontend/src/components/ui/panel.tsx`:

```tsx
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

export interface PanelProps extends HTMLAttributes<HTMLElement> {
  title?: string;
  eyebrow?: string;
  actions?: ReactNode;
  children: ReactNode;
}

export function Panel({
  title,
  eyebrow,
  actions,
  children,
  className,
  ...props
}: PanelProps) {
  return (
    <section className={cn("modern-panel overflow-hidden", className)} {...props}>
      {(title || eyebrow || actions) && (
        <div className="modern-panel-header">
          <div>
            {eyebrow && <div className="field-label">{eyebrow}</div>}
            {title && <h2 className="m-0 text-lg font-bold text-[var(--text-strong)]">{title}</h2>}
          </div>
          {actions && <div className="modern-toolbar">{actions}</div>}
        </div>
      )}
      <div className="modern-panel-body">{children}</div>
    </section>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```bash
npm --prefix apps/frontend run test -- src/components/ui/panel.test.tsx --run
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/components/ui/panel.tsx apps/frontend/src/components/ui/panel.test.tsx
git commit -m "feat: add panel primitive"
```

---

### Task 4: Toolbar, MetricCard, And EmptyState Primitives

**Files:**

- Create: `apps/frontend/src/components/ui/toolbar.tsx`
- Create: `apps/frontend/src/components/ui/toolbar.test.tsx`
- Create: `apps/frontend/src/components/ui/metric-card.tsx`
- Create: `apps/frontend/src/components/ui/metric-card.test.tsx`
- Create: `apps/frontend/src/components/ui/empty-state.tsx`
- Create: `apps/frontend/src/components/ui/empty-state.test.tsx`

- [ ] **Step 1: Write failing tests**

Create `apps/frontend/src/components/ui/toolbar.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { Toolbar } from "./toolbar";

test("renders as an accessible toolbar", () => {
  render(
    <Toolbar aria-label="Service filters">
      <button type="button">Refresh</button>
    </Toolbar>
  );

  expect(screen.getByRole("toolbar", { name: "Service filters" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Refresh" })).toBeInTheDocument();
});
```

Create `apps/frontend/src/components/ui/metric-card.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { MetricCard } from "./metric-card";

test("renders metric label and value", () => {
  render(<MetricCard label="Avg P95" value="184ms" tone="good" />);
  expect(screen.getByText("Avg P95")).toBeInTheDocument();
  expect(screen.getByText("184ms")).toBeInTheDocument();
});
```

Create `apps/frontend/src/components/ui/empty-state.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { EmptyState } from "./empty-state";

test("renders title, description, metadata, and actions", () => {
  render(
    <EmptyState
      title="No services found"
      description="Adjust filters or send telemetry."
      metadata={["Tenant: local-dev", "Range: Last 1h"]}
      actions={<button type="button">Open setup</button>}
    />
  );

  expect(screen.getByRole("heading", { name: "No services found" })).toBeInTheDocument();
  expect(screen.getByText("Adjust filters or send telemetry.")).toBeInTheDocument();
  expect(screen.getByText("Tenant: local-dev")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Open setup" })).toBeInTheDocument();
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
npm --prefix apps/frontend run test -- src/components/ui/toolbar.test.tsx src/components/ui/metric-card.test.tsx src/components/ui/empty-state.test.tsx --run
```

Expected: FAIL because the modules do not exist.

- [ ] **Step 3: Implement `Toolbar`**

Create `apps/frontend/src/components/ui/toolbar.tsx`:

```tsx
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

export interface ToolbarProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

export function Toolbar({ className, children, ...props }: ToolbarProps) {
  return (
    <div role="toolbar" className={cn("modern-toolbar", className)} {...props}>
      {children}
    </div>
  );
}
```

- [ ] **Step 4: Implement `MetricCard`**

Create `apps/frontend/src/components/ui/metric-card.tsx`:

```tsx
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

type MetricTone = "good" | "warn" | "bad" | "info";

export interface MetricCardProps extends HTMLAttributes<HTMLDivElement> {
  label: string;
  value: ReactNode;
  tone?: MetricTone;
}

const toneClasses: Record<MetricTone, string> = {
  good: "border-t-[var(--good)]",
  warn: "border-t-[var(--warn)]",
  bad: "border-t-[var(--bad)]",
  info: "border-t-[var(--brand)]",
};

export function MetricCard({
  label,
  value,
  tone = "info",
  className,
  ...props
}: MetricCardProps) {
  return (
    <div
      className={cn(
        "modern-panel border-t-[3px] p-3",
        toneClasses[tone],
        className
      )}
      {...props}
    >
      <div className="metric-label">{label}</div>
      <div className="metric-value">{value}</div>
    </div>
  );
}
```

- [ ] **Step 5: Implement `EmptyState`**

Create `apps/frontend/src/components/ui/empty-state.tsx`:

```tsx
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

export interface EmptyStateProps extends HTMLAttributes<HTMLDivElement> {
  title: string;
  description?: string;
  metadata?: string[];
  actions?: ReactNode;
}

export function EmptyState({
  title,
  description,
  metadata = [],
  actions,
  className,
  ...props
}: EmptyStateProps) {
  return (
    <div
      className={cn(
        "modern-panel grid min-h-[240px] content-center justify-items-center gap-3 p-7 text-center",
        className
      )}
      {...props}
    >
      <h2 className="empty-title">{title}</h2>
      {description && <p className="m-0 max-w-xl text-sm text-[var(--muted)]">{description}</p>}
      {metadata.length > 0 && (
        <div className="empty-metrics">
          {metadata.map((item) => (
            <span key={item}>{item}</span>
          ))}
        </div>
      )}
      {actions && <div className="modern-toolbar justify-center">{actions}</div>}
    </div>
  );
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run:

```bash
npm --prefix apps/frontend run test -- src/components/ui/toolbar.test.tsx src/components/ui/metric-card.test.tsx src/components/ui/empty-state.test.tsx --run
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add apps/frontend/src/components/ui/toolbar.tsx apps/frontend/src/components/ui/toolbar.test.tsx apps/frontend/src/components/ui/metric-card.tsx apps/frontend/src/components/ui/metric-card.test.tsx apps/frontend/src/components/ui/empty-state.tsx apps/frontend/src/components/ui/empty-state.test.tsx
git commit -m "feat: add shared layout primitives"
```

---

### Task 5: Modernize App Shell

**Files:**

- Modify: `apps/frontend/src/components/AppShell.tsx`
- Modify: `apps/frontend/src/styles.css`

- [ ] **Step 1: Add nav metadata and icon glyphs**

In `AppShell.tsx`, replace `navItems` with:

```tsx
const navItems = [
  { label: "Setup", to: "/setup", icon: "S" },
  { label: "Services", to: "/services", icon: "Sv" },
  { label: "Traces", to: "/traces", icon: "Tr" },
  { label: "Logs", to: "/logs", icon: "Lg" },
  { label: "Infrastructure", to: "/infrastructure", icon: "In" },
  { label: "Service Overview", to: "/service-overview", icon: "Map" },
  { label: "Dashboards", to: "/dashboards", icon: "Db" },
  { label: "Alerts & SLOs", to: "/alerts", icon: "Al" },
  { label: "Admin / Fleet / Billing", to: "/admin", icon: "Ad" },
] as const;
```

Then render each link as:

```tsx
<Link
  key={item.to}
  to={item.to}
  className="nav-link"
  activeProps={{ className: "nav-link active" }}
>
  <span className="nav-icon" aria-hidden="true">
    {item.icon}
  </span>
  <span>{item.label}</span>
</Link>
```

- [ ] **Step 2: Improve shell styling**

In `styles.css`, update `.sidebar`, `.nav-link`, `.nav-link.active`, and `.topbar`:

```css
.sidebar {
  background: var(--surface);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 24px;
  padding: 20px;
  box-shadow: var(--shadow-control);
}

.nav-link {
  border-radius: 6px;
  padding: 9px 10px;
  color: var(--muted);
  font-weight: 650;
  display: grid;
  grid-template-columns: 28px minmax(0, 1fr);
  align-items: center;
  gap: 8px;
}

.nav-icon {
  width: 26px;
  height: 26px;
  border-radius: 6px;
  background: var(--surface-subtle);
  color: var(--muted);
  display: grid;
  place-items: center;
  font-size: 11px;
  font-weight: 800;
}

.nav-link.active .nav-icon {
  background: var(--brand-bg);
  color: var(--brand-strong);
}

.nav-link.active {
  box-shadow: inset 3px 0 0 var(--brand);
}

.topbar {
  min-height: 72px;
  background: color-mix(in srgb, var(--surface) 92%, var(--bg));
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 24px;
}
```

- [ ] **Step 3: Run frontend checks**

Run:

```bash
npm --prefix apps/frontend run typecheck
npm --prefix apps/frontend run test -- src/App.test.tsx --run
```

Expected: both PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/components/AppShell.tsx apps/frontend/src/styles.css
git commit -m "feat: modernize app shell"
```

---

### Task 6: Migrate Services Page To Shared Primitives

**Files:**

- Modify: `apps/frontend/src/pages/ProductAreaPage.tsx`
- Modify or create focused tests if the existing page tests do not cover services rendering

- [ ] **Step 1: Add primitive imports**

At the top of `ProductAreaPage.tsx`, add:

```tsx
import { Badge } from "../components/ui/badge";
import { EmptyState } from "../components/ui/empty-state";
import { MetricCard } from "../components/ui/metric-card";
import { Panel } from "../components/ui/panel";
import { Toolbar } from "../components/ui/toolbar";
```

- [ ] **Step 2: Replace filter wrapper**

Replace:

```tsx
<div className="toolbar-row">
```

with:

```tsx
<Toolbar aria-label="Service filters">
```

and replace the closing `</div>` with `</Toolbar>`.

- [ ] **Step 3: Replace local metric tiles**

Replace `MetricTile` usages in the Services area with `MetricCard`:

```tsx
<MetricCard label="Services" value={String(stats.count)} tone="info" />
<MetricCard
  label="Active Alerts"
  value={String(servicesData?.items.reduce((acc, s) => acc + s.active_alert_count, 0) ?? 0)}
  tone="warn"
/>
<MetricCard label="Avg P95" value={`${Math.round(stats.avgP95)}ms`} tone="good" />
<MetricCard
  label="Avg Error Rate"
  value={(stats.avgError * 100).toFixed(2) + "%"}
  tone={stats.avgError > 0.01 ? "warn" : "good"}
/>
```

- [ ] **Step 4: Wrap the table in `Panel`**

Replace:

```tsx
<div className="table-panel">
```

with:

```tsx
<Panel title="Service catalog" eyebrow="Health and performance">
```

Replace the closing table-panel `</div>` with `</Panel>`.

Add `className="modern-table-row"` to each table body row:

```tsx
<tr key={row.service_name} className="modern-table-row">
```

- [ ] **Step 5: Replace health status implementation**

Replace `HealthStatus` with:

```tsx
function HealthStatus({ healthState }: { healthState: ServiceSummary["health_state"] }) {
  if (healthState === "breach") return <Badge tone="bad">Breach</Badge>;
  if (healthState === "watch") return <Badge tone="warn">Watch</Badge>;
  return <Badge tone="good">Healthy</Badge>;
}
```

- [ ] **Step 6: Replace placeholder product area empty panels**

For non-services product areas, replace the existing `<div className="empty-panel">...</div>` block with:

```tsx
<EmptyState
  title={copy.title}
  description="This workspace will use the same dense operational layout as the service catalog."
  metadata={["Tenant: local-dev", `Environment: ${environment}`, "Range: Last 1h"]}
/>
```

- [ ] **Step 7: Remove local `MetricTile` helper**

Delete the local `MetricTile` function at the bottom of `ProductAreaPage.tsx` after all usages have been replaced by `MetricCard`.

- [ ] **Step 8: Run frontend checks**

Run:

```bash
npm --prefix apps/frontend run typecheck
npm --prefix apps/frontend run test -- src/App.test.tsx --run
```

Expected: both PASS.

- [ ] **Step 9: Commit**

```bash
git add apps/frontend/src/pages/ProductAreaPage.tsx
git commit -m "feat: migrate services page to modern primitives"
```

---

### Task 7: Add Services Accessibility Coverage

**Files:**

- Modify: `apps/frontend/e2e/accessibility.spec.ts`

- [ ] **Step 1: Add fixtures**

Add these constants near the existing fixtures:

```ts
const FIXTURE_ENVIRONMENTS = {
  items: ["local-dev", "prod"],
};

const FIXTURE_SERVICES = {
  items: [
    {
      service_name: "checkout",
      environment: "local-dev",
      request_rate: 12.4,
      error_rate: 0.004,
      p95_latency_ms: 184,
      health_state: "healthy",
      active_alert_count: 0,
      last_deployment_at: null,
    },
    {
      service_name: "payments",
      environment: "local-dev",
      request_rate: 4.9,
      error_rate: 0.021,
      p95_latency_ms: 312,
      health_state: "watch",
      active_alert_count: 1,
      last_deployment_at: null,
    },
  ],
};
```

- [ ] **Step 2: Add the axe test**

Add this describe block:

```ts
test.describe("services catalog", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/v1/environments**", (route) =>
      route.fulfill({ json: FIXTURE_ENVIRONMENTS })
    );
    await page.route("**/v1/services**", (route) =>
      route.fulfill({ json: FIXTURE_SERVICES })
    );
  });

  test("has no axe violations", async ({ page }) => {
    await page.goto("/services");
    await page.waitForSelector("text=checkout");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});
```

- [ ] **Step 3: Run the accessibility test**

Run:

```bash
npm --prefix apps/frontend run test:a11y -- e2e/accessibility.spec.ts
```

Expected: PASS, including the new Services catalog axe test.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/e2e/accessibility.spec.ts
git commit -m "test: add services accessibility coverage"
```

---

### Task 8: Update UI Design Guide

**Files:**

- Modify: `docs/superpowers/specs/2026-04-21-ui-design-guide.md`

- [ ] **Step 1: Document new tokens**

In the "Design Tokens" section, add a subsection:

```markdown
### 1.3 Modern Surface Tokens

| Token | Purpose |
|---|---|
| `--surface-raised` | Panels and elevated work surfaces |
| `--surface-inset` | Recessed code, logs, and dense detail regions |
| `--border-strong` | Strong separators and selected boundaries |
| `--text-strong` | Highest-emphasis headings and identifiers |
| `--shadow-panel` | Floating panels and future popovers |
| `--shadow-control` | Subtle depth for persistent shell and panels |
| `--brand-bg` | Informational selected states and icon backgrounds |
```

- [ ] **Step 2: Document the new primitives**

In the "Components" section, add:

```markdown
### 5.9 Modern Shared Primitives

The modern UI foundation owns these reusable primitives under `apps/frontend/src/components/ui/`:

| Primitive | Responsibility |
|---|---|
| `Badge` | Semantic status and neutral metadata labels |
| `Panel` | Bordered work surfaces with optional eyebrow, title, and action area |
| `Toolbar` | Responsive filter/action rows with toolbar semantics |
| `MetricCard` | Compact KPI tiles using the existing health color semantics |
| `EmptyState` | Consistent no-data and not-yet-built states with metadata and actions |

New product pages should consume these primitives before adding page-local equivalents.
```

- [ ] **Step 3: State ADR/spec sync**

Add this note near the end of the document:

```markdown
No ADR update is required for the modern UI foundation slice because it preserves the approved frontend stack, theme model, and service-centric navigation architecture.
```

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-04-21-ui-design-guide.md
git commit -m "docs: document modern ui primitives"
```

---

### Task 9: Final Verification And PR

**Files:**

- Verify all files changed by this plan

- [ ] **Step 1: Run focused frontend verification**

Run:

```bash
npm --prefix apps/frontend run typecheck
npm --prefix apps/frontend run lint
npm --prefix apps/frontend run test -- --run
npm --prefix apps/frontend run build
npm --prefix apps/frontend run test:a11y -- e2e/accessibility.spec.ts
```

Expected: all PASS.

- [ ] **Step 2: Run mandatory local CI before push**

Because this implementation plan changes frontend code, run:

```bash
bash scripts/local-ci.sh
```

Expected: PASS.

If Docker is unavailable, use:

```bash
bash scripts/local-ci.sh --skip-docker --skip-smoke
```

The PR body must state the skipped stages and why.

- [ ] **Step 3: Push the branch**

Run:

```bash
git push -u origin codex/modern-ui-foundation
```

- [ ] **Step 4: Open a draft PR**

PR title:

```text
[codex] modernize frontend UI foundation
```

PR body:

```markdown
## Summary

- adds modern surface/elevation tokens while preserving the existing theme contract
- adds shared Badge, Panel, Toolbar, MetricCard, and EmptyState primitives
- modernizes the app shell and migrates the Services page as the first proof point
- adds Services catalog accessibility coverage
- updates the UI design guide

## Verification

- npm --prefix apps/frontend run typecheck
- npm --prefix apps/frontend run lint
- npm --prefix apps/frontend run test -- --run
- npm --prefix apps/frontend run build
- npm --prefix apps/frontend run test:a11y -- e2e/accessibility.spec.ts
- bash scripts/local-ci.sh

## ADR / Spec Sync

No ADR update is required because this preserves the approved React/Vite, Base UI, Tailwind CSS v4, theme, and service-centric navigation architecture. The UI design guide is updated with the new primitives and tokens.
```

---

## Plan Self-Review

- Spec coverage: This plan maps to `spec/05-frontend.md` design system, accessibility, theme, and information-density requirements, and to the existing UI design guide.
- Placeholder scan: No placeholder task remains. Each implementation task names exact files, concrete code, commands, and expected outcomes.
- Type consistency: Component names and prop names are consistent across tests, implementations, and migration steps.
- Scope check: This is a single reviewable foundation slice. Later screen migrations should be separate PRs.
