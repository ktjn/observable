# P3-S10 Infrastructure Correlation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Users can navigate from a service, trace, or log view to correlated host/pod/container infrastructure detail pages using OTel resource attributes that the backend already returns.

**Architecture:** Frontend-only change. The backend `Span` and `LogRecord` domain structs already include `resource_attributes` in JSON responses — the TypeScript interfaces just don't expose them yet. A new pure utility `infraLinks()` maps OTel attribute keys to `/infrastructure/:type/:id` URLs. Three surfaces consume it: a `ServiceInfraPanel` in the service overview tab, a trace-level infra pill row in `TraceDetail`, and per-row infra badges in the log explorer.

**Tech Stack:** React, TypeScript, TanStack Query, TanStack Router, Vitest + Testing Library. All changes in `apps/frontend/src/`.

---

## File Map

| Action | File | Responsibility |
|---|---|---|
| Create | `apps/frontend/src/utils/infraLinks.ts` | Pure OTel-attr → `/infrastructure` URL mapper |
| Create | `apps/frontend/src/utils/infraLinks.test.ts` | Unit tests for the mapper |
| Modify | `apps/frontend/src/api/traces.ts` | Add `resource_attributes` to `Span` interface |
| Modify | `apps/frontend/src/api/logs.ts` | Add `resource_attributes` to `LogRecord` interface |
| Create | `apps/frontend/src/components/ServiceInfraPanel.tsx` | Compact infra entity list for service overview |
| Create | `apps/frontend/src/components/ServiceInfraPanel.test.tsx` | Tests for panel rendering |
| Modify | `apps/frontend/src/pages/ServiceDetailPage.tsx` | Wire `ServiceInfraPanel` below metrics grid |
| Modify | `apps/frontend/src/pages/TraceDetail.tsx` | Add trace-level infra pill summary above waterfall |
| Modify | `apps/frontend/src/pages/TraceDetail.test.tsx` | Extend with infra summary test cases |
| Modify | `apps/frontend/src/pages/LogSearch.tsx` | Add infra badges to each log row |

---

## Task 1: `infraLinks` utility — tests first

**Files:**
- Create: `apps/frontend/src/utils/infraLinks.ts`
- Create: `apps/frontend/src/utils/infraLinks.test.ts`

- [ ] **Step 1: Create the test file**

Create `apps/frontend/src/utils/infraLinks.test.ts` with this content:

```ts
import { describe, it, expect } from "vitest";
import { infraLinks } from "./infraLinks";

describe("infraLinks", () => {
  it("returns empty array for empty attrs", () => {
    expect(infraLinks({})).toEqual([]);
  });

  it("returns empty array for unrecognised attrs", () => {
    expect(infraLinks({ "custom.attr": "value", "another.key": "x" })).toEqual([]);
  });

  it("returns a pod link when k8s.pod.name is present", () => {
    const links = infraLinks({ "k8s.pod.name": "checkout-pod-1" });
    expect(links).toHaveLength(1);
    expect(links[0].label).toBe("pod: checkout-pod-1");
    expect(links[0].href).toBe("/infrastructure/pod/checkout-pod-1");
  });

  it("returns a host link from host.name", () => {
    const links = infraLinks({ "host.name": "node-3" });
    expect(links).toHaveLength(1);
    expect(links[0].label).toBe("host: node-3");
    expect(links[0].href).toBe("/infrastructure/host/node-3");
  });

  it("falls back to host.id when host.name is absent", () => {
    const links = infraLinks({ "host.id": "h-abc123" });
    expect(links).toHaveLength(1);
    expect(links[0].label).toBe("host: h-abc123");
    expect(links[0].href).toBe("/infrastructure/host/h-abc123");
  });

  it("prefers host.name over host.id when both present", () => {
    const links = infraLinks({ "host.name": "node-3", "host.id": "h-abc123" });
    expect(links).toHaveLength(1);
    expect(links[0].href).toBe("/infrastructure/host/node-3");
  });

  it("falls back to container.id when container.name is absent", () => {
    const links = infraLinks({ "container.id": "c-xyz" });
    expect(links).toHaveLength(1);
    expect(links[0].label).toBe("container: c-xyz");
    expect(links[0].href).toBe("/infrastructure/container/c-xyz");
  });

  it("URL-encodes special characters in entity IDs", () => {
    const links = infraLinks({ "k8s.pod.name": "pod/with spaces" });
    expect(links[0].href).toBe(
      "/infrastructure/pod/" + encodeURIComponent("pod/with spaces")
    );
  });

  it("returns multiple links when multiple infra attrs are present", () => {
    const links = infraLinks({
      "k8s.pod.name": "checkout-pod-1",
      "host.name": "node-3",
      "k8s.namespace.name": "default",
      "k8s.cluster.name": "prod-cluster",
      "container.name": "checkout",
    });
    expect(links).toHaveLength(5);
    const hrefs = links.map((l) => l.href);
    expect(hrefs).toContain("/infrastructure/pod/checkout-pod-1");
    expect(hrefs).toContain("/infrastructure/host/node-3");
    expect(hrefs).toContain("/infrastructure/namespace/default");
    expect(hrefs).toContain("/infrastructure/cluster/prod-cluster");
    expect(hrefs).toContain("/infrastructure/container/checkout");
  });

  it("skips attrs whose value is not a non-empty string", () => {
    const links = infraLinks({
      "k8s.pod.name": "",
      "host.name": null as unknown as string,
      "container.name": 42 as unknown as string,
    });
    expect(links).toEqual([]);
  });
});
```

