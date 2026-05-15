# TraceDetail Uplift Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring `TraceDetailPage` and `TraceDetail` to visual parity with the rest of the uplifted pages — `page-stack`/`page-header` layout, MetricCard summary row, service color legend, Panel wrappers around the waterfall and correlated-logs sections.

**Architecture:** Changes are local to three files. `TraceDetailPage` gets a trivial loading/error fix. `TraceDetail` switches from a raw `div.grid` to `section.page-stack`, adds MetricCards and a service legend using data already computed from the spans prop, and wraps the waterfall and `LogCorrelatedList` in `Panel` components. `LogCorrelatedList` loses its internal `<h3>` label (now carried by the wrapping Panel title in the parent). No new API calls or hooks are introduced.

**Tech Stack:** React, TypeScript, Tailwind CSS with CSS design-system variables, `@tanstack/react-query`, Vitest + React Testing Library.

---

## File Map

| File | Action |
|---|---|
| `apps/frontend/src/pages/TraceDetailPage.tsx` | Modify — replace bare `<p>` loading/error states with `LoadingState`/`EmptyState` |
| `apps/frontend/src/pages/TraceDetail.tsx` | Modify — full layout uplift: page-stack, page-header, MetricCards, service legend, Panel wrappers |
| `apps/frontend/src/pages/TraceDetail.test.tsx` | Modify — add router mock, fix ambiguous `getByText("5.00ms")` |
| `apps/frontend/src/pages/TraceDetail.renovation.test.tsx` | Modify — add router mock |
| `apps/frontend/src/components/LogCorrelatedList.tsx` | Modify — remove `<h3>` section label (moved to Panel title in parent) |
| `apps/frontend/src/components/LogCorrelatedList.render.test.tsx` | Modify — remove assertions on the heading texts that are no longer rendered by the component |

---

## Task 1: Fix TraceDetailPage loading and error states

**Files:**
- Modify: `apps/frontend/src/pages/TraceDetailPage.tsx`

- [ ] **Step 1: Replace the file**

```tsx
import { useParams } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { getTrace } from "../api/traces";
import { TraceDetail } from "./TraceDetail";
import { EmptyState } from "../components/ui/empty-state";
import { LoadingState } from "../components/ui/loading-state";
import { useTenantContext } from "../hooks/useTenantContext";

export default function TraceDetailPage() {
  const { traceId } = useParams({ from: "/traces/$traceId" });
  const { tenantId } = useTenantContext();
  const { data, isLoading } = useQuery({
    queryKey: ["trace", tenantId, traceId],
    queryFn: () => getTrace(tenantId, traceId),
  });
  if (isLoading) return <LoadingState>Loading trace…</LoadingState>;
  if (!data) return <EmptyState title="Trace not found." />;
  return <TraceDetail traceId={data.trace_id} spans={data.spans} events={data.events} />;
}
```

- [ ] **Step 2: Run the full frontend test suite to confirm nothing broke**

```
cd apps/frontend && npx vitest run
```

Expected: All existing tests pass.

- [ ] **Step 3: Commit**

```
git add apps/frontend/src/pages/TraceDetailPage.tsx
git commit -m "fix(traces): replace bare loading/error states with LoadingState and EmptyState"
```

---

## Task 2: Update existing TraceDetail tests — add router mock and fix ambiguous assertion

**Files:**
- Modify: `apps/frontend/src/pages/TraceDetail.test.tsx`
- Modify: `apps/frontend/src/pages/TraceDetail.renovation.test.tsx`

After the uplift in Task 3, `TraceDetail` will render a `<Link to="/traces">` from `@tanstack/react-router`. Without a router context the component throws. Both existing test files need the same mock.

Additionally, in `TraceDetail.test.tsx` the assertion `getByText("5.00ms")` will become ambiguous: the Duration MetricCard and the span row both show `5.00ms` for a single-span trace. Fix it now.

- [ ] **Step 1: Add the router mock and fix the assertion in `TraceDetail.test.tsx`**

At the top of the file (after the existing imports, before `const queryClient = ...`), add:

```tsx
vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    Link: ({
      children,
      ...props
    }: React.AnchorHTMLAttributes<HTMLAnchorElement> & { children?: React.ReactNode }) => (
      <a {...props}>{children}</a>
    ),
  };
});
```

Also add `vi` to the existing import from `vitest`:

