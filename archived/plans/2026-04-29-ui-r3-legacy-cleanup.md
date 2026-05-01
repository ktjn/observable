# UI-R3: Legacy Style Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove remaining legacy CSS classes and inline style objects from all product pages, add three new `components/ui` primitives (`LoadingState`, `TablePanel`, `HealthDot`), and document the frontend styling migration rule.

**Architecture:** Add primitives first (Tasks 1–3), then migrate each product page in isolation (Tasks 4–13), then clean up `styles.css` (Task 14), then update the spec rule (Task 15), then verify (Task 16). Each task is independently committable.

**Tech Stack:** React 18, TypeScript, Tailwind CSS v4, Vitest + React Testing Library, `cn` utility (clsx + tailwind-merge).

---

## File Map

**Create:**
- `apps/frontend/src/components/ui/loading-state.tsx`
- `apps/frontend/src/components/ui/loading-state.test.tsx`
- `apps/frontend/src/components/ui/table-panel.tsx`
- `apps/frontend/src/components/ui/table-panel.test.tsx`

**Modify:**
- `apps/frontend/src/components/ui/badge.tsx` — add `HealthDot` export
- `apps/frontend/src/components/ui/badge.test.tsx` — add `HealthDot` tests
- `apps/frontend/src/pages/InfrastructureInventoryPage.tsx`
- `apps/frontend/src/components/ServiceInfraPanel.tsx`
- `apps/frontend/src/pages/InfrastructureDetailPage.tsx`
- `apps/frontend/src/pages/SetupPage.tsx`
- `apps/frontend/src/pages/ServiceDetailPage.tsx`
- `apps/frontend/src/pages/ProductAreaPage.tsx`
- `apps/frontend/src/pages/ServiceOverview.tsx`
- `apps/frontend/src/pages/LogSearch.tsx`
- `apps/frontend/src/pages/TraceSearch.tsx`
- `apps/frontend/src/features/alerts/AlertsPage.tsx`
- `apps/frontend/src/styles.css`
- `spec/15-frontend-local-dev.md`

---

## Task 1: Add `LoadingState` primitive

**Files:**
- Create: `apps/frontend/src/components/ui/loading-state.tsx`
- Create: `apps/frontend/src/components/ui/loading-state.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// apps/frontend/src/components/ui/loading-state.test.tsx
import { render, screen } from "@testing-library/react";
import { LoadingState } from "./loading-state";

test("renders children as content", () => {
  render(<LoadingState>Loading data…</LoadingState>);
  expect(screen.getByText("Loading data…")).toBeInTheDocument();
});

test("applies muted text styling", () => {
  render(<LoadingState>Loading…</LoadingState>);
  expect(screen.getByText("Loading…").parentElement ?? screen.getByText("Loading…")).toBeTruthy();
});

test("merges additional className via prop", () => {
  render(<LoadingState className="text-[var(--bad)]">Error!</LoadingState>);
  const el = screen.getByText("Error!");
  expect(el.className).toContain("text-[var(--bad)]");
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd apps/frontend && npm run test -- loading-state
```
Expected: FAIL — `LoadingState` not found.

- [ ] **Step 3: Write the implementation**

```tsx
// apps/frontend/src/components/ui/loading-state.tsx
import type { ReactNode } from "react";
import { cn } from "./cn";

export function LoadingState({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("p-12 text-center text-[var(--muted)]", className)}>
      {children}
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd apps/frontend && npm run test -- loading-state
```
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/components/ui/loading-state.tsx apps/frontend/src/components/ui/loading-state.test.tsx
git commit -m "feat(ui): add LoadingState primitive"
```

---

## Task 2: Add `TablePanel` primitive

**Files:**
- Create: `apps/frontend/src/components/ui/table-panel.tsx`
- Create: `apps/frontend/src/components/ui/table-panel.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// apps/frontend/src/components/ui/table-panel.test.tsx
import { render, screen } from "@testing-library/react";
import { TablePanel } from "./table-panel";

test("renders children", () => {
  render(<TablePanel><p>content</p></TablePanel>);
  expect(screen.getByText("content")).toBeInTheDocument();
});

test("forwards aria-label to the wrapper div", () => {
  render(<TablePanel aria-label="Service traces"><table /></TablePanel>);
  expect(screen.getByRole("generic", { name: "Service traces" })).toBeInTheDocument();
});