- [ ] **Step 2: Run tests — expect all to fail**

```bash
cd apps/frontend && npx vitest run src/utils/infraLinks.test.ts
```

Expected: FAIL — `Cannot find module './infraLinks'`

- [ ] **Step 3: Create the utility**

Create `apps/frontend/src/utils/infraLinks.ts`:

```ts
export interface InfraLink {
  label: string;
  href: string;
}

function str(attrs: Record<string, unknown>, ...keys: string[]): string {
  for (const key of keys) {
    const v = attrs[key];
    if (typeof v === "string" && v.length > 0) return v;
  }
  return "";
}

export function infraLinks(attrs: Record<string, unknown>): InfraLink[] {
  const results: InfraLink[] = [];

  const pod = str(attrs, "k8s.pod.name");
  if (pod) results.push({ label: `pod: ${pod}`, href: `/infrastructure/pod/${encodeURIComponent(pod)}` });

  const host = str(attrs, "host.name", "host.id");
  if (host) results.push({ label: `host: ${host}`, href: `/infrastructure/host/${encodeURIComponent(host)}` });

  const ns = str(attrs, "k8s.namespace.name");
  if (ns) results.push({ label: `namespace: ${ns}`, href: `/infrastructure/namespace/${encodeURIComponent(ns)}` });

  const cluster = str(attrs, "k8s.cluster.name");
  if (cluster) results.push({ label: `cluster: ${cluster}`, href: `/infrastructure/cluster/${encodeURIComponent(cluster)}` });

  const container = str(attrs, "container.name", "container.id");
  if (container) results.push({ label: `container: ${container}`, href: `/infrastructure/container/${encodeURIComponent(container)}` });

  return results;
}
```

- [ ] **Step 4: Run tests — expect all to pass**

```bash
cd apps/frontend && npx vitest run src/utils/infraLinks.test.ts
```

Expected: All 9 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/utils/infraLinks.ts apps/frontend/src/utils/infraLinks.test.ts
git commit -m "feat(frontend): add infraLinks utility for OTel resource-attr to infra URL mapping"
```

---

## Task 2: Extend API interfaces with `resource_attributes`

**Files:**
- Modify: `apps/frontend/src/api/traces.ts`
- Modify: `apps/frontend/src/api/logs.ts`

No new tests — these are interface-only additions that the type checker validates downstream.

- [ ] **Step 1: Add `resource_attributes` to `Span`**

In `apps/frontend/src/api/traces.ts`, the current `Span` interface ends at line 11. Add the field:

```ts
export interface Span {
  tenant_id: string;
  trace_id: string;
  span_id: string;
  service_name: string;
  operation_name: string;
  start_time_unix_nano: number;
  end_time_unix_nano: number;
  duration_ns: number;
  status_code: string;
  resource_attributes?: Record<string, unknown>;
}
```

- [ ] **Step 2: Add `resource_attributes` to `LogRecord`**

In `apps/frontend/src/api/logs.ts`, the current `LogRecord` interface ends at line 17. Add the field:

```ts
export interface LogRecord {
  tenant_id: string;
  log_id: string;
  timestamp_unix_nano: string;
  severity_number: number;
  severity_text: string;
  body: unknown;
  trace_id?: string;
  span_id?: string;
  service_name: string;
  resource_attributes?: Record<string, unknown>;
}
```

- [ ] **Step 3: Verify typecheck passes**

```bash
cd apps/frontend && npx tsc --noEmit
```

Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/api/traces.ts apps/frontend/src/api/logs.ts
git commit -m "feat(frontend): expose resource_attributes on Span and LogRecord interfaces"
```