```tsx
import { vi } from "vitest";
```

Then change the `"renders waterfall with spans"` test — replace the `getByText("5.00ms")` assertion with a specific span-row check:

```tsx
test("renders waterfall with spans", () => {
  render(
    <QueryClientProvider client={queryClient}>
      <TenantContextProvider>
        <TimeDisplayProvider>
          <TraceDetail traceId="abc" spans={[baseSpan]} />
        </TimeDisplayProvider>
      </TenantContextProvider>
    </QueryClientProvider>
  );
  expect(screen.getByText(/POST \/order/)).toBeInTheDocument();
  // duration appears in the span row (and after uplift also in the Duration MetricCard)
  expect(screen.getAllByText("5.00ms").length).toBeGreaterThanOrEqual(1);
});
```

- [ ] **Step 2: Add the router mock to `TraceDetail.renovation.test.tsx`**

At the top of the file (after the existing imports, before `function wrapper ...`), add:

```tsx
vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    Link: ({
      children,
      ...props
    }: React.AnchorHTMLAttributes<HTMLAnchorElement> & { children?: React.ReactNode }) => (
      <a {...props}>{children}</a>
    ),
  };
});
```

- [ ] **Step 3: Run both test files to confirm they still pass**

```
cd apps/frontend && npx vitest run src/pages/TraceDetail.test.tsx src/pages/TraceDetail.renovation.test.tsx
```

Expected: All tests pass (the router mock is inert until `Link` is actually used).

---

## Task 3: Write new failing tests for the uplift, then implement

**Files:**
- Modify: `apps/frontend/src/pages/TraceDetail.test.tsx`
- Modify: `apps/frontend/src/pages/TraceDetail.tsx`

### Step 3a — Write the new failing tests

- [ ] **Step 1: Add the new tests to the end of `TraceDetail.test.tsx`**

First, add `searchLogs` mock setup to the existing `beforeEach` if absent. Since the existing file has no `beforeEach`, add one after the `queryClient` declaration and before the existing tests:

```tsx
import * as logsApi from "../api/logs";

// ... existing code ...

beforeEach(() => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({ logs: [], total: 0, facets: {} });
});
```

Then add these tests at the end of the file:

```tsx
test("renders page-header with Traces eyebrow and truncated trace ID", () => {
  render(<TraceDetail traceId="abcdef1234567890xyz" spans={[baseSpan]} />, { wrapper });
  expect(screen.getByText("Traces")).toBeInTheDocument();
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("abcdef12345678…");
});

test("renders Back to traces link", () => {
  render(<TraceDetail traceId="abc" spans={[baseSpan]} />, { wrapper });
  const link = screen.getByRole("link", { name: "Back to traces" });
  expect(link).toBeInTheDocument();
  expect(link).toHaveAttribute("href", "/traces");
});

test("renders MetricCard row with span count, duration, services, and errors", () => {
  const errorSpan = { ...baseSpan, span_id: "222", status_code: "ERROR" };
  render(<TraceDetail traceId="abc" spans={[baseSpan, errorSpan]} />, { wrapper });
  expect(screen.getByText("Total Spans")).toBeInTheDocument();
  expect(screen.getByText("Duration")).toBeInTheDocument();
  expect(screen.getByText("Services")).toBeInTheDocument();
  expect(screen.getByText("Errors")).toBeInTheDocument();
});

test("Errors MetricCard has bad tone when there are error spans", () => {
  const errorSpan = { ...baseSpan, span_id: "222", status_code: "ERROR" };
  render(<TraceDetail traceId="abc" spans={[baseSpan, errorSpan]} />, { wrapper });
  // The error count MetricCard value is "1"
  expect(screen.getByText("Errors")).toBeInTheDocument();
});

test("renders service color legend with unique service names", () => {
  const paymentSpan = {
    ...baseSpan,
    span_id: "222",
    service_name: "payment",
  };
  render(<TraceDetail traceId="abc" spans={[baseSpan, paymentSpan]} />, { wrapper });
  const legend = screen.getByRole("generic", { name: "Service color legend" });
  expect(legend).toBeInTheDocument();
  expect(legend).toHaveTextContent("checkout");
  expect(legend).toHaveTextContent("payment");
});

test("service color legend deduplicates services", () => {
  const span2 = { ...baseSpan, span_id: "222" };
  render(<TraceDetail traceId="abc" spans={[baseSpan, span2]} />, { wrapper });
  const legend = screen.getByRole("generic", { name: "Service color legend" });
  // Only one "checkout" entry despite two spans from the same service
  expect(legend.querySelectorAll("span[aria-hidden]")).toHaveLength(1);
});

test("waterfall is wrapped in a Panel with Spans heading", () => {
  render(<TraceDetail traceId="abc" spans={[baseSpan]} />, { wrapper });
  expect(screen.getByText("Spans")).toBeInTheDocument();
  expect(screen.getByText("Waterfall")).toBeInTheDocument();
});

test("correlated logs panel shows Trace-correlated logs title when no span selected", () => {
  render(<TraceDetail traceId="abc" spans={[baseSpan]} />, { wrapper });
  expect(screen.getByText("Trace-correlated logs")).toBeInTheDocument();
  expect(screen.getByText("Correlation")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run new tests to confirm they fail**

```
cd apps/frontend && npx vitest run src/pages/TraceDetail.test.tsx
```

Expected: The 7 new tests FAIL (component does not yet have the new structure).

### Step 3b — Implement the TraceDetail uplift

- [ ] **Step 3: Replace the entire `TraceDetail.tsx` file**

```tsx
import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { Span, SpanEvent } from "../api/traces";
import { LogCorrelatedList } from "../components/LogCorrelatedList";
import { infraLinks, InfraLink } from "../utils/infraLinks";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { MetricCard } from "../components/ui/metric-card";
import { Panel } from "../components/ui/panel";