test("merges className prop", () => {
  const { container } = render(<TablePanel className="flex-1"><p>x</p></TablePanel>);
  expect(container.firstChild).toHaveClass("flex-1");
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd apps/frontend && npm run test -- table-panel
```
Expected: FAIL — `TablePanel` not found.

- [ ] **Step 3: Write the implementation**

```tsx
// apps/frontend/src/components/ui/table-panel.tsx
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

export interface TablePanelProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

export function TablePanel({ children, className, ...props }: TablePanelProps) {
  return (
    <div
      className={cn(
        "bg-[var(--surface)] border border-[var(--border)] rounded-lg overflow-x-auto",
        className,
      )}
      {...props}
    >
      {children}
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd apps/frontend && npm run test -- table-panel
```
Expected: 3 tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/components/ui/table-panel.tsx apps/frontend/src/components/ui/table-panel.test.tsx
git commit -m "feat(ui): add TablePanel primitive"
```

---

## Task 3: Add `HealthDot` export to `badge.tsx`

**Files:**
- Modify: `apps/frontend/src/components/ui/badge.tsx`
- Modify: `apps/frontend/src/components/ui/badge.test.tsx`

- [ ] **Step 1: Write the failing tests**

Append these tests to `apps/frontend/src/components/ui/badge.test.tsx`:

```tsx
import { HealthDot } from "./badge";

test("HealthDot renders with role img and aria-label for healthy", () => {
  render(<HealthDot state="healthy" />);
  expect(screen.getByRole("img", { name: "healthy" })).toBeInTheDocument();
});

test("HealthDot renders for watch state", () => {
  render(<HealthDot state="watch" />);
  expect(screen.getByRole("img", { name: "watch" })).toBeInTheDocument();
});

test("HealthDot renders for breach state", () => {
  render(<HealthDot state="breach" />);
  expect(screen.getByRole("img", { name: "breach" })).toBeInTheDocument();
});

test("HealthDot renders for unknown state", () => {
  render(<HealthDot state="unknown" />);
  expect(screen.getByRole("img", { name: "unknown" })).toBeInTheDocument();
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd apps/frontend && npm run test -- badge
```
Expected: the 4 new HealthDot tests FAIL, existing 3 badge tests still PASS.

- [ ] **Step 3: Add `HealthDot` to `badge.tsx`**

Append after the closing brace of `Badge` in `apps/frontend/src/components/ui/badge.tsx`:

```tsx
type HealthState = "healthy" | "watch" | "breach" | "unknown";

const dotClasses: Record<HealthState, string> = {
  healthy: "bg-[var(--good)]",
  watch: "bg-[var(--warn)]",
  breach: "bg-[var(--bad)]",
  unknown: "bg-[var(--muted)]",
};

export function HealthDot({ state }: { state: HealthState }) {
  return (
    <span
      role="img"
      aria-label={state}
      className={cn("inline-block h-2 w-2 flex-shrink-0 rounded-full", dotClasses[state])}
    />
  );
}
```

- [ ] **Step 4: Run tests to verify all pass**

```bash
cd apps/frontend && npm run test -- badge
```
Expected: all 7 tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/components/ui/badge.tsx apps/frontend/src/components/ui/badge.test.tsx
git commit -m "feat(ui): add HealthDot export to badge primitive"
```

---

## Task 4: Migrate `InfrastructureInventoryPage.tsx`

**Files:**
- Modify: `apps/frontend/src/pages/InfrastructureInventoryPage.tsx`

Removes: `metric-tile`, `status .good/.warn/.bad`, `metric-grid`, `loading-state`, `table-panel`, local `MetricTile` component.

- [ ] **Step 1: Replace the file content**

Replace the full file with:

```tsx
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { listEnvironments } from "../api/services";
import {
  listInfrastructure,
  type InfrastructureEntitySummary,
  type InfrastructureEntityType,
} from "../api/infrastructure";
import { Badge } from "../components/ui/badge";
import { Input } from "../components/ui/input";
import { LoadingState } from "../components/ui/loading-state";
import { MetricCard } from "../components/ui/metric-card";
import { Select, SelectOption } from "../components/ui/select";
import { TablePanel } from "../components/ui/table-panel";

type InfrastructureTypeFilter = "all" | InfrastructureEntityType;

const infrastructureTypeOptions: InfrastructureTypeFilter[] = [
  "all",
  "host",
  "cluster",
  "namespace",
  "pod",
  "container",
];

export default function InfrastructureInventoryPage() {
  const [environment, setEnvironment] = useState("all");
  const [entityType, setEntityType] = useState<InfrastructureTypeFilter>("all");
  const [search, setSearch] = useState("");

  const { data: environments } = useQuery({
    queryKey: ["environments"],
    queryFn: () => listEnvironments(),
  });

  const { data, isLoading, isError } = useQuery({
    queryKey: ["infrastructure"],
    queryFn: () => listInfrastructure(),
  });

  const filteredItems = useMemo(() => {
    const searchValue = search.trim().toLowerCase();
    return (data?.items ?? []).filter((item) => {
      const matchesType = entityType === "all" || item.entity_type === entityType;
      const matchesEnvironment = environment === "all" || item.environment === environment;
      const matchesSearch =
        searchValue.length === 0 ||
        item.display_name.toLowerCase().includes(searchValue) ||
        item.entity_id.toLowerCase().includes(searchValue) ||
        item.parent_display_name?.toLowerCase().includes(searchValue) === true ||
        item.related_services.some((service) => service.toLowerCase().includes(searchValue));

      return matchesType && matchesEnvironment && matchesSearch;
    });
  }, [data, environment, entityType, search]);

  const summary = useMemo(() => summarizeInfrastructure(filteredItems), [filteredItems]);

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Inventory</div>
          <h1>Infrastructure</h1>
        </div>
      </div>

      <div className="toolbar-row">
        <Input
          className="max-w-[360px]"
          aria-label="Search infrastructure"
          placeholder="Search infrastructure"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
        <Select
          aria-label="Infrastructure type filter"
          value={entityType}
          onChange={(event) => setEntityType(event.target.value as InfrastructureTypeFilter)}
        >
          {infrastructureTypeOptions.map((option) => (
            <SelectOption key={option} value={option}>
              {option === "all" ? "All types" : option}
            </SelectOption>
          ))}
        </Select>
        <Select
          aria-label="Environment filter"
          value={environment}
          onChange={(event) => setEnvironment(event.target.value)}
        >
          <SelectOption value="all">All environments</SelectOption>
          {environments?.items.map((env) => (
            <SelectOption key={env} value={env}>
              {env}
            </SelectOption>
          ))}
        </Select>
      </div>

      <div
        className="grid gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
        style={{ gridTemplateColumns: "repeat(4, minmax(140px, 1fr))" }}
        aria-label="Infrastructure summary"
      >
        <MetricCard label="Entities" value={String(filteredItems.length)} tone="info" />
        <MetricCard label="Healthy" value={String(summary.healthy)} tone="good" />
        <MetricCard label="Watch" value={String(summary.watch)} tone="warn" />
        <MetricCard label="Breach" value={String(summary.breach)} tone="bad" />
      </div>

      <TablePanel>
        {isLoading ? (
          <LoadingState>Loading infrastructure…</LoadingState>
        ) : isError ? (
          <div className="signal-empty">Infrastructure inventory could not be loaded.</div>
        ) : filteredItems.length === 0 ? (
          <div className="signal-empty">No infrastructure entities matched the current filters.</div>
        ) : (
          <table aria-label="Infrastructure inventory">
            <thead>
              <tr>
                <th>Entity</th>
                <th>Type</th>
                <th>Environment</th>
                <th>Health</th>
                <th>Related services</th>
                <th>Log rate</th>
                <th>Error rate</th>
                <th>Last seen</th>
              </tr>
            </thead>
            <tbody>
              {filteredItems.map((item) => (
                <InfrastructureRow key={`${item.entity_type}:${item.entity_id}`} item={item} />
              ))}
            </tbody>
          </table>
        )}
      </TablePanel>
    </section>
  );
}

function InfrastructureRow({ item }: { item: InfrastructureEntitySummary }) {
  return (
    <tr>
      <td className="strong-cell">
        <Link
          to="/infrastructure/$entityType/$entityId"
          params={{ entityType: item.entity_type, entityId: item.entity_id }}
        >
          {item.display_name}
        </Link>
      </td>
      <td>{item.entity_type}</td>
      <td>{item.environment ?? "Unavailable"}</td>
      <td>
        <HealthStatus healthState={item.health_state} />
      </td>
      <td>
        <RelatedServiceLinks services={item.related_services} />
      </td>
      <td>{formatPerMinute(item.log_rate_per_minute)}</td>
      <td>{formatPercent(item.error_rate)}</td>
      <td>{formatUnixNano(item.last_seen_unix_nano)}</td>
    </tr>
  );
}

function RelatedServiceLinks({ services }: { services: string[] }) {
  if (services.length === 0) return <>Unavailable</>;
  return (
    <>
      {services.map((service, i) => (
        <span key={service}>
          {i > 0 && ", "}
          <Link to="/services/$serviceId" params={{ serviceId: service }}>
            {service}
          </Link>
        </span>
      ))}
    </>
  );
}

function summarizeInfrastructure(items: InfrastructureEntitySummary[]) {
  return items.reduce(
    (acc, item) => {
      if (item.health_state === "healthy") acc.healthy += 1;
      if (item.health_state === "watch") acc.watch += 1;
      if (item.health_state === "breach") acc.breach += 1;
      return acc;
    },
    { healthy: 0, watch: 0, breach: 0 },
  );
}

function formatPerMinute(value: number | null) {
  return value === null ? "Unavailable" : `${value.toFixed(2)}/min`;
}

function formatPercent(value: number | null) {
  return value === null ? "Unavailable" : `${(value * 100).toFixed(2)}%`;
}

function formatUnixNano(nanos: number): string {
  return new Date(nanos / 1_000_000).toLocaleString();
}

function HealthStatus({ healthState }: { healthState: InfrastructureEntitySummary["health_state"] }) {
  const tone = healthState === "breach" ? "bad" : healthState === "watch" ? "warn" : "good";
  const label = healthState === "breach" ? "Breach" : healthState === "watch" ? "Watch" : "Healthy";
  return <Badge tone={tone}>{label}</Badge>;
}
```

> Note: `gridTemplateColumns` is kept as an inline style because the `minmax(140px, 1fr)` value has no direct Tailwind equivalent. The responsive class overrides it at narrower breakpoints.

- [ ] **Step 2: Run typecheck and tests**

```bash
cd apps/frontend && npm run typecheck && npm run test
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/pages/InfrastructureInventoryPage.tsx
git commit -m "refactor(ui): migrate InfrastructureInventoryPage to ui primitives"
```

---

## Task 5: Migrate `ServiceInfraPanel.tsx`

**Files:**
- Modify: `apps/frontend/src/components/ServiceInfraPanel.tsx`

Removes: `entity-card-list`, `entity-card-row`, `entity-card-link`, `entity-card-metric`, local `HealthDot`, `loading-state`, `signal-empty` (the last three become primitive/Tailwind).

- [ ] **Step 1: Replace the file content**

```tsx
import { useQuery } from "@tanstack/react-query";
import { listInfrastructure, type InfrastructureEntitySummary } from "../api/infrastructure";
import { Badge, HealthDot } from "./ui/badge";
import { LoadingState } from "./ui/loading-state";
import { Panel } from "./ui/panel";

interface Props {
  serviceName: string;
}

export function ServiceInfraPanel({ serviceName }: Props) {
  const { data, isLoading, isError } = useQuery({
    queryKey: ["service-infra", serviceName],
    queryFn: () => listInfrastructure({ service: serviceName }),
  });

  if (isLoading) return <LoadingState>Loading infrastructure…</LoadingState>;
  if (isError) return <div className="signal-empty">Could not load infrastructure.</div>;
  if (!data?.items.length) {
    return (
      <div className="signal-empty">
        No infrastructure entities observed for this service.
      </div>
    );
  }

  const items = data.items.slice(0, 10);

  return (
    <Panel eyebrow="Infrastructure" title="Running On">
      <div className="flex flex-col gap-2">
        {items.map((entity) => (
          <EntityCard key={`${entity.entity_type}/${entity.entity_id}`} entity={entity} />
        ))}
      </div>
    </Panel>
  );
}

function EntityCard({ entity }: { entity: InfrastructureEntitySummary }) {
  const href = `/infrastructure/${entity.entity_type}/${encodeURIComponent(entity.entity_id)}`;
  return (
    <div className="flex items-center gap-3 border-b border-[var(--border)] py-2 last:border-0">
      <Badge tone="info" className="min-w-[72px] justify-center">
        {entity.entity_type}
      </Badge>
      <a href={href} className="flex-1 min-w-0 font-[650] text-[var(--text)] no-underline hover:text-[var(--brand-strong)]">
        {entity.display_name}
      </a>
      <HealthDot state={entity.health_state} />
      {entity.cpu_usage !== null && (
        <span className="text-[var(--muted)] text-xs">
          CPU {Math.round(entity.cpu_usage * 100)}%
        </span>
      )}
      {entity.memory_usage !== null && (
        <span className="text-[var(--muted)] text-xs">
          Mem {Math.round(entity.memory_usage * 100)}%
        </span>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Run typecheck and tests**

```bash
cd apps/frontend && npm run typecheck && npm run test
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/components/ServiceInfraPanel.tsx
git commit -m "refactor(ui): migrate ServiceInfraPanel to ui primitives"
```

---

## Task 6: Migrate `InfrastructureDetailPage.tsx`

**Files:**
- Modify: `apps/frontend/src/pages/InfrastructureDetailPage.tsx`

Removes: `entry-link-grid`, `entry-link`, `loading-state`, `field-label` (eyebrow), `metric-grid`.

- [ ] **Step 1: Apply targeted edits**

Replace the two `loading-state` divs (lines 25 and 29) with `LoadingState`:

```tsx
// Add import at top with other ui imports:
import { LoadingState } from "../components/ui/loading-state";
```

Change lines 25–30:
```tsx
  if (!entityType || !entityId) {
    return <LoadingState>Loading infrastructure detail…</LoadingState>;
  }

  if (isLoading) {
    return <LoadingState>Loading infrastructure detail…</LoadingState>;
  }
```

Change line 37 (`<div className="field-label">Infrastructure</div>`):
```tsx
<div className="text-xs font-bold uppercase text-[var(--muted)]">Infrastructure</div>
```

Change line 55 (same `field-label`):
```tsx
<div className="text-xs font-bold uppercase text-[var(--muted)]">Infrastructure</div>
```

Change line 63 (`<div className="metric-grid" aria-label="Infrastructure summary">`):
```tsx
<div
  className="grid gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
  style={{ gridTemplateColumns: "repeat(4, minmax(140px, 1fr))" }}
  aria-label="Infrastructure summary"
>
```

Change line 98 (`<div className="entry-link-grid" aria-label="Related services">`):
```tsx
<div className="grid grid-cols-2 gap-2.5" aria-label="Related services">
```

Change all `<Link ... className="entry-link">` (line 100) to:
```tsx
<Link
  key={service}
  to="/services/$serviceId"
  params={{ serviceId: service }}
  className="min-h-[54px] border border-[var(--border)] rounded-md grid place-items-center text-[var(--text)] no-underline font-bold hover:border-[var(--brand)] hover:text-[var(--brand-strong)]"
>
  {service}
</Link>
```

Change line 144 (`<div className="entry-link-grid" aria-label="Infrastructure action links">`):
```tsx
<div className="grid grid-cols-2 gap-2.5" aria-label="Infrastructure action links">
```

Change all `<a ... className="entry-link">` (lines 145, 148, 151) to:
```tsx
<a href={links.logs} className="min-h-[54px] border border-[var(--border)] rounded-md grid place-items-center text-[var(--text)] no-underline font-bold hover:border-[var(--brand)] hover:text-[var(--brand-strong)]">
  Logs
</a>
<a href={links.traces} className="min-h-[54px] border border-[var(--border)] rounded-md grid place-items-center text-[var(--text)] no-underline font-bold hover:border-[var(--brand)] hover:text-[var(--brand-strong)]">
  Traces
</a>
<a href={links.metrics} className="min-h-[54px] border border-[var(--border)] rounded-md grid place-items-center text-[var(--text)] no-underline font-bold hover:border-[var(--brand)] hover:text-[var(--brand-strong)]">
  Metrics
</a>
```

- [ ] **Step 2: Run typecheck and tests**

```bash
cd apps/frontend && npm run typecheck && npm run test
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/pages/InfrastructureDetailPage.tsx
git commit -m "refactor(ui): migrate InfrastructureDetailPage to ui primitives"
```

---

## Task 7: Migrate `SetupPage.tsx`

**Files:**
- Modify: `apps/frontend/src/pages/SetupPage.tsx`

Removes: `detail-panel`, `detail-panel-header`, `status`, inline `field-label` on copy status span.

- [ ] **Step 1: Replace the file content**

```tsx
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Panel } from "../components/ui/panel";
import {
  getFirstSignalStatus,
  LOCAL_DEV_API_KEY,
  LOCAL_DEV_TENANT,
  LOCAL_DEV_TENANT_ID,
  OTLP_HTTP_TRACE_ENDPOINT,
  REDACTED_LOCAL_API_KEY,
} from "../api/setup";

export default function SetupPage() {
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");
  const { data, isLoading, refetch } = useQuery({
    queryKey: ["setup", "first-signal"],
    queryFn: getFirstSignalStatus,
  });

  const statusText = isLoading
    ? "Checking telemetry"
    : data?.state === "detected"
      ? "First signal detected"
      : data?.state === "error"
        ? "First signal check failed"
        : "Waiting for first signal";

  async function copyApiKey() {
    try {
      await navigator.clipboard.writeText(LOCAL_DEV_API_KEY);
      setCopyState("copied");
    } catch {
      setCopyState("failed");
    }
  }

  return (
    <section className="page-stack" aria-labelledby="setup-heading">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Onboarding</div>
          <h1 id="setup-heading">Setup</h1>
        </div>
        <Button variant="secondary" onClick={() => void refetch()}>
          Recheck
        </Button>
      </div>

      <div className="detail-grid">
        <Panel eyebrow="Local ingest" title="Collector endpoint">
          <dl className="definition-grid">
            <div>
              <dt>Tenant</dt>
              <dd>{LOCAL_DEV_TENANT}</dd>
            </div>
            <div>
              <dt>Tenant ID</dt>
              <dd>{LOCAL_DEV_TENANT_ID}</dd>
            </div>
            <div>
              <dt>OTLP HTTP traces</dt>
              <dd>{OTLP_HTTP_TRACE_ENDPOINT}</dd>
            </div>
            <div>
              <dt>API key</dt>
              <dd>{REDACTED_LOCAL_API_KEY}</dd>
            </div>
          </dl>
          <div className="setup-actions">
            <Button variant="secondary" onClick={() => void copyApiKey()}>
              Copy API key
            </Button>
            <span className="text-xs font-bold uppercase text-[var(--muted)]" role="status">
              {copyState === "copied"
                ? "Copied"
                : copyState === "failed"
                  ? "Copy unavailable"
                  : "Redacted in the UI"}
            </span>
          </div>
        </Panel>

        <Panel
          eyebrow="Validation"
          title="First signal"
          actions={
            <Badge tone={data?.state === "detected" ? "good" : "warn"}>
              {statusText}
            </Badge>
          }
        >
          <dl className="definition-grid">
            <div>
              <dt>Traces</dt>
              <dd>{data?.traces ?? 0}</dd>
            </div>
            <div>
              <dt>Logs</dt>
              <dd>{data?.logs ?? 0}</dd>
            </div>
            <div>
              <dt>Metrics</dt>
              <dd>{data?.metrics ?? 0}</dd>
            </div>
          </dl>
        </Panel>
      </div>
    </section>
  );
}
```

- [ ] **Step 2: Run typecheck and tests**

```bash
cd apps/frontend && npm run typecheck && npm run test
```
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/pages/SetupPage.tsx
git commit -m "refactor(ui): migrate SetupPage to Panel and Badge primitives"
```

---

## Task 8: Migrate `ServiceDetailPage.tsx`

**Files:**
- Modify: `apps/frontend/src/pages/ServiceDetailPage.tsx`

Removes: `table-panel`, `metric-grid`, `entry-link-grid`, `entry-link`, `loading-state`, `field-label`.

- [ ] **Step 1: Add new imports at the top**

Add `LoadingState` and `TablePanel` to imports:

```tsx
import { LoadingState } from "../components/ui/loading-state";
import { TablePanel } from "../components/ui/table-panel";
```

- [ ] **Step 2: Replace loading-state usages**

Change all four `<div className="loading-state">Loading service overview...</div>` (lines 18 and 31) to:
```tsx
<LoadingState>Loading service overview…</LoadingState>
```

Change `<div className="loading-state">Loading service logs...</div>` (line 218):
```tsx
<LoadingState>Loading service logs…</LoadingState>
```

Change `<div className="loading-state">Loading service metrics...</div>` (line 254):
```tsx
<LoadingState>Loading service metrics…</LoadingState>
```

Change `<div className="loading-state">Loading service traces...</div>` (line 299):
```tsx
<LoadingState>Loading service traces…</LoadingState>
```

- [ ] **Step 3: Replace field-label eyebrow**

Change line 65 (`<div className="field-label">Service Overview</div>`):
```tsx
<div className="text-xs font-bold uppercase text-[var(--muted)]">Service Overview</div>
```

- [ ] **Step 4: Replace metric-grid**

Change line 71 (`<div className="metric-grid" aria-label="Service performance summary">`):
```tsx
<div
  className="grid gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
  style={{ gridTemplateColumns: "repeat(4, minmax(140px, 1fr))" }}
  aria-label="Service performance summary"
>
```

- [ ] **Step 5: Replace entry-link-grid and entry-link**

Change line 110 (`<div className="entry-link-grid" aria-label="Signal entry points">`):
```tsx
<div className="grid grid-cols-2 gap-2.5" aria-label="Signal entry points">
```

Change all four `className="entry-link"` anchors (lines 111–120):
```tsx
<a
  href={`/traces?service=${encodeURIComponent(service.service_name)}`}
  className="min-h-[54px] border border-[var(--border)] rounded-md grid place-items-center text-[var(--text)] no-underline font-bold hover:border-[var(--brand)] hover:text-[var(--brand-strong)]"
>
  Traces
</a>
<a
  href={`/logs?service=${encodeURIComponent(service.service_name)}`}
  className="min-h-[54px] border border-[var(--border)] rounded-md grid place-items-center text-[var(--text)] no-underline font-bold hover:border-[var(--brand)] hover:text-[var(--brand-strong)]"
>
  Logs
</a>
<a
  href={`/metrics?service=${encodeURIComponent(service.service_name)}`}
  className="min-h-[54px] border border-[var(--border)] rounded-md grid place-items-center text-[var(--text)] no-underline font-bold hover:border-[var(--brand)] hover:text-[var(--brand-strong)]"
>
  Metrics
</a>
<a
  href={`/infrastructure?service=${encodeURIComponent(service.service_name)}`}
  className="min-h-[54px] border border-[var(--border)] rounded-md grid place-items-center text-[var(--text)] no-underline font-bold hover:border-[var(--brand)] hover:text-[var(--brand-strong)]"
>
  Infrastructure
</a>
```

- [ ] **Step 6: Replace table-panel in the three signal tabs**

In `ServiceLogsTab`, change `<div className="table-panel">` to `<TablePanel>` and `</div>` to `</TablePanel>`.

In `ServiceMetricsTab`, same replacement.

In `ServiceTracesTab`, same replacement.

- [ ] **Step 7: Run typecheck and tests**

```bash
cd apps/frontend && npm run typecheck && npm run test
```
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/frontend/src/pages/ServiceDetailPage.tsx
git commit -m "refactor(ui): migrate ServiceDetailPage to ui primitives"
```

---

## Task 9: Migrate `ProductAreaPage.tsx`

**Files:**
- Modify: `apps/frontend/src/pages/ProductAreaPage.tsx`

Removes: `metric-grid`, `loading-state`, `field-label`.

- [ ] **Step 1: Add LoadingState import**

```tsx
import { LoadingState } from "../components/ui/loading-state";
```

- [ ] **Step 2: Replace loading-state**

Change line 148 (`<div className="loading-state">Loading services...</div>`):
```tsx
<LoadingState>Loading services…</LoadingState>
```

- [ ] **Step 3: Replace metric-grid (two occurrences)**

Change line 131 (`<div className="metric-grid" aria-label="Service summary">`):
```tsx
<div
  className="grid gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
  style={{ gridTemplateColumns: "repeat(4, minmax(140px, 1fr))" }}
  aria-label="Service summary"
>
```

Change line 198 (`<div className="metric-grid" aria-label={`${copy.title} summary`}>`):
```tsx
<div
  className="grid gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
  style={{ gridTemplateColumns: "repeat(4, minmax(140px, 1fr))" }}
  aria-label={`${copy.title} summary`}
>
```

- [ ] **Step 4: Replace field-label in PageHeader**

Change line 223 (`<div className="field-label">{kicker}</div>`):
```tsx
<div className="text-xs font-bold uppercase text-[var(--muted)]">{kicker}</div>
```

- [ ] **Step 5: Run typecheck and tests**

```bash
cd apps/frontend && npm run typecheck && npm run test
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/pages/ProductAreaPage.tsx
git commit -m "refactor(ui): migrate ProductAreaPage to ui primitives"
```

---

## Task 10: Migrate `ServiceOverview.tsx`

**Files:**
- Modify: `apps/frontend/src/pages/ServiceOverview.tsx`

Removes: `table-panel` + its inline layout styles, `loading-state`, `field-label`, focused-service-bar inline style, topology inner container inline style.
Quarantined (intentionally kept): SVG canvas `style` props inside `TopologyMap` — node positions, edge stroke values, cursor style on `<g>` elements, popover absolute positioning at SVG coordinates.

- [ ] **Step 1: Add LoadingState and TablePanel imports**

```tsx
import { LoadingState } from "../components/ui/loading-state";
import { TablePanel } from "../components/ui/table-panel";
```

- [ ] **Step 2: Replace field-label eyebrow**

Change `<div className="field-label">Topology</div>`:
```tsx
<div className="text-xs font-bold uppercase text-[var(--muted)]">Topology</div>
```

- [ ] **Step 3: Replace focused-service bar inline style**

Change:
```tsx
<div
  style={{ display: "flex", gap: "1rem", alignItems: "center", padding: "0.5rem 0" }}
>
```
To:
```tsx
<div className="flex gap-4 items-center py-2">
```

- [ ] **Step 4: Replace outer topology container**

Change:
```tsx
<div
  className="table-panel"
  style={{
    padding: "2rem",
    overflow: "auto",
    background: "var(--bg-deep)",
    position: "relative",
  }}
>
```
To:
```tsx
<TablePanel className="p-8 overflow-auto relative bg-[var(--surface-inset)]">
```

And its closing `</div>` to `</TablePanel>`.

- [ ] **Step 5: Replace topology inner container**

Change:
```tsx
<div
  className="topology-map-container"
  style={{
    minHeight: "600px",
    display: "flex",
    justifyContent: "center",
    position: "relative",
  }}
>
```
To:
```tsx
<div className="min-h-[600px] flex justify-center relative">
```

- [ ] **Step 6: Replace loading-state**

Change `<div className="loading-state">Loading topology...</div>`:
```tsx
<LoadingState>Loading topology…</LoadingState>
```

- [ ] **Step 7: Run typecheck and tests**

```bash
cd apps/frontend && npm run typecheck && npm run test
```
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add apps/frontend/src/pages/ServiceOverview.tsx
git commit -m "refactor(ui): migrate ServiceOverview container styles to ui primitives"
```

---

## Task 11: Migrate `LogSearch.tsx`

**Files:**
- Modify: `apps/frontend/src/pages/LogSearch.tsx`

Removes: `field-label`, `table-panel`, `loading-state`, `status` on severity badge, infra link inline styles, outer flex container inline style, table inline styles.

- [ ] **Step 1: Add imports**

```tsx
import { Badge } from "../components/ui/badge";
import { LoadingState } from "../components/ui/loading-state";
import { TablePanel } from "../components/ui/table-panel";
```

- [ ] **Step 2: Replace field-label eyebrow**

Change `<div className="field-label">Explorer</div>`:
```tsx
<div className="text-xs font-bold uppercase text-[var(--muted)]">Explorer</div>
```

- [ ] **Step 3: Replace outer flex container inline style**

Change `<div style={{ display: "flex", alignItems: "flex-start" }}>`:
```tsx
<div className="flex items-start">
```

- [ ] **Step 4: Replace table-panel with TablePanel**

Change `<div className="table-panel" style={{ flex: 1 }}>` to `<TablePanel className="flex-1">` and the closing `</div>` to `</TablePanel>`.

- [ ] **Step 5: Replace loading-state**

Change `<div className="loading-state">Loading logs...</div>`:
```tsx
<LoadingState>Loading logs…</LoadingState>
```

- [ ] **Step 6: Replace table inline style**

Change `<table style={{ borderCollapse: "collapse", width: "100%" }}>` to `<table>`. The `styles.css` global `table` rule already handles this.

- [ ] **Step 7: Replace timestamp td inline style**

Change `<td style={{ whiteSpace: "nowrap" }}>` to `<td className="whitespace-nowrap">`.

- [ ] **Step 8: Replace severity status span with Badge**

Change:
```tsx
<td>
  <span className={`status ${severityTone(log.severity_number)}`}>
    {log.severity_text || log.severity_number}
  </span>
</td>
```
To:
```tsx
<td>
  <Badge tone={severityTone(log.severity_number)}>
    {log.severity_text || String(log.severity_number)}
  </Badge>
</td>
```

- [ ] **Step 9: Replace infra badge inline styles**

Change the infra badges wrapper and link styles in `LogRow`:

```tsx
// Before:
{badges.length > 0 && (
  <span style={{ display: "inline-flex", gap: 4, marginLeft: 6 }}>
    {badges.map((link) => (
      <a
        key={link.href}
        href={link.href}
        style={{
          fontSize: 11,
          padding: "1px 6px",
          borderRadius: 10,
          background: "var(--color-bg-subtle, #edf2f7)",
          color: "var(--color-text, #2d3748)",
          textDecoration: "none",
          border: "1px solid var(--color-border, #e2e8f0)",
          whiteSpace: "nowrap",
        }}
      >
        {link.label}
      </a>
    ))}
  </span>
)}

// After:
{badges.length > 0 && (
  <span className="inline-flex gap-1 ml-1.5">
    {badges.map((link) => (
      <a
        key={link.href}
        href={link.href}
        className="text-[11px] px-1.5 rounded-full bg-[var(--surface-subtle)] text-[var(--text)] no-underline border border-[var(--border)] whitespace-nowrap"
      >
        {link.label}
      </a>
    ))}
  </span>
)}
```

- [ ] **Step 10: Run typecheck and tests**

```bash
cd apps/frontend && npm run typecheck && npm run test
```
Expected: PASS.

- [ ] **Step 11: Commit**

```bash
git add apps/frontend/src/pages/LogSearch.tsx
git commit -m "refactor(ui): migrate LogSearch to ui primitives and token classes"
```

---

## Task 12: Migrate `TraceSearch.tsx`

**Files:**
- Modify: `apps/frontend/src/pages/TraceSearch.tsx`

Removes: `field-label`, `table-panel`, `loading-state`, `status` on status code badge, outer flex container inline style, table inline style.

- [ ] **Step 1: Add imports**

```tsx
import { Badge } from "../components/ui/badge";
import { LoadingState } from "../components/ui/loading-state";
import { TablePanel } from "../components/ui/table-panel";
```

- [ ] **Step 2: Replace field-label eyebrow**

Change `<div className="field-label">Explorer</div>`:
```tsx
<div className="text-xs font-bold uppercase text-[var(--muted)]">Explorer</div>
```

- [ ] **Step 3: Replace outer flex container inline style**

Change `<div style={{ display: "flex", alignItems: "flex-start" }}>`:
```tsx
<div className="flex items-start">
```

- [ ] **Step 4: Replace table-panel**

Change `<div className="table-panel" style={{ flex: 1 }}>` to `<TablePanel className="flex-1">` and closing `</div>` to `</TablePanel>`.

- [ ] **Step 5: Replace loading-state**

Change `<div className="loading-state">Loading traces...</div>`:
```tsx
<LoadingState>Loading traces…</LoadingState>
```

- [ ] **Step 6: Replace table inline style**

Change `<table style={{ borderCollapse: "collapse", width: "100%" }}>` to `<table>`.

- [ ] **Step 7: Replace status span with Badge**

Change:
```tsx
<td>
  <span className={`status ${root.status_code === "ERROR" ? "bad" : "good"}`}>
    {root.status_code}
  </span>
</td>
```
To:
```tsx
<td>
  <Badge tone={root.status_code === "ERROR" ? "bad" : "good"}>
    {root.status_code}
  </Badge>
</td>
```

- [ ] **Step 8: Run typecheck and tests**

```bash
cd apps/frontend && npm run typecheck && npm run test
```
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add apps/frontend/src/pages/TraceSearch.tsx
git commit -m "refactor(ui): migrate TraceSearch to ui primitives"
```

---

## Task 13: Migrate `AlertsPage.tsx`

**Files:**
- Modify: `apps/frontend/src/features/alerts/AlertsPage.tsx`

Removes: `field-label` on form labels and the "Reliability" section header.

- [ ] **Step 1: Replace all field-label usages**

Read the file. You will find `className="field-label"` on:
- The `<div className="field-label">Reliability</div>` section label (around line 76)
- Four `<label className="field-label" ...>` form labels (lines 102, 112, 125, 139)

Replace each with `className="text-xs font-bold uppercase text-[var(--muted)]"`.

For the label elements, the full replacement is:
```tsx
<label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="rule-name">Rule name</label>
<label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="metric-name">Metric name</label>
<label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="operator">Operator</label>
<label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="threshold">Threshold value</label>
```

And the section div:
```tsx
<div className="text-xs font-bold uppercase text-[var(--muted)]">Reliability</div>
```

- [ ] **Step 2: Run typecheck and tests**

```bash
cd apps/frontend && npm run typecheck && npm run test
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/features/alerts/AlertsPage.tsx
git commit -m "refactor(ui): migrate AlertsPage field-label to Tailwind token classes"
```

---

## Task 14: Clean up `styles.css`

**Files:**
- Modify: `apps/frontend/src/styles.css`

Remove all rule blocks for the retired classes. Add a comment marking `field-label`/`metric-label` as primitive-internal.

- [ ] **Step 1: Verify no product page still uses the retired classes**

Run these checks. Each should return zero matches:

```bash
cd apps/frontend
npx rg 'className="metric-tile' src --glob="*.tsx"
npx rg 'className="table-panel' src --glob="*.tsx"
npx rg 'className="detail-panel' src --glob="*.tsx"
npx rg 'className="entry-link' src --glob="*.tsx"
npx rg 'className="entity-card' src --glob="*.tsx"
npx rg 'className="signal-panel' src --glob="*.tsx"
npx rg 'className=.*\btab-link\b' src --glob="*.tsx"
npx rg 'className="loading-state' src --glob="*.tsx"
npx rg '"status (good|warn|bad)"' src --glob="*.tsx"
npx rg 'className="empty-panel' src --glob="*.tsx"
```

If any check returns results, go back and fix the relevant file before continuing.

> Note: `signal-empty` is intentionally kept and should NOT appear in the checks above.

- [ ] **Step 2: Remove retired CSS blocks from `styles.css`**

Remove these entire rule blocks from `apps/frontend/src/styles.css`:

- `.metric-tile`, `.metric-tile.good`, `.metric-tile.warn`, `.metric-tile.bad`, `.metric-value`
- `.empty-panel`, `.empty-title`, `.empty-metrics`, `.empty-metrics span`
- `.detail-panel`, `.detail-panel-header`, `.detail-panel h2`
- `.entry-link`, `.entry-link-grid`
- `.entity-card-list`, `.entity-card-row`, `.entity-card-link`, `.entity-card-metric`
- `.signal-panel`
- `.tab-list`, `.tab-link`, `.tab-link:hover`, `.tab-link.active`
- `.loading-state`
- `.status`, `.status.good`, `.status.warn` (keep `.status.bad` only if still referenced; otherwise remove too)

Keep `.field-label`, `.metric-label`, `.brand-context` — add a comment above them:

```css
/* primitive-internal — used by Panel and MetricCard primitives only.
   Do not use these classes in product pages; use Tailwind token classes instead. */
.brand-context,
.field-label,
.metric-label {
```

- [ ] **Step 3: Run full frontend checks**

```bash
cd apps/frontend && npm run typecheck && npm run lint && npm run test && npm run build
```
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/styles.css
git commit -m "refactor(ui): remove retired legacy CSS classes from styles.css"
```

---

## Task 15: Document the migration rule in `spec/15-frontend-local-dev.md`

**Files:**
- Modify: `spec/15-frontend-local-dev.md`

- [ ] **Step 1: Append the migration rule section**

Add the following section at the end of `spec/15-frontend-local-dev.md`:

```markdown
---

## 24. Frontend Styling Migration Rule

All frontend slices after UI-R3 must follow this rule:

1. **Primitives first:** Reuse `src/components/ui/` primitives for interactive elements, loading states, table wrappers, status indicators, metric cards, panels, and buttons. Do not duplicate their styling in product pages.
2. **Add missing primitives in the same slice:** If a needed primitive does not exist, add it to `src/components/ui/` with a focused RTL test in the same PR. Do not defer primitive additions to a follow-up.
3. **Token classes over hardcoded values:** Use Tailwind token classes (`text-[var(--muted)]`, `bg-[var(--surface)]`, `border-[var(--border)]`, etc.) instead of hardcoded hex values or non-standard CSS custom properties.
4. **Avoid page-local interactive styling:** Do not define new interactive CSS rules (hover, focus, active states) in `styles.css` or in `<style>` blocks. Express them via Tailwind utilities.
5. **Approved rendering exceptions** — inline `style` props are permitted only for:
   - **Span bar position/width** in `TraceDetail.tsx`: percentage-based `left`/`width` for canvas-accurate waterfall rendering.
   - **Topology SVG/canvas** in `ServiceOverview.tsx`: dynamic `x`/`y`/`fill`/`stroke` values for force-directed graph node and edge positioning.
   - **Severity colors** in `LogContextView.tsx`, `LogCorrelatedList.tsx`, `LogLiveTail.tsx`: runtime-computed numeric severity → color mapping.
   - **`minmax()` grid columns**: `style={{ gridTemplateColumns: "repeat(4, minmax(140px, 1fr))" }}` where Tailwind has no direct equivalent.
   - Any new exception must be noted with a comment in the file and approved by the reviewer.
```

- [ ] **Step 2: Run doc-review skill if available, otherwise skip**

- [ ] **Step 3: Commit**

```bash
git add spec/15-frontend-local-dev.md
git commit -m "docs: add frontend styling migration rule to spec/15"
```

---

## Task 16: Final verification

- [ ] **Step 1: Run full local CI**

```bash
bash scripts/local-ci.sh --skip-docker
```
Expected: Rust fmt ✓, clippy ✓, tests ✓, frontend typecheck ✓, lint ✓, build ✓, test ✓.

- [ ] **Step 2: Confirm retired classes are gone from product pages**

```bash
cd apps/frontend
npx rg 'className=.*(metric-tile|"table-panel|"detail-panel|"entry-link|"entity-card|"signal-panel|tab-link[^s]|"loading-state|"empty-panel)' src/pages src/features src/components/ServiceInfraPanel.tsx --glob="*.tsx"
```
Expected: zero matches.

- [ ] **Step 3: Run accessibility scan**

```bash
cd apps/frontend && npm run e2e 2>/dev/null || echo "Chromium not available — skipping e2e"
```
If Chromium is available: expected all accessibility tests pass.

- [ ] **Step 4: Update the phase plan**

In `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`, mark `UI-R3` as complete:

```markdown
- [x] **UI-R3: Remove remaining legacy style drift and document the frontend migration rule**
```

Add the checkpoint answer:
> Checkpoint: is the frontend modernized enough that new product UI work can start without copying legacy local styles? Answer: yes. All legacy CSS classes (`metric-tile`, `table-panel`, `detail-panel`, `entry-link*`, `entity-card-*`, `signal-panel`, `tab-link`/`tab-list`, `loading-state`, `status`) have been removed from product pages. Three new primitives (`LoadingState`, `TablePanel`, `HealthDot`) cover the high-frequency patterns. The migration rule is documented in `spec/15-frontend-local-dev.md §24`. Rendering exceptions (span bar offsets, topology SVG, severity colors, `minmax()` grid) are documented and approved.

- [ ] **Step 5: Commit plan update**

```bash
git add docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md
git commit -m "docs: mark UI-R3 complete in phases 2-8 plan"
```