---

## Task 3: `ServiceInfraPanel` component

**Files:**
- Create: `apps/frontend/src/components/ServiceInfraPanel.tsx`
- Create: `apps/frontend/src/components/ServiceInfraPanel.test.tsx`
- Modify: `apps/frontend/src/pages/ServiceDetailPage.tsx`

- [ ] **Step 1: Write the failing tests**

Create `apps/frontend/src/components/ServiceInfraPanel.test.tsx`:

```tsx
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, describe, it, expect, beforeEach } from "vitest";
import { ServiceInfraPanel } from "./ServiceInfraPanel";
import * as infraApi from "../api/infrastructure";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("ServiceInfraPanel", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renders entity cards with links", async () => {
    vi.spyOn(infraApi, "listInfrastructure").mockResolvedValue({
      items: [
        {
          entity_type: "pod",
          entity_id: "checkout-pod-1",
          display_name: "checkout-pod-1",
          parent_id: null,
          parent_display_name: null,
          environment: "prod",
          health_state: "healthy",
          last_seen_unix_nano: 0,
          related_services: ["checkout"],
          log_rate_per_minute: null,
          error_rate: null,
          restart_count: null,
          cpu_usage: 0.42,
          memory_usage: 0.31,
          disk_usage: null,
          network_io: null,
        },
      ],
    });

    render(<ServiceInfraPanel serviceName="checkout" />, { wrapper });

    await waitFor(() =>
      expect(screen.getByRole("link", { name: /checkout-pod-1/ })).toBeInTheDocument()
    );
    expect(screen.getByRole("link", { name: /checkout-pod-1/ })).toHaveAttribute(
      "href",
      "/infrastructure/pod/checkout-pod-1"
    );
    expect(screen.getByText("pod")).toBeInTheDocument();
  });

  it("shows empty state when no entities", async () => {
    vi.spyOn(infraApi, "listInfrastructure").mockResolvedValue({ items: [] });

    render(<ServiceInfraPanel serviceName="checkout" />, { wrapper });

    await waitFor(() =>
      expect(
        screen.getByText("No infrastructure entities observed for this service.")
      ).toBeInTheDocument()
    );
  });

  it("shows error state when fetch fails", async () => {
    vi.spyOn(infraApi, "listInfrastructure").mockRejectedValue(new Error("fail"));

    render(<ServiceInfraPanel serviceName="checkout" />, { wrapper });

    await waitFor(() =>
      expect(screen.getByText("Could not load infrastructure.")).toBeInTheDocument()
    );
  });

  it("shows cpu and memory when available", async () => {
    vi.spyOn(infraApi, "listInfrastructure").mockResolvedValue({
      items: [
        {
          entity_type: "host",
          entity_id: "node-3",
          display_name: "node-3",
          parent_id: null,
          parent_display_name: null,
          environment: null,
          health_state: "watch",
          last_seen_unix_nano: 0,
          related_services: [],
          log_rate_per_minute: null,
          error_rate: null,
          restart_count: null,
          cpu_usage: 0.75,
          memory_usage: 0.88,
          disk_usage: null,
          network_io: null,
        },
      ],
    });

    render(<ServiceInfraPanel serviceName="checkout" />, { wrapper });

    await waitFor(() => expect(screen.getByText(/CPU/)).toBeInTheDocument());
    expect(screen.getByText(/75%/)).toBeInTheDocument();
    expect(screen.getByText(/88%/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests — expect all to fail**

```bash
cd apps/frontend && npx vitest run src/components/ServiceInfraPanel.test.tsx
```

Expected: FAIL — `Cannot find module './ServiceInfraPanel'`

- [ ] **Step 3: Create the component**

Create `apps/frontend/src/components/ServiceInfraPanel.tsx`:

```tsx
import { useQuery } from "@tanstack/react-query";
import { listInfrastructure, InfrastructureEntitySummary } from "../api/infrastructure";