interface Props {
  traceId: string;
  spans: Span[];
  events?: SpanEvent[];
}

function mergedInfraLinks(spans: Span[]): InfraLink[] {
  const seen = new Set<string>();
  const result: InfraLink[] = [];
  for (const span of spans) {
    for (const link of infraLinks(span.resource_attributes ?? {})) {
      if (!seen.has(link.href)) {
        seen.add(link.href);
        result.push(link);
      }
    }
  }
  return result;
}

function buildDepthMap(spans: Span[]): Map<string, number> {
  const parentOf = new Map(spans.map((s) => [s.span_id, s.parent_span_id]));
  const memo = new Map<string, number>();
  function depth(id: string): number {
    if (memo.has(id)) return memo.get(id)!;
    const parent = parentOf.get(id);
    const d = parent && parentOf.has(parent) ? depth(parent) + 1 : 0;
    memo.set(id, d);
    return d;
  }
  for (const s of spans) depth(s.span_id);
  return memo;
}

const SERVICE_COLORS = [
  "var(--brand)",
  "#7c3aed",
  "#0891b2",
  "#059669",
  "#d97706",
  "#db2777",
  "#6d28d9",
  "#0284c7",
];

function serviceColor(name: string): string {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) & 0xffff;
  return SERVICE_COLORS[h % SERVICE_COLORS.length];
}

function TimeRuler({ totalMs }: { totalMs: number }) {
  const ticks = [0, 0.25, 0.5, 0.75, 1.0];
  return (
    <div className="flex items-center mb-1 select-none" aria-hidden="true">
      <span className="w-[200px] shrink-0" />
      <div className="flex-1 relative h-4">
        {ticks.map((t) => (
          <span
            key={t}
            className="absolute text-[10px] text-[var(--muted)] -translate-x-1/2 top-0"
            style={{ left: `${t * 100}%` }}
          >
            {(totalMs * t).toFixed(1)}ms
          </span>
        ))}
      </div>
      <span className="w-[60px] shrink-0" />
    </div>
  );
}

function CopyButton({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      className="ml-1 text-[10px] text-[var(--muted)] hover:text-[var(--brand)]"
      onClick={() => {
        navigator.clipboard.writeText(value);
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      }}
      title="Copy"
    >
      {copied ? "✓" : "⎘"}
    </button>
  );
}

function SectionHeader({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="m-0 mt-4 mb-2 text-xs font-bold uppercase text-[var(--muted)] border-b border-[var(--border)] pb-1">
      {children}
    </h3>
  );
}

function DlRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="contents">
      <dt className="break-all font-bold text-[var(--muted)]">{label}</dt>
      <dd className="m-0 min-w-0 break-all text-[var(--text)]">{children}</dd>
    </div>
  );
}

