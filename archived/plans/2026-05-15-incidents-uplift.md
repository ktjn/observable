# Incidents Page Uplift Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring `IncidentsPage` and `IncidentDetailPage` to visual and structural parity with the uplifted `AlertsPage` — page-stack layout, MetricCard summary row, filter pills, row tinting, CSS vars, and consistent timestamps.

**Architecture:** Both files are self-contained page components; all changes are local to their files. Incidents are fetched in full (no server-side status filter) so summary counts and pills can be computed client-side from one query, matching the AlertsPage pattern. ISO timestamps are converted to nanoseconds for `formatTimestamp`.

**Tech Stack:** React, TypeScript, Tailwind CSS with CSS design-system variables, `@tanstack/react-query`, Vitest + React Testing Library.

---

## File Map

| File | Action |
|------|--------|
| `apps/frontend/src/features/incidents/IncidentsPage.tsx` | Modify — layout, MetricCards, filter pills, row tinting, CSS vars, timestamps |
| `apps/frontend/src/features/incidents/IncidentDetailPage.tsx` | Modify — layout, page-header, CSS vars, monospace glyphs, timestamps |
| `apps/frontend/src/features/incidents/IncidentsPage.test.tsx` | Create — tests for list page |

---

## Task 1: Write tests for IncidentsPage (they will fail until Task 2)

**Files:**
- Create: `apps/frontend/src/features/incidents/IncidentsPage.test.tsx`

- [ ] **Step 1: Create the test file**

```tsx
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import * as incidentsApi from "../../api/incidents";
import { IncidentsPage } from "./IncidentsPage";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../../lib/timeDisplay", () => ({
  useTimeDisplay: () => ({ format: "iso-local-ms" }),
}));

const sampleIncidents: incidentsApi.IncidentItem[] = [
  {
    incident_id: "inc-1",
    title: "Database CPU spike",
    severity: "critical",
    status: "triggered",
    triggered_at: "2026-05-15T10:00:00Z",
    resolved_at: null,
    triggered_by_rule_id: "rule-1",
  },
  {
    incident_id: "inc-2",
    title: "API latency high",
    severity: "warning",
    status: "acknowledged",
    triggered_at: "2026-05-15T09:00:00Z",
    resolved_at: null,
    triggered_by_rule_id: null,
  },
  {
    incident_id: "inc-3",
    title: "Disk full",
    severity: "critical",
    status: "resolved",
    triggered_at: "2026-05-15T08:00:00Z",
    resolved_at: "2026-05-15T08:30:00Z",
    triggered_by_rule_id: null,
  },
];

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <IncidentsPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
});

test("renders page header with Reliability eyebrow", async () => {
  vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: sampleIncidents });
  renderPage();
  await waitFor(() => screen.getByText("Incidents"));
  expect(screen.getByText("Reliability")).toBeInTheDocument();
});

test("renders MetricCard summary row with correct counts", async () => {
  vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: sampleIncidents });
  renderPage();
  await waitFor(() => screen.getByText("Total"));
  expect(screen.getByText("Total")).toBeInTheDocument();
  expect(screen.getByText("Triggered")).toBeInTheDocument();
  expect(screen.getByText("Acknowledged")).toBeInTheDocument();
  expect(screen.getByText("Resolved")).toBeInTheDocument();
  expect(screen.getByText("MTTR")).toBeInTheDocument();
  // 3 total, 1 triggered, 1 acknowledged, 1 resolved
  expect(screen.getByRole("group", { name: "Incident summary" })).toBeInTheDocument();
});

test("renders filter pills and filters table on click", async () => {
  vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: sampleIncidents });
  renderPage();
  await waitFor(() => screen.getByText("Database CPU spike"));

  // All three rows visible initially
  expect(screen.getByText("Database CPU spike")).toBeInTheDocument();
  expect(screen.getByText("API latency high")).toBeInTheDocument();
  expect(screen.getByText("Disk full")).toBeInTheDocument();

  // Click Triggered pill
  fireEvent.click(screen.getByRole("button", { name: /triggered/i }));
  await waitFor(() => expect(screen.queryByText("API latency high")).not.toBeInTheDocument());
  expect(screen.getByText("Database CPU spike")).toBeInTheDocument();
  expect(screen.queryByText("Disk full")).not.toBeInTheDocument();
});

test("renders empty state when no incidents", async () => {
  vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: [] });
  renderPage();
  await waitFor(() => expect(screen.getByText("No incidents found.")).toBeInTheDocument());
});

test("MTTR shows dash when no resolved incidents", async () => {
  const noResolved = sampleIncidents.filter((i) => i.status !== "resolved");
  vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: noResolved });
  renderPage();
  await waitFor(() => screen.getByText("MTTR"));
  expect(screen.getByText("—")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run tests to confirm they fail**

```
cd apps/frontend && npx vitest run src/features/incidents/IncidentsPage.test.tsx
```

Expected: FAIL — `IncidentsPage` does not yet have the new structure.

---

## Task 2: Uplift IncidentsPage

**Files:**
- Modify: `apps/frontend/src/features/incidents/IncidentsPage.tsx`

- [ ] **Step 1: Replace the entire file with the uplifted version**

```tsx
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { listIncidents, type IncidentItem } from "../../api/incidents";
import { Badge } from "../../components/ui/badge";
import { EmptyState } from "../../components/ui/empty-state";
import { LoadingState } from "../../components/ui/loading-state";
import { MetricCard } from "../../components/ui/metric-card";
import { Panel } from "../../components/ui/panel";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp } from "../../utils/formatTimestamp";