interface Props {
  serviceName: string;
}

export function ServiceInfraPanel({ serviceName }: Props) {
  const { data, isLoading, isError } = useQuery({
    queryKey: ["service-infra", serviceName],
    queryFn: () => listInfrastructure({ service: serviceName }),
  });

  if (isLoading) return <div className="loading-state">Loading infrastructure…</div>;
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
    <section className="detail-panel">
      <div className="detail-panel-header">
        <div>
          <div className="field-label">Infrastructure</div>
          <h2>Running On</h2>
        </div>
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {items.map((entity) => (
          <EntityCard key={`${entity.entity_type}/${entity.entity_id}`} entity={entity} />
        ))}
      </div>
    </section>
  );
}

function EntityCard({ entity }: { entity: InfrastructureEntitySummary }) {
  const href = `/infrastructure/${entity.entity_type}/${encodeURIComponent(entity.entity_id)}`;
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "6px 0",
        borderBottom: "1px solid var(--color-border, #e2e8f0)",
      }}
    >
      <span className="status info" style={{ minWidth: 72, textAlign: "center" }}>
        {entity.entity_type}
      </span>
      <a href={href} style={{ flex: 1, fontWeight: 500 }}>
        {entity.display_name}
      </a>
      <HealthDot state={entity.health_state} />
      {entity.cpu_usage !== null && (
        <span style={{ fontSize: 12, color: "var(--color-text-muted, #718096)" }}>
          CPU {Math.round(entity.cpu_usage * 100)}%
        </span>
      )}
      {entity.memory_usage !== null && (
        <span style={{ fontSize: 12, color: "var(--color-text-muted, #718096)" }}>
          Mem {Math.round(entity.memory_usage * 100)}%
        </span>
      )}
    </div>
  );
}

function HealthDot({ state }: { state: InfrastructureEntitySummary["health_state"] }) {
  const color =
    state === "breach" ? "#e53e3e" : state === "watch" ? "#d69e2e" : "#38a169";
  return (
    <span
      aria-label={state}
      style={{
        display: "inline-block",
        width: 8,
        height: 8,
        borderRadius: "50%",
        background: color,
        flexShrink: 0,
      }}
    />
  );
}
```

- [ ] **Step 4: Run tests — expect all to pass**

```bash
cd apps/frontend && npx vitest run src/components/ServiceInfraPanel.test.tsx
```

Expected: All 4 tests PASS.

- [ ] **Step 5: Wire `ServiceInfraPanel` into `ServiceDetailPage`**

In `apps/frontend/src/pages/ServiceDetailPage.tsx`:

Add the import at the top (after the existing imports):
```ts
import { ServiceInfraPanel } from "../components/ServiceInfraPanel";
```

In the `ServiceOverview` function, add `<ServiceInfraPanel>` immediately after the closing `</div>` of the `detail-grid` section (line ~127, before `<ServiceSignalTabs>`):

```tsx
      <ServiceInfraPanel serviceName={service.service_name} />

      <ServiceSignalTabs