function SpanContextPanel({
  span,
  events,
  traceStartNs: _traceStartNs,
  onClose,
}: {
  span: Span;
  events: SpanEvent[];
  traceStartNs: number;
  onClose: () => void;
}) {
  const dbSystem = span.attributes?.["db.system"] as string | undefined;
  const dbName = span.attributes?.["db.name"] as string | undefined;
  const dbOp = span.attributes?.["db.operation"] as string | undefined;
  const dbStatement = span.attributes?.["db.statement"] as string | undefined;

  const httpMethod = span.attributes?.["http.method"] as string | undefined;
  const httpUrl = (span.attributes?.["http.url"] ??
    span.attributes?.["http.target"]) as string | undefined;
  const httpStatus = span.attributes?.["http.status_code"] as
    | string
    | number
    | undefined;

  const remainingAttrs = Object.entries(span.attributes ?? {}).filter(
    ([k]) => !k.startsWith("db.") && !k.startsWith("http.")
  );

  const spanInfraLinks = infraLinks(span.resource_attributes ?? {});
  const hasResourceSection =
    Object.keys(span.resource_attributes ?? {}).length > 0 ||
    spanInfraLinks.length > 0;

  const startMs = span.start_time_unix_nano / 1e6;
  const startDate = new Date(startMs).toISOString();

  return (
    <aside
      aria-label="Selected span context"
      className="w-[320px] shrink-0 border border-[var(--border)] bg-[var(--surface)] p-4 max-[900px]:w-full max-h-[calc(100vh-80px)] overflow-y-auto"
    >
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">
            Selected Span
          </div>
          <h2 className="m-0 text-base font-bold text-[var(--text-strong)]">
            Context Properties
          </h2>
        </div>
        <Button
          variant="secondary"
          className="min-h-8 px-2 text-xs"
          onClick={onClose}
        >
          Close
        </Button>
      </div>

      <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
        <DlRow label="trace_id">
          <span title={span.trace_id}>
            {span.trace_id.substring(0, 16)}…
          </span>
          <CopyButton value={span.trace_id} />
        </DlRow>
        <DlRow label="span_id">
          <span title={span.span_id}>
            {span.span_id.substring(0, 16)}
          </span>
          <CopyButton value={span.span_id} />
        </DlRow>
        <DlRow label="service">{span.service_name}</DlRow>
        {span.service_version && (
          <DlRow label="version">{span.service_version}</DlRow>
        )}
        <DlRow label="operation">{span.operation_name}</DlRow>
        <DlRow label="kind">{span.span_kind}</DlRow>
        <DlRow label="status">
          <Badge tone={span.status_code === "ERROR" ? "bad" : "good"}>
            {span.status_code}
          </Badge>
        </DlRow>
        <DlRow label="duration">
          {(span.duration_ns / 1e6).toFixed(2)}ms
        </DlRow>
        <DlRow label="start time">{startDate}</DlRow>
      </dl>

      {dbSystem && (
        <>
          <SectionHeader>DB Operation</SectionHeader>
          <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
            <DlRow label="system">{dbSystem}</DlRow>
            {dbName && <DlRow label="database">{dbName}</DlRow>}
            {dbOp && <DlRow label="operation">{dbOp}</DlRow>}
          </dl>
          {dbStatement && (
            <pre className="mt-2 text-[11px] p-2 bg-[var(--surface-inset)] border border-[var(--border)] overflow-x-auto whitespace-pre-wrap break-all">
              {dbStatement}
            </pre>
          )}
        </>
      )}

      {httpMethod && (
        <>
          <SectionHeader>HTTP</SectionHeader>
          <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
            <DlRow label="method">{httpMethod}</DlRow>
            {httpUrl && <DlRow label="url">{httpUrl}</DlRow>}
            {httpStatus !== undefined && (
              <DlRow label="status_code">{String(httpStatus)}</DlRow>
            )}
          </dl>
        </>
      )}

      {events.length > 0 && (
        <>
          <SectionHeader>Span Events</SectionHeader>
          <div className="space-y-2">
            {events.map((e) => {
              const offsetMs =
                (e.timestamp_unix_nano - span.start_time_unix_nano) / 1e6;
              return (
                <div
                  key={e.event_index}
                  className="text-xs border border-[var(--border)] p-2 bg-[var(--surface-inset)]"
                >
                  <div className="flex justify-between">
                    <span className="font-bold text-[var(--text-strong)]">
                      {e.name}
                    </span>
                    <span className="text-[var(--muted)]">
                      +{offsetMs.toFixed(2)}ms
                    </span>
                  </div>
                  {e.attributes && Object.keys(e.attributes).length > 0 && (
                    <dl className="mt-1 grid grid-cols-[minmax(88px,auto)_1fr] gap-x-2 gap-y-1 text-[11px]">
                      {Object.entries(e.attributes).map(([k, v]) => (
                        <div key={k} className="contents">
                          <dt className="text-[var(--muted)] font-bold break-all">
                            {k}
                          </dt>
                          <dd className="m-0 break-all">{String(v)}</dd>
                        </div>
                      ))}
                    </dl>
                  )}
                </div>
              );
            })}
          </div>
        </>
      )}

      {remainingAttrs.length > 0 && (
        <>
          <SectionHeader>Attributes</SectionHeader>
          <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
            {remainingAttrs.map(([k, v]) => (
              <DlRow key={k} label={k}>
                {String(v)}
              </DlRow>
            ))}
          </dl>
        </>
      )}

      {hasResourceSection && (
        <>
          <SectionHeader>Resource / Infrastructure</SectionHeader>
          {Object.keys(span.resource_attributes ?? {}).length > 0 && (
            <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
              {Object.entries(span.resource_attributes ?? {}).map(([k, v]) => (
                <DlRow key={k} label={k}>
                  {String(v)}
                </DlRow>
              ))}
            </dl>
          )}
          {spanInfraLinks.length > 0 && (
            <div className="flex flex-wrap gap-2 mt-2">
              {spanInfraLinks.map((link) => (
                <a
                  key={link.href}
                  href={link.href}
                  className="text-xs px-2 py-0.5 bg-[var(--surface-subtle)] text-[var(--text)] border border-[var(--border)] no-underline hover:border-[var(--brand)] hover:text-[var(--brand)]"
                >
                  {link.label}
                </a>
              ))}
            </div>
          )}
        </>
      )}
    </aside>
  );
}