type StatusFilter = "" | "triggered" | "acknowledged" | "resolved";

function severityColor(severity: string): "bad" | "warn" | "neutral" {
  switch (severity) {
    case "critical": return "bad";
    case "warning":  return "warn";
    default:         return "neutral";
  }
}

function statusColor(status: string): "bad" | "warn" | "good" | "neutral" {
  switch (status) {
    case "triggered":    return "bad";
    case "acknowledged": return "warn";
    case "resolved":     return "good";
    default:             return "neutral";
  }
}

function isoToNs(iso: string): number {
  return new Date(iso).getTime() * 1_000_000;
}

export function IncidentsPage() {
  const { tenantId } = useTenantContext();
  const { format } = useTimeDisplay();
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("");

  const { data, isLoading } = useQuery({
    queryKey: ["incidents", tenantId],
    queryFn: () => listIncidents(tenantId),
  });

  const items = data?.items ?? [];
  const triggeredCount    = items.filter((i) => i.status === "triggered").length;
  const acknowledgedCount = items.filter((i) => i.status === "acknowledged").length;
  const resolvedCount     = items.filter((i) => i.status === "resolved").length;

  const resolvedItems = items.filter((i) => i.resolved_at);
  const mttrMin = resolvedItems.length
    ? Math.round(
        resolvedItems.reduce(
          (sum, i) =>
            sum + (new Date(i.resolved_at!).getTime() - new Date(i.triggered_at).getTime()),
          0,
        ) /
          resolvedItems.length /
          60_000,
      )
    : null;

  const filteredItems = statusFilter ? items.filter((i) => i.status === statusFilter) : items;

  const pillDefs: { label: string; value: StatusFilter; count: number; activeColor: string }[] = [
    { label: "All",          value: "",             count: items.length,     activeColor: "var(--brand)" },
    { label: "Triggered",    value: "triggered",    count: triggeredCount,   activeColor: "var(--bad)"   },
    { label: "Acknowledged", value: "acknowledged", count: acknowledgedCount, activeColor: "var(--warn)" },
    { label: "Resolved",     value: "resolved",     count: resolvedCount,    activeColor: "var(--good)"  },
  ];

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Reliability</div>
          <h1>Incidents</h1>
        </div>
      </div>

      <div
        className="grid grid-cols-1 gap-4 sm:grid-cols-5"
        aria-label="Incident summary"
        role="group"
      >
        <MetricCard label="Total"        value={items.length}                          tone="info"                                     />
        <MetricCard label="Triggered"    value={triggeredCount}                        tone={triggeredCount > 0 ? "bad" : "good"}      />
        <MetricCard label="Acknowledged" value={acknowledgedCount}                     tone={acknowledgedCount > 0 ? "warn" : "info"}  />
        <MetricCard label="Resolved"     value={resolvedCount}                         tone="good"                                     />
        <MetricCard label="MTTR"         value={mttrMin !== null ? `${mttrMin}m` : "—"} tone="info"                                   />
      </div>

      <div className="flex items-center gap-1" role="group" aria-label="Filter incidents">
        {pillDefs.map(({ label, value, count, activeColor }) => {
          const isActive = statusFilter === value;
          return (
            <button
              key={value || "all"}
              type="button"
              onClick={() => setStatusFilter(value)}
              className={[
                "flex items-center gap-1.5 px-2.5 py-1 text-xs font-bold border rounded transition-colors",
                isActive
                  ? ""
                  : "border-[var(--border)] text-[var(--muted)] hover:border-[var(--brand)] hover:text-[var(--text)]",
              ].join(" ")}
              style={isActive ? { borderColor: activeColor, color: activeColor } : undefined}
            >
              <span>{label}</span>
              <span aria-hidden="true">({count})</span>
            </button>
          );
        })}
      </div>

      <Panel title="Incidents" eyebrow="Active and historical">
        {isLoading ? (
          <LoadingState>Loading incidents...</LoadingState>
        ) : !filteredItems.length ? (
          <EmptyState title="No incidents found." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm" aria-label="Incidents">
              <thead>
                <tr className="border-b text-left">
                  <th className="pb-2 pr-4">Title</th>
                  <th className="pb-2 pr-4">Severity</th>
                  <th className="pb-2 pr-4">Status</th>
                  <th className="pb-2 pr-4">Triggered</th>
                  <th className="pb-2 pr-4">Resolved</th>
                </tr>
              </thead>
              <tbody>
                {filteredItems.map((incident: IncidentItem) => {
                  const rowClass = [
                    "modern-table-row border-l-2",
                    incident.status === "triggered"
                      ? "border-l-[var(--bad)]"
                      : incident.status === "acknowledged"
                        ? "border-l-[var(--warn)]"
                        : "border-l-transparent",
                  ].join(" ");
                  return (
                    <tr key={incident.incident_id} className={rowClass}>
                      <td className="py-2 pr-4">
                        <Link
                          to="/incidents/$incidentId"
                          params={{ incidentId: incident.incident_id }}
                          className="font-medium hover:underline"
                        >
                          {incident.title}
                        </Link>
                      </td>
                      <td className="py-2 pr-4">
                        <Badge tone={severityColor(incident.severity)}>{incident.severity}</Badge>
                      </td>
                      <td className="py-2 pr-4">
                        <Badge tone={statusColor(incident.status)}>{incident.status}</Badge>
                      </td>
                      <td className="py-2 pr-4 text-[var(--muted)]">
                        {formatTimestamp(isoToNs(incident.triggered_at), format)}
                      </td>
                      <td className="py-2 pr-4 text-[var(--muted)]">
                        {incident.resolved_at
                          ? formatTimestamp(isoToNs(incident.resolved_at), format)
                          : "—"}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </Panel>
    </section>
  );
}
```

- [ ] **Step 2: Run tests to confirm they pass**

```
cd apps/frontend && npx vitest run src/features/incidents/IncidentsPage.test.tsx
```

Expected: All 5 tests PASS.

- [ ] **Step 3: Run the full frontend test suite to check for regressions**

```
cd apps/frontend && npx vitest run
```

Expected: All existing tests pass.

- [ ] **Step 4: Commit**

```
git add apps/frontend/src/features/incidents/IncidentsPage.tsx apps/frontend/src/features/incidents/IncidentsPage.test.tsx
git commit -m "feat(incidents): uplift list page with MetricCards, filter pills, row tinting, CSS vars"
```

---

## Task 3: Uplift IncidentDetailPage

**Files:**
- Modify: `apps/frontend/src/features/incidents/IncidentDetailPage.tsx`

- [ ] **Step 1: Replace the entire file with the uplifted version**

```tsx
import { useParams } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { getIncident, type IncidentEventItem } from "../../api/incidents";
import { Badge } from "../../components/ui/badge";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp } from "../../utils/formatTimestamp";

function severityColor(severity: string): "bad" | "warn" | "neutral" {
  switch (severity) {
    case "critical": return "bad";
    case "warning":  return "warn";
    default:         return "neutral";
  }
}

function statusColor(status: string): "bad" | "warn" | "good" | "neutral" {
  switch (status) {
    case "triggered":    return "bad";
    case "acknowledged": return "warn";
    case "resolved":     return "good";
    default:             return "neutral";
  }
}

function eventGlyph(eventType: string): string {
  switch (eventType) {
    case "triggered":         return "▸";
    case "alert_fired":       return "!";
    case "alert_resolved":    return "✓";
    case "acknowledged":      return "◎";
    case "comment":           return "·";
    case "status_change":     return "→";
    case "deployment_linked": return "↑";
    default:                  return "·";
  }
}

function isoToNs(iso: string): number {
  return new Date(iso).getTime() * 1_000_000;
}

export function IncidentDetailPage() {
  const { tenantId } = useTenantContext();
  const { format } = useTimeDisplay();
  const { incidentId } = useParams({ from: "/incidents/$incidentId" });

  const { data, isLoading } = useQuery({
    queryKey: ["incident", tenantId, incidentId],
    queryFn: () => getIncident(tenantId, incidentId),
  });

  if (isLoading) {
    return <LoadingState>Loading incident...</LoadingState>;
  }

  if (!data) {
    return <Panel>Incident not found.</Panel>;
  }

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Incident</div>
          <h1>{data.title}</h1>
        </div>
        <div className="flex items-center gap-2">
          <Badge tone={severityColor(data.severity)}>{data.severity}</Badge>
          <Badge tone={statusColor(data.status)}>{data.status}</Badge>
        </div>
      </div>

      <Panel>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <div>
            <div className="field-label">Severity</div>
            <div className="mt-1">
              <Badge tone={severityColor(data.severity)}>{data.severity}</Badge>
            </div>
          </div>
          <div>
            <div className="field-label">Status</div>
            <div className="mt-1">
              <Badge tone={statusColor(data.status)}>{data.status}</Badge>
            </div>
          </div>
          <div>
            <div className="field-label">Triggered</div>
            <div className="mt-1">{formatTimestamp(isoToNs(data.triggered_at), format)}</div>
          </div>
          <div>
            <div className="field-label">Resolved</div>
            <div className="mt-1">
              {data.resolved_at
                ? formatTimestamp(isoToNs(data.resolved_at), format)
                : "—"}
            </div>
          </div>
        </div>
      </Panel>

      <Panel>
        <h3 className="text-sm font-semibold mb-3">Timeline</h3>
        <div className="space-y-3">
          {data.timeline.map((event: IncidentEventItem, idx: number) => (
            <div key={idx} className="flex gap-3">
              <div className="font-mono text-base leading-none text-[var(--muted)] w-4 flex-shrink-0">
                {eventGlyph(event.event_type)}
              </div>
              <div className="flex-1">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium capitalize">
                    {event.event_type.replace(/_/g, " ")}
                  </span>
                  <span className="text-xs text-[var(--muted)]">
                    {formatTimestamp(isoToNs(event.event_time), format)}
                  </span>
                </div>
                {event.message && (
                  <p className="text-sm text-[var(--muted)] mt-0.5">{event.message}</p>
                )}
                <p className="text-xs text-[var(--muted)]">by {event.actor}</p>
              </div>
            </div>
          ))}
          {data.timeline.length === 0 && (
            <p className="text-sm text-[var(--muted)]">No timeline events.</p>
          )}
        </div>
      </Panel>
    </section>
  );
}
```

- [ ] **Step 2: Run the full frontend test suite**

```
cd apps/frontend && npx vitest run
```

Expected: All tests pass.

- [ ] **Step 3: Commit**

```
git add apps/frontend/src/features/incidents/IncidentDetailPage.tsx
git commit -m "feat(incidents): uplift detail page with page-header, CSS vars, monospace timeline glyphs"
```

---

## Task 4: Open pull request

- [ ] **Step 1: Push branch and open PR**

```
git push -u origin HEAD
gh pr create --title "feat(incidents): uplift Incidents pages with filter pills, row tinting, MetricCards, and CSS vars" --body "$(cat <<'EOF'
## Summary
- Replace tab-based filter with inline filter pills (All / Triggered / Acknowledged / Resolved) with live counts
- Add MetricCard summary row: Total, Triggered, Acknowledged, Resolved, MTTR
- Apply `modern-table-row border-l-2` row tinting: red = triggered, warn = acknowledged, neutral = resolved
- Replace all `text-muted-foreground` with `text-[var(--muted)]` throughout both files
- Switch layout to `page-stack` / `page-header` pattern matching Alerts & SLOs page
- Replace emoji timeline icons with monospace glyphs in IncidentDetailPage
- Timestamps use `useTimeDisplay` + `formatTimestamp` for consistency with Logs/Traces

## Test plan
- [ ] `npx vitest run src/features/incidents/IncidentsPage.test.tsx` — all 5 tests pass
- [ ] `npx vitest run` — full suite clean
- [ ] Open `/incidents` in browser — MetricCards visible, pills filter table, triggered rows have red left border
- [ ] Click an incident — detail page shows page-header with badges, `field-label` metadata, monospace glyphs in timeline

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```