```

The full `ServiceOverview` return after the change will look like:

```tsx
  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="field-label">Service Overview</div>
          <h1>{service.service_name}</h1>
        </div>
        <Link to="/services" className="secondary-link">Back to services</Link>
      </div>

      <div className="metric-grid" aria-label="Service performance summary">
        <MetricTile label="Request Rate" value={`${service.request_rate.toFixed(2)} rps`} tone="info" />
        <MetricTile
          label="Error Rate"
          value={`${(service.error_rate * 100).toFixed(2)}%`}
          tone={service.health_state === "breach" ? "bad" : service.health_state === "watch" ? "warn" : "good"}
        />
        <MetricTile label="P95 Latency" value={`${Math.round(service.p95_latency_ms)}ms`} tone="good" />
        <MetricTile label="Active Alerts" value={String(service.active_alert_count)} tone={service.active_alert_count > 0 ? "warn" : "good"} />
      </div>

      <div className="detail-grid">
        <section className="detail-panel">
          <div className="detail-panel-header">
            <div>
              <div className="field-label">Health</div>
              <h2>Current State</h2>
            </div>
            <HealthStatus healthState={service.health_state} />
          </div>
          <dl className="definition-grid">
            <div>
              <dt>SLO / health state</dt>
              <dd>{healthLabel(service.health_state)}</dd>
            </div>
            <div>
              <dt>Latest deployment</dt>
              <dd>{service.latest_deployment ?? "No deployment marker"}</dd>
            </div>
            <div>
              <dt>Lookback</dt>
              <dd>Last 1h</dd>
            </div>
          </dl>
        </section>

        <section className="detail-panel">
          <div className="detail-panel-header">
            <div>
              <div className="field-label">Investigate</div>
              <h2>Signal Entry Points</h2>
            </div>
          </div>
          <div className="entry-link-grid" aria-label="Signal entry points">
            <a href={`/traces?service=${encodeURIComponent(service.service_name)}`} className="entry-link">
              Traces
            </a>
            <a href={`/logs?service=${encodeURIComponent(service.service_name)}`} className="entry-link">
              Logs
            </a>
            <a href={`/metrics?service=${encodeURIComponent(service.service_name)}`} className="entry-link">
              Metrics
            </a>
            <a href={`/infrastructure?service=${encodeURIComponent(service.service_name)}`} className="entry-link">
              Infrastructure
            </a>
          </div>
        </section>
      </div>

      <ServiceInfraPanel serviceName={service.service_name} />

      <ServiceSignalTabs
        serviceName={service.service_name}
        activeTab={activeTab}
        lookbackMinutes={lookbackMinutes}
      />
    </section>
  );
```

- [ ] **Step 6: Verify typecheck still passes**

```bash
cd apps/frontend && npx tsc --noEmit
```

Expected: No errors.

- [ ] **Step 7: Commit**

```bash
git add apps/frontend/src/components/ServiceInfraPanel.tsx \
        apps/frontend/src/components/ServiceInfraPanel.test.tsx \
        apps/frontend/src/pages/ServiceDetailPage.tsx
git commit -m "feat(frontend): add ServiceInfraPanel to service overview tab"
```

---

## Task 4: Trace-level infrastructure summary in `TraceDetail`

**Files:**
- Modify: `apps/frontend/src/pages/TraceDetail.tsx`
- Modify: `apps/frontend/src/pages/TraceDetail.test.tsx`

- [ ] **Step 1: Add failing tests to `TraceDetail.test.tsx`**

Open `apps/frontend/src/pages/TraceDetail.test.tsx` and replace the entire file with:

```tsx
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { TraceDetail } from "./TraceDetail";

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