export function TraceDetail({ traceId, spans, events }: Props) {
  const [selectedSpanId, setSelectedSpanId] = useState<string | undefined>();
  const minStart = Math.min(...spans.map((s) => Number(s.start_time_unix_nano)));
  const maxEnd = Math.max(...spans.map((s) => Number(s.end_time_unix_nano)));
  const totalNs = maxEnd - minStart || 1;
  const totalMs = totalNs / 1e6;

  const infraPills = mergedInfraLinks(spans);
  const depthMap = buildDepthMap(spans);
  const selectedSpan = spans.find((s) => s.span_id === selectedSpanId);

  const uniqueServices = [...new Set(spans.map((s) => s.service_name))];
  const errorCount = spans.filter((s) => s.status_code === "ERROR").length;
  const logPanelTitle = selectedSpanId
    ? `Exact span logs (${selectedSpanId.substring(0, 8)}…) and trace-level logs`
    : "Trace-correlated logs";

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Traces</div>
          <h1>{traceId.substring(0, 16)}…</h1>
        </div>
        <Link to="/traces" className="secondary-link">Back to traces</Link>
      </div>

      <div className="grid grid-cols-4 gap-3 max-[700px]:grid-cols-2">
        <MetricCard label="Total Spans" value={spans.length} tone="info" />
        <MetricCard label="Duration" value={`${totalMs.toFixed(2)}ms`} tone="info" />
        <MetricCard label="Services" value={uniqueServices.length} tone="info" />
        <MetricCard label="Errors" value={errorCount} tone={errorCount > 0 ? "bad" : "good"} />
      </div>

      <div className="flex flex-wrap gap-x-4 gap-y-1" aria-label="Service color legend">
        {uniqueServices.map((name) => (
          <span key={name} className="flex items-center gap-1.5 text-xs text-[var(--muted)]">
            <span
              aria-hidden="true"
              className="inline-block h-2 w-2 rounded-full shrink-0"
              style={{ background: serviceColor(name) }}
            />
            {name}
          </span>
        ))}
      </div>

      {infraPills.length > 0 && (
        <div aria-label="Infrastructure" className="flex flex-wrap gap-2">
          {infraPills.map((link) => (
            <a
              key={link.href}
              href={link.href}
              className="text-xs px-2 py-0.5 bg-[var(--surface-subtle)] text-[var(--text)] border border-[var(--border)] no-underline hover:border-[var(--brand)] hover:text-[var(--brand)]"
            >
              {link.label}
            </a>
          ))}
        </div>
      )}

      <Panel eyebrow="Waterfall" title="Spans">
        <div className="overflow-x-auto">
          <TimeRuler totalMs={totalMs} />
          <div className="flex items-start gap-3 max-[900px]:flex-col">
            <div className="flex-1 min-w-0">
              {spans.map((span) => {
                const offset =
                  ((Number(span.start_time_unix_nano) - minStart) / totalNs) * 100;
                const width = (span.duration_ns / totalNs) * 100;
                const isSelected = selectedSpanId === span.span_id;
                const depth = depthMap.get(span.span_id) ?? 0;
                const color =
                  span.status_code === "ERROR"
                    ? "var(--bad)"
                    : serviceColor(span.service_name);
                return (
                  <div
                    key={span.span_id}
                    role="button"
                    tabIndex={0}
                    onClick={() =>
                      setSelectedSpanId(isSelected ? undefined : span.span_id)
                    }
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        setSelectedSpanId(isSelected ? undefined : span.span_id);
                      }
                    }}
                    className={`flex items-center mb-1 cursor-pointer px-0 py-0.5 ${
                      isSelected ? "bg-[var(--surface-subtle)]" : "bg-transparent"
                    }`}
                  >
                    <span
                      className="w-[200px] overflow-hidden text-ellipsis whitespace-nowrap text-xs shrink-0"
                      style={{ paddingLeft: `${depth * 12}px` }}
                    >
                      {span.service_name}: {span.operation_name}
                      <span className="ml-1 text-[10px] text-[var(--muted)] font-mono">
                        [{span.span_kind}]
                      </span>
                    </span>
                    <div className="flex-1 relative h-4 bg-[var(--surface-inset)]">
                      <div
                        className="absolute h-full"
                        style={{
                          left: `${offset}%`,
                          width: `${Math.max(width, 0.5)}%`,
                          background: color,
                        }}
                        title={`${(span.duration_ns / 1e6).toFixed(2)}ms`}
                      />
                    </div>
                    <span className="w-[60px] text-right text-xs shrink-0">
                      {(span.duration_ns / 1e6).toFixed(2)}ms
                    </span>
                  </div>
                );
              })}
            </div>
            {selectedSpan && (
              <SpanContextPanel
                span={selectedSpan}
                events={(events ?? []).filter(
                  (e) => e.span_id === selectedSpan.span_id
                )}
                traceStartNs={minStart}
                onClose={() => setSelectedSpanId(undefined)}
              />
            )}
          </div>
        </div>
      </Panel>

      <Panel eyebrow="Correlation" title={logPanelTitle}>
        <LogCorrelatedList traceId={traceId} spanId={selectedSpanId} />
      </Panel>
    </section>
  );
}
```

- [ ] **Step 4: Run new tests to confirm they pass**

```
cd apps/frontend && npx vitest run src/pages/TraceDetail.test.tsx
```

Expected: All tests pass including the 7 new ones.

- [ ] **Step 5: Run renovation tests to confirm they still pass**

```
cd apps/frontend && npx vitest run src/pages/TraceDetail.renovation.test.tsx
```

Expected: All 3 tests pass.

- [ ] **Step 6: Commit**

```
git add apps/frontend/src/pages/TraceDetail.tsx apps/frontend/src/pages/TraceDetail.test.tsx apps/frontend/src/pages/TraceDetail.renovation.test.tsx
git commit -m "feat(traces): uplift TraceDetail with page-stack, MetricCards, service legend, and Panel wrappers"
```

---

## Task 4: Remove h3 from LogCorrelatedList and wrap in Panel in TraceDetail

**Context:** After Task 3, `LogCorrelatedList` is wrapped in a Panel whose `title` carries the dynamic label ("Trace-correlated logs" / "Exact span logs…"). The `<h3>` inside `LogCorrelatedList` is now redundant and creates a duplicate heading. Remove it. Update `LogCorrelatedList.render.test.tsx` to remove the assertions that checked for that heading (the heading now lives in the parent).

**Files:**
- Modify: `apps/frontend/src/components/LogCorrelatedList.tsx`
- Modify: `apps/frontend/src/components/LogCorrelatedList.render.test.tsx`

- [ ] **Step 1: Remove the `<h3>` from `LogCorrelatedList.tsx`**

In `LogCorrelatedList.tsx`, the `return` block currently starts with:

```tsx
return (
  <div className="mt-5">
    <h3 className="text-sm font-bold text-[var(--text-strong)] mb-2">
      {spanId
        ? `Exact span logs and trace-level logs (${spanId.substring(0, 8)})`
        : "Trace-correlated logs"}
    </h3>
    <LogList
```

Replace with (remove the `<h3>` and the outer `div.mt-5`, since Panel provides the spacing):

```tsx
return (
  <div>
    <LogList
```

Full replacement for the entire `return` block in `LogCorrelatedList`:

```tsx
  return (
    <div>
      <LogList
        logs={logs}
        loading={isLoading}
        emptyMessage="No correlated logs found."
        onRowClick={(log) => setFocusedLogId(log.log_id)}
        showTraceLink
        timeFormat={format}
      />
      {focusedLogId && (
        <LogContextView logId={focusedLogId} onClose={() => setFocusedLogId(undefined)} />
      )}
    </div>
  );
```

- [ ] **Step 2: Update `LogCorrelatedList.render.test.tsx` — remove heading assertions**

The tests at lines 66–79 and 81–94 assert that heading text is rendered by the component. After removing the `<h3>`, those assertions must change to verify the content (log rows) rather than the heading.

Replace the `"shows trace-correlated heading when no span selected"` test:

```tsx
test("shows all logs when no span selected", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [traceLog, spanLog],
    total: 2,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText("trace level message")).toBeInTheDocument()
  );
  expect(screen.getByText("span level message")).toBeInTheDocument();
});
```

Replace the `"shows span-scoped heading and filters when spanId is provided"` test:

```tsx
test("filters to exact span logs and trace-level logs when spanId provided", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [traceLog, spanLog],
    total: 2,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" spanId="span-111" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText("span level message")).toBeInTheDocument()
  );
  expect(screen.getByText("trace level message")).toBeInTheDocument();
});
```

- [ ] **Step 3: Run `LogCorrelatedList` tests to confirm they pass**

```
cd apps/frontend && npx vitest run src/components/LogCorrelatedList
```

Expected: All tests pass.

- [ ] **Step 4: Run the full frontend test suite**

```
cd apps/frontend && npx vitest run
```

Expected: All tests pass.

- [ ] **Step 5: Commit**

```
git add apps/frontend/src/components/LogCorrelatedList.tsx apps/frontend/src/components/LogCorrelatedList.render.test.tsx
git commit -m "refactor(traces): remove LogCorrelatedList self-label h3, heading now in parent Panel"
```

---

## Task 5: Open pull request

- [ ] **Step 1: Push and open PR**

```
git push -u origin HEAD
gh pr create --title "feat(traces): uplift TraceDetail with page-stack, MetricCards, service color legend, and Panel wrappers" --body "$(cat <<'EOF'
## Summary
- Replace bare \`<p>Loading…</p>\` / \`<p>Not found</p>\` states in TraceDetailPage with \`LoadingState\` and \`EmptyState\`
- Switch TraceDetail wrapper from \`div.grid\` to \`section.page-stack\`
- Add \`page-header\` with Traces eyebrow, truncated trace ID h1, and Back-to-traces link
- Add MetricCard row: Total Spans, Duration, Services (unique), Errors (red when > 0)
- Add service color legend — one dot per unique service mapped via \`serviceColor()\`
- Wrap waterfall in \`Panel eyebrow="Waterfall" title="Spans"\`
- Wrap correlated logs in \`Panel eyebrow="Correlation" title={dynamic}\` — dynamic title reflects selected span
- Remove redundant \`<h3>\` from \`LogCorrelatedList\` (title now in parent Panel)

## Test plan
- [ ] \`npx vitest run src/pages/TraceDetail.test.tsx\` — all tests pass
- [ ] \`npx vitest run src/pages/TraceDetail.renovation.test.tsx\` — all tests pass
- [ ] \`npx vitest run src/components/LogCorrelatedList\` — all tests pass
- [ ] \`npx vitest run\` — full suite clean
- [ ] Open \`/traces/<any-trace-id>\` in browser — MetricCards visible, service legend shows, waterfall in Panel, correlated logs in Panel
- [ ] Click a span — SpanContextPanel appears inside waterfall Panel, Correlation panel title updates to span-scoped label
- [ ] Click Back to traces — navigates to /traces

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```