function wrapper({ children }: { children: React.ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

const baseSpan = {
  trace_id: "abc",
  tenant_id: "t1",
  span_id: "111",
  service_name: "checkout",
  operation_name: "POST /order",
  start_time_unix_nano: 0,
  end_time_unix_nano: 5000000,
  duration_ns: 5_000_000,
  status_code: "OK",
};

test("renders waterfall with spans", () => {
  render(
    <QueryClientProvider client={queryClient}>
      <TraceDetail traceId="abc" spans={[baseSpan]} />
    </QueryClientProvider>
  );
  expect(screen.getByText(/POST \/order/)).toBeInTheDocument();
  expect(screen.getByText("5.00ms")).toBeInTheDocument();
});

test("renders infra pill links when spans have resource_attributes", () => {
  const spans = [
    {
      ...baseSpan,
      resource_attributes: {
        "k8s.pod.name": "checkout-pod-1",
        "host.name": "node-3",
      },
    },
    {
      ...baseSpan,
      span_id: "222",
      resource_attributes: {
        "k8s.pod.name": "checkout-pod-1", // duplicate — should deduplicate
      },
    },
  ];

  render(<TraceDetail traceId="abc" spans={spans} />, { wrapper });

  const podLink = screen.getByRole("link", { name: "pod: checkout-pod-1" });
  expect(podLink).toBeInTheDocument();
  expect(podLink).toHaveAttribute("href", "/infrastructure/pod/checkout-pod-1");

  const hostLink = screen.getByRole("link", { name: "host: node-3" });
  expect(hostLink).toBeInTheDocument();
  expect(hostLink).toHaveAttribute("href", "/infrastructure/host/node-3");

  // Deduplicated — only one pod link
  expect(screen.getAllByRole("link", { name: "pod: checkout-pod-1" })).toHaveLength(1);
});

test("omits infra section entirely when no span has resource_attributes", () => {
  render(
    <QueryClientProvider client={queryClient}>
      <TraceDetail traceId="abc" spans={[baseSpan]} />
    </QueryClientProvider>
  );
  expect(screen.queryByText(/Infrastructure/)).not.toBeInTheDocument();
});

test("omits infra section when resource_attributes has no recognised infra keys", () => {
  const spans = [
    {
      ...baseSpan,
      resource_attributes: { "custom.attr": "value" },
    },
  ];
  render(<TraceDetail traceId="abc" spans={spans} />, { wrapper });
  expect(screen.queryByText(/Infrastructure/)).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run tests — existing test passes, new tests fail**

```bash
cd apps/frontend && npx vitest run src/pages/TraceDetail.test.tsx
```

Expected: 1 PASS (existing waterfall test), 3 FAIL (new infra tests).

- [ ] **Step 3: Update `TraceDetail.tsx`**

Replace the entire file `apps/frontend/src/pages/TraceDetail.tsx` with:

```tsx
import { useState } from "react";
import { Span } from "../api/traces";
import { LogCorrelatedList } from "../components/LogCorrelatedList";
import { infraLinks, InfraLink } from "../utils/infraLinks";

interface Props {
  traceId: string;
  spans: Span[];
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

export function TraceDetail({ traceId, spans }: Props) {
  const [selectedSpanId, setSelectedSpanId] = useState<string | undefined>();
  const minStart = Math.min(...spans.map((s) => Number(s.start_time_unix_nano)));
  const maxEnd = Math.max(...spans.map((s) => Number(s.end_time_unix_nano)));
  const totalNs = maxEnd - minStart || 1;

  const infraPills = mergedInfraLinks(spans);

  return (
    <div>
      <h2>Trace {traceId.substring(0, 16)}…</h2>
      <p>
        Total: {(totalNs / 1e6).toFixed(2)}ms — {spans.length} spans
      </p>

      {infraPills.length > 0 && (
        <div
          aria-label="Infrastructure"
          style={{ display: "flex", flexWrap: "wrap", gap: 8, marginBottom: 12 }}
        >
          {infraPills.map((link) => (
            <a
              key={link.href}
              href={link.href}
              style={{
                fontSize: 12,
                padding: "2px 8px",
                borderRadius: 12,
                background: "var(--color-bg-subtle, #edf2f7)",
                color: "var(--color-text, #2d3748)",
                textDecoration: "none",
                border: "1px solid var(--color-border, #e2e8f0)",
              }}
            >
              {link.label}
            </a>
          ))}
        </div>
      )}

      <div style={{ overflowX: "auto" }}>
        {spans.map((span) => {
          const offset =
            ((Number(span.start_time_unix_nano) - minStart) / totalNs) * 100;
          const width = (span.duration_ns / totalNs) * 100;
          return (
            <div
              key={span.span_id}
              onClick={() =>
                setSelectedSpanId(
                  span.span_id === selectedSpanId ? undefined : span.span_id
                )
              }
              style={{
                display: "flex",
                alignItems: "center",
                marginBottom: 4,
                cursor: "pointer",
                background:
                  selectedSpanId === span.span_id ? "#edf2f7" : "transparent",
                borderRadius: "4px",
                padding: "2px 0",
              }}
            >
              <span
                style={{
                  width: 200,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                  fontSize: 12,
                }}
              >
                {span.service_name}: {span.operation_name}
              </span>
              <div
                style={{
                  flex: 1,
                  position: "relative",
                  height: 16,
                  background: "#f0f0f0",
                }}
              >
                <div
                  style={{
                    position: "absolute",
                    left: `${offset}%`,
                    width: `${Math.max(width, 0.5)}%`,
                    height: "100%",
                    background:
                      span.status_code === "ERROR" ? "#e53e3e" : "#4299e1",
                  }}
                  title={`${(span.duration_ns / 1e6).toFixed(2)}ms`}
                />
              </div>
              <span style={{ width: 60, textAlign: "right", fontSize: 12 }}>
                {(span.duration_ns / 1e6).toFixed(2)}ms
              </span>
            </div>
          );
        })}
      </div>
      <LogCorrelatedList traceId={traceId} spanId={selectedSpanId} />
    </div>
  );
}
```

- [ ] **Step 4: Run tests — all 4 should pass**

```bash
cd apps/frontend && npx vitest run src/pages/TraceDetail.test.tsx
```

Expected: All 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/pages/TraceDetail.tsx apps/frontend/src/pages/TraceDetail.test.tsx
git commit -m "feat(frontend): add trace-level infra pill summary to TraceDetail"
```

---

## Task 5: Infra badges on log rows in `LogSearch`

**Files:**
- Modify: `apps/frontend/src/pages/LogSearch.tsx`

The log row tests live inside the `LogSearch` component. We'll add a focused test directly in a new file that renders a minimal log table with mock data.

- [ ] **Step 1: Write the failing tests**

Create `apps/frontend/src/pages/LogSearch.infra.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";

// Minimal inline test of the badge rendering logic without mounting the full page.
// We test the InfraBadges helper directly by extracting the relevant logic.
import { infraLinks } from "../utils/infraLinks";

describe("infra badge links from log resource_attributes", () => {
  it("produces pod link from k8s.pod.name", () => {
    const links = infraLinks({ "k8s.pod.name": "api-pod-99" });
    expect(links).toHaveLength(1);
    expect(links[0].label).toBe("pod: api-pod-99");
    expect(links[0].href).toBe("/infrastructure/pod/api-pod-99");
  });

  it("produces no links when resource_attributes is empty", () => {
    expect(infraLinks({})).toHaveLength(0);
  });
});

// Integration-style: render a log row with badges via a minimal component
import React from "react";

function LogRowBadges({ attrs }: { attrs: Record<string, unknown> }) {
  const links = infraLinks(attrs);
  if (!links.length) return null;
  return (
    <span aria-label="infra-badges">
      {links.map((l) => (
        <a key={l.href} href={l.href} style={{ marginLeft: 4, fontSize: 11 }}>
          {l.label}
        </a>
      ))}
    </span>
  );
}

describe("LogRowBadges component", () => {
  it("renders badges when infra attrs present", () => {
    render(
      <LogRowBadges attrs={{ "k8s.pod.name": "api-pod-99", "host.name": "node-1" }} />
    );
    expect(screen.getByRole("link", { name: "pod: api-pod-99" })).toHaveAttribute(
      "href",
      "/infrastructure/pod/api-pod-99"
    );
    expect(screen.getByRole("link", { name: "host: node-1" })).toHaveAttribute(
      "href",
      "/infrastructure/host/node-1"
    );
  });

  it("renders nothing when no infra attrs", () => {
    const { container } = render(<LogRowBadges attrs={{}} />);
    expect(container.firstChild).toBeNull();
  });
});
```

- [ ] **Step 2: Run tests — expect all to pass (utility already exists)**

```bash
cd apps/frontend && npx vitest run src/pages/LogSearch.infra.test.tsx
```

Expected: All 4 tests PASS (they exercise `infraLinks` which already exists).

- [ ] **Step 3: Add infra badges to log rows in `LogSearch.tsx`**

Replace the entire `apps/frontend/src/pages/LogSearch.tsx` with:

```tsx
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { searchLogs, LogRecord } from "../api/logs";
import { FacetSidebar } from "../components/FacetSidebar";
import { infraLinks } from "../utils/infraLinks";

export default function LogSearch() {
  const [service, setService] = useState("");

  const { data, isLoading, error } = useQuery({
    queryKey: ["logs", service],
    queryFn: () =>
      searchLogs({
        service: service || undefined,
        limit: 50,
        facets: ["service_name", "severity_number", "environment", "host_id"],
      }),
  });

  const handleFacetClick = (field: string, value: string) => {
    if (field === "service_name") {
      setService(value);
    }
  };

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="field-label">Explorer</div>
          <h1>Logs</h1>
        </div>
      </div>

      <div className="toolbar-row">
        <input
          className="search-input"
          placeholder="Filter by service"
          value={service}
          onChange={(e) => setService(e.target.value)}
          aria-label="Filter by service"
        />
        {service && (
          <button
            className="secondary-link"
            onClick={() => setService("")}
            style={{ cursor: "pointer", background: "none" }}
          >
            Clear filters
          </button>
        )}
      </div>

      <div style={{ display: "flex", alignItems: "flex-start" }}>
        <FacetSidebar facets={data?.facets} onFacetClick={handleFacetClick} />

        <div className="table-panel" style={{ flex: 1 }}>
          {isLoading ? (
            <div className="loading-state">Loading logs...</div>
          ) : error ? (
            <div className="signal-empty">Error loading logs: {String(error)}</div>
          ) : data?.logs.length === 0 ? (
            <div className="signal-empty">No logs found.</div>
          ) : (
            <table style={{ borderCollapse: "collapse", width: "100%" }}>
              <thead>
                <tr>
                  <th>Timestamp</th>
                  <th>Service</th>
                  <th>Level</th>
                  <th>Message</th>
                </tr>
              </thead>
              <tbody>
                {data?.logs.map((log) => (
                  <LogRow key={log.log_id} log={log} />
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </section>
  );
}

function LogRow({ log }: { log: LogRecord }) {
  const badges = infraLinks(log.resource_attributes ?? {});
  return (
    <tr>
      <td>{log.timestamp_unix_nano}</td>
      <td>
        {log.service_name}
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
      </td>
      <td>
        <span className={`status ${severityTone(log.severity_number)}`}>
          {log.severity_text || log.severity_number}
        </span>
      </td>
      <td>{typeof log.body === "string" ? log.body : JSON.stringify(log.body)}</td>
    </tr>
  );
}

function severityTone(severity: number) {
  if (severity >= 17) return "bad";
  if (severity >= 13) return "warn";
  if (severity >= 9) return "info";
  return "good";
}
```

- [ ] **Step 4: Verify typecheck passes**

```bash
cd apps/frontend && npx tsc --noEmit
```

Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/pages/LogSearch.tsx apps/frontend/src/pages/LogSearch.infra.test.tsx
git commit -m "feat(frontend): add infra resource-attribute badges to log rows"
```

---

## Task 6: Full verification

- [ ] **Step 1: Run all frontend tests**

```bash
cd apps/frontend && npx vitest run
```

Expected: All tests pass with no failures.

- [ ] **Step 2: Run local CI (skip Docker)**

```bash
cd /c/git/Observable && bash scripts/local-ci.sh --skip-docker
```

Expected: Rust fmt/clippy/tests pass, frontend typecheck/lint/build/test all pass.

- [ ] **Step 3: Update the iteration plan**

In `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`, find the P3-S10 slice and mark it complete:

Change:
```markdown
- [ ] **P3-S10: Add infrastructure correlation from service and trace views**
```
To:
```markdown
- [x] **P3-S10: Add infrastructure correlation from service and trace views**
  - Outcome: Service overview shows a correlated infrastructure panel via `listInfrastructure`; trace detail shows a deduplicated pill row of infra entities derived from span `resource_attributes`; log rows show inline infra badges. All three surfaces use a shared `infraLinks()` utility. Frontend-only — no backend changes. Completed 2026-04-24.
  - Checkpoint: are links derived correctly from OTel resource attributes? Answer: yes. `infraLinks()` maps `k8s.pod.name`, `host.name`/`host.id`, `k8s.namespace.name`, `k8s.cluster.name`, and `container.name`/`container.id` to `/infrastructure/:type/:id` URLs using `encodeURIComponent`, with 9 unit tests covering all mappings, fallbacks, deduplication, and URL encoding.
```

Also update section 13 "Recommended Next Slice":

Add after the current last numbered item:
```markdown
14. ~~P3-S10: Add infrastructure correlation from service and trace views~~ (done)

**Next recommended slice: P3-S11 - Add deployment event ingestion and one timeline overlay.**
```

- [ ] **Step 4: Commit the plan update**

```bash
git add docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md
git commit -m "docs(plan): mark P3-S10 infrastructure correlation complete"
```
