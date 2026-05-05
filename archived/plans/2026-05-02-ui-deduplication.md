# UI Deduplication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate duplicated layout, state, and rendering logic across the signal explorers and log-list components, leaving `LogExplorer` and `TraceExplorer` as thin wrappers around a shared `SignalExplorer` shell.

**Architecture:** `SignalExplorer` owns toolbar layout (service input, lookback select, custom range display, promote button) and the left panel / table split. Wrappers own data fetching, histogram rendering, and domain-specific table/panel components. A new `LogList` component unifies the monospace log row renderer used by `LogContextView`, `LogCorrelatedList`, and the log detail panel.

**Tech Stack:** React 18, TypeScript, Vitest, @testing-library/react, @tanstack/react-query, Tailwind CSS

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| Move | `features/signals/LogResultsTable.tsx` → `features/signals/components/` | Log results table |
| Move | `features/signals/TraceResultsTable.tsx` → `features/signals/components/` | Trace results table (gains Time column) |
| Create | `components/shared/LogList.tsx` | Shared mono log-row renderer |
| Create | `components/shared/SignalExplorer.tsx` | Shared explorer shell (toolbar + panel/table layout) |
| Create | `utils/formatBucketLabel.ts` | Extracted duplicate helper (used by both explorers) |
| Modify | `components/LogContextView.tsx` | Delegate row rendering to LogList |
| Modify | `components/LogCorrelatedList.tsx` | Delegate row rendering to LogList |
| Modify | `pages/LogSearch.tsx` | Thin wrapper using SignalExplorer |
| Modify | `pages/TraceSearch.tsx` | Thin wrapper using SignalExplorer |
| Modify | `pages/view-unification.test.ts` | Add SignalExplorer assertions |
| Delete | `components/shared/SignalExplorerLayout.tsx` | Superseded |

---

## Task 1: Fix directory structure

**Files:**
- Move: `apps/frontend/src/features/signals/LogResultsTable.tsx` → `apps/frontend/src/features/signals/components/LogResultsTable.tsx`
- Move: `apps/frontend/src/features/signals/TraceResultsTable.tsx` → `apps/frontend/src/features/signals/components/TraceResultsTable.tsx`
- Modify: `apps/frontend/src/pages/LogSearch.tsx` (import path)
- Modify: `apps/frontend/src/pages/TraceSearch.tsx` (import path)

- [ ] **Step 1: Move the files**

```powershell
New-Item -ItemType Directory -Force apps/frontend/src/features/signals/components
Move-Item apps/frontend/src/features/signals/LogResultsTable.tsx apps/frontend/src/features/signals/components/LogResultsTable.tsx
Move-Item apps/frontend/src/features/signals/TraceResultsTable.tsx apps/frontend/src/features/signals/components/TraceResultsTable.tsx
Move-Item apps/frontend/src/features/signals/LogResultsTable.test.tsx apps/frontend/src/features/signals/components/LogResultsTable.test.tsx
Move-Item apps/frontend/src/features/signals/TraceResultsTable.test.tsx apps/frontend/src/features/signals/components/TraceResultsTable.test.tsx
```

- [ ] **Step 2: Update import in LogSearch.tsx**

In `apps/frontend/src/pages/LogSearch.tsx`, change:
```ts
import { LogResultsTable } from "../features/signals/LogResultsTable";
```
to:
```ts
import { LogResultsTable } from "../features/signals/components/LogResultsTable";
```

- [ ] **Step 3: Update import in TraceSearch.tsx**

In `apps/frontend/src/pages/TraceSearch.tsx`, change:
```ts
import { TraceResultsTable } from "../features/signals/TraceResultsTable";
```
to:
```ts
import { TraceResultsTable } from "../features/signals/components/TraceResultsTable";
```

- [ ] **Step 4: Verify typecheck passes**

```powershell
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/features/signals/
git add apps/frontend/src/pages/LogSearch.tsx apps/frontend/src/pages/TraceSearch.tsx
git commit -m "refactor: move signal result tables to features/signals/components/"
```

---

## Task 2: Add start-time date column to TraceResultsTable

**Files:**
- Modify: `apps/frontend/src/features/signals/components/TraceResultsTable.tsx`
- Modify: `apps/frontend/src/features/signals/components/TraceResultsTable.test.tsx`
- Modify: `apps/frontend/src/pages/TraceSearch.tsx`

- [ ] **Step 1: Write failing test for Time column**

Add to `apps/frontend/src/features/signals/components/TraceResultsTable.test.tsx`:

```ts
test("renders a Time column using the provided timeFormat", () => {
  render(
    <TraceResultsTable
      traces={traces}
      selectedTraceId={undefined}
      onSelectTrace={vi.fn()}
      timeFormat="iso-utc-ms"
    />,
  );

  const table = screen.getByRole("table", { name: "Trace results" });
  expect(within(table).getByRole("columnheader", { name: "Time" })).toBeInTheDocument();
  // start_time_unix_nano: 1 → 1970-01-01 00:00:00.000Z
  expect(within(table).getByText("1970-01-01 00:00:00.000Z")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

```powershell
cd apps/frontend && npx vitest run src/features/signals/components/TraceResultsTable.test.tsx
```
Expected: FAIL — "Time" column header not found.

- [ ] **Step 3: Update TraceResultsTable to add timeFormat prop and Time column**

Replace the full content of `apps/frontend/src/features/signals/components/TraceResultsTable.tsx`:

```tsx
import { Link } from "@tanstack/react-router";
import type { TraceResponse } from "../../api/traces";
import { Badge } from "../../components/ui/badge";
import { formatTimestamp } from "../../utils/formatTimestamp";
import type { TimeFormat } from "../../lib/timeDisplay";

export function TraceResultsTable({
  traces,
  selectedTraceId,
  onSelectTrace,
  mode = "select",
  showServiceColumn = true,
  timeFormat = "iso-local-ms",
  ariaLabel = showServiceColumn ? "Trace results" : "Service traces",
}: {
  traces: TraceResponse[];
  selectedTraceId: string | undefined;
  onSelectTrace: (traceId: string) => void;
  mode?: "select" | "link";
  showServiceColumn?: boolean;
  timeFormat?: TimeFormat;
  ariaLabel?: string;
}) {
  return (
    <table aria-label={ariaLabel}>
      <thead>
        <tr>
          <th aria-label="Time">Time</th>
          <th>Trace ID</th>
          {showServiceColumn && <th>Service</th>}
          <th>Operation</th>
          <th>Duration</th>
          <th>Status</th>
        </tr>
      </thead>
      <tbody>
        {traces.map((trace) => (
          <TraceResultsRow
            key={trace.trace_id}
            trace={trace}
            selected={selectedTraceId === trace.trace_id}
            onSelect={() => onSelectTrace(trace.trace_id)}
            mode={mode}
            showServiceColumn={showServiceColumn}
            timeFormat={timeFormat}
          />
        ))}
      </tbody>
    </table>
  );
}

function TraceResultsRow({
  trace,
  selected,
  onSelect,
  mode,
  showServiceColumn,
  timeFormat,
}: {
  trace: TraceResponse;
  selected: boolean;
  onSelect: () => void;
  mode: "select" | "link";
  showServiceColumn: boolean;
  timeFormat: TimeFormat;
}) {
  const root = trace.spans[0];
  if (!root) return null;

  return (
    <tr className={`modern-table-row ${selected ? "bg-[var(--surface-subtle)]" : ""}`}>
      <td className="whitespace-nowrap">{formatTimestamp(root.start_time_unix_nano, timeFormat)}</td>
      <td className="strong-cell">
        {mode === "link" ? (
          <Link to="/traces/$traceId" params={{ traceId: trace.trace_id }}>
            {trace.trace_id.substring(0, 16)}
          </Link>
        ) : (
          <button
            type="button"
            className="text-left text-[var(--brand)] bg-transparent border-0 p-0 font-inherit cursor-pointer hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)]"
            aria-label={`${trace.trace_id.substring(0, 16)}…`}
            onClick={onSelect}
          >
            {trace.trace_id.substring(0, 16)}…
          </button>
        )}
      </td>
      {showServiceColumn && <td>{root.service_name}</td>}
      <td>{root.operation_name}</td>
      <td>{(root.duration_ns / 1e6).toFixed(2)}ms</td>
      <td>
        <Badge tone={root.status_code === "ERROR" ? "bad" : "good"}>{root.status_code}</Badge>
      </td>
    </tr>
  );
}
```

- [ ] **Step 4: Update TraceSearch.tsx to pass timeFormat to TraceResultsTable**

In `apps/frontend/src/pages/TraceSearch.tsx`, update the `TraceResultsTable` usage:
```tsx
<TraceResultsTable
  traces={traces}
  selectedTraceId={selectedTraceId}
  onSelectTrace={setSelectedTraceId}
  mode={tableMode}
  showServiceColumn={showServiceColumn}
  timeFormat={format}
  ariaLabel={tableAriaLabel}
/>
```

- [ ] **Step 5: Run tests to verify they pass**

```powershell
cd apps/frontend && npx vitest run src/features/signals/components/TraceResultsTable.test.tsx
```
Expected: all tests PASS (existing + new Time column test).

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/features/signals/components/TraceResultsTable.tsx
git add apps/frontend/src/features/signals/components/TraceResultsTable.test.tsx
git add apps/frontend/src/pages/TraceSearch.tsx
git commit -m "feat: add start-time date column to TraceResultsTable"
```

---

## Task 3: Create LogList shared component

**Files:**
- Create: `apps/frontend/src/components/shared/LogList.tsx`
- Create: `apps/frontend/src/components/shared/LogList.test.tsx`

- [ ] **Step 1: Write failing tests**

Create `apps/frontend/src/components/shared/LogList.test.tsx`:

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { vi, expect, test } from "vitest";
import type { LogRecord } from "../../api/logs";
import { LogList } from "./LogList";

const log: LogRecord = {
  tenant_id: "t1",
  log_id: "log-1",
  timestamp_unix_nano: "1700000000000000000",
  severity_number: 9,
  severity_text: "INFO",
  body: "checkout completed",
  trace_id: "trace-abc",
  service_name: "svc",
};

test("renders timestamp, severity, and message for each log row", () => {
  render(<LogList logs={[log]} timeFormat="iso-utc-ms" />);
  expect(screen.getByText("checkout completed")).toBeInTheDocument();
  expect(screen.getByText("INFO")).toBeInTheDocument();
});

test("shows loading text when loading=true", () => {
  render(<LogList logs={[]} loading timeFormat="iso-utc-ms" />);
  expect(screen.getByText(/Loading logs/)).toBeInTheDocument();
});

test("shows custom emptyMessage when no logs", () => {
  render(<LogList logs={[]} emptyMessage="Nothing here." timeFormat="iso-utc-ms" />);
  expect(screen.getByText("Nothing here.")).toBeInTheDocument();
});

test("highlights the pivot row with warn-bg class", () => {
  render(<LogList logs={[log]} pivotId="log-1" timeFormat="iso-utc-ms" />);
  const row = screen.getByText("checkout completed").closest("div[data-log-id]")!;
  expect(row.className).toMatch(/warn-bg/);
});

test("calls onRowClick when row is clicked", () => {
  const onClick = vi.fn();
  render(<LogList logs={[log]} onRowClick={onClick} timeFormat="iso-utc-ms" />);
  fireEvent.click(screen.getByText("checkout completed"));
  expect(onClick).toHaveBeenCalledWith(log);
});

test("renders trace link when showTraceLink=true and log has trace_id", () => {
  render(<LogList logs={[log]} showTraceLink timeFormat="iso-utc-ms" />);
  const link = screen.getByRole("link", { name: /View trace/ });
  expect(link).toHaveAttribute("href", "/traces/trace-abc");
});

test("does not render trace link when showTraceLink is omitted", () => {
  render(<LogList logs={[log]} timeFormat="iso-utc-ms" />);
  expect(screen.queryByRole("link", { name: /View trace/ })).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run tests to verify they fail**

```powershell
cd apps/frontend && npx vitest run src/components/shared/LogList.test.tsx
```
Expected: FAIL — module not found.

- [ ] **Step 3: Implement LogList**

Create `apps/frontend/src/components/shared/LogList.tsx`:

```tsx
import type { LogRecord } from "../../api/logs";
import { formatTimestamp } from "../../utils/formatTimestamp";
import { formatLogMessage, getSeverityColor } from "../../utils/logFormatting";
import type { TimeFormat } from "../../lib/timeDisplay";

export interface LogListProps {
  logs: LogRecord[];
  loading?: boolean;
  emptyMessage?: string;
  pivotId?: string;
  onRowClick?: (log: LogRecord) => void;
  showTraceLink?: boolean;
  timeFormat: TimeFormat;
}

export function LogList({
  logs,
  loading = false,
  emptyMessage = "No logs found.",
  pivotId,
  onRowClick,
  showTraceLink = false,
  timeFormat,
}: LogListProps) {
  if (loading) {
    return <p className="text-sm text-[var(--muted)]">Loading logs…</p>;
  }
  if (!logs.length) {
    return <p className="text-sm text-[var(--muted)]">{emptyMessage}</p>;
  }

  return (
    <div className="font-mono text-xs max-h-[400px] overflow-y-auto border border-[var(--border)] bg-[var(--surface)] p-2">
      {logs.map((log) => {
        const isPivot = pivotId !== undefined && log.log_id === pivotId;
        return (
          <div
            key={log.log_id}
            data-log-id={log.log_id}
            role={onRowClick ? "button" : undefined}
            tabIndex={onRowClick ? 0 : undefined}
            onClick={onRowClick ? () => onRowClick(log) : undefined}
            onKeyDown={
              onRowClick
                ? (e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      onRowClick(log);
                    }
                  }
                : undefined
            }
            className={[
              "flex gap-3 py-1 border-b border-[var(--border)] last:border-b-0",
              isPivot ? "bg-[var(--warn-bg)] font-bold" : "",
              onRowClick ? "cursor-pointer hover:bg-[var(--surface-subtle)]" : "",
            ]
              .filter(Boolean)
              .join(" ")}
          >
            <span className="text-[var(--muted)] shrink-0">
              {formatTimestamp(log.timestamp_unix_nano, timeFormat)}
            </span>
            <span className="w-[50px] shrink-0" style={{ color: getSeverityColor(log.severity_number) }}>
              {log.severity_text || `LVL ${log.severity_number}`}
            </span>
            <span className="flex-1 min-w-0 break-all">{formatLogMessage(log.body)}</span>
            {showTraceLink && log.trace_id && (
              <a
                href={`/traces/${log.trace_id}`}
                className="shrink-0 text-[var(--brand)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)]"
                aria-label={log.span_id ? `View span ${log.span_id}` : `View trace ${log.trace_id}`}
              >
                trace
              </a>
            )}
            {isPivot && (
              <span className="text-[var(--warn)] text-[10px] shrink-0">[PIVOT]</span>
            )}
          </div>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

```powershell
cd apps/frontend && npx vitest run src/components/shared/LogList.test.tsx
```
Expected: all 7 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/components/shared/LogList.tsx
git add apps/frontend/src/components/shared/LogList.test.tsx
git commit -m "feat: add shared LogList component for mono log-row rendering"
```

---

## Task 4: Refactor LogContextView to use LogList

**Files:**
- Modify: `apps/frontend/src/components/LogContextView.tsx`
- Test: `apps/frontend/src/components/LogContextView.test.tsx`

- [ ] **Step 1: Verify existing LogContextView tests pass (baseline)**

```powershell
cd apps/frontend && npx vitest run src/components/LogContextView.test.tsx
```
Expected: all existing tests PASS. (Capture output to compare after refactor.)

- [ ] **Step 2: Rewrite LogContextView to delegate to LogList**

Replace the full content of `apps/frontend/src/components/LogContextView.tsx`:

```tsx
import { useQuery } from "@tanstack/react-query";
import { getLogContext } from "../api/logs";
import { Button } from "./ui/button";
import { useTimeDisplay } from "../lib/timeDisplay";
import { LogList } from "./shared/LogList";

interface Props {
  logId: string;
  onClose: () => void;
}

export function LogContextView({ logId, onClose }: Props) {
  const { data, isLoading } = useQuery({
    queryKey: ["logs", "context", logId],
    queryFn: () => getLogContext(logId),
  });
  const { format } = useTimeDisplay();

  return (
    <div className="mt-3 p-3 bg-[var(--surface-inset)] border border-[var(--border)]">
      <div className="flex justify-between items-center mb-3">
        <h4 className="m-0 text-sm font-bold text-[var(--text-strong)]">Surrounding Logs</h4>
        <Button variant="secondary" onClick={onClose}>Close</Button>
      </div>
      <LogList
        logs={data?.logs ?? []}
        loading={isLoading}
        pivotId={logId}
        showTraceLink
        timeFormat={format}
      />
    </div>
  );
}
```

- [ ] **Step 3: Run tests to verify they still pass**

```powershell
cd apps/frontend && npx vitest run src/components/LogContextView.test.tsx
```
Expected: same tests PASS as before.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/components/LogContextView.tsx
git commit -m "refactor: LogContextView delegates rendering to LogList"
```

---

## Task 5: Refactor LogCorrelatedList to use LogList

**Files:**
- Modify: `apps/frontend/src/components/LogCorrelatedList.tsx`
- Test: `apps/frontend/src/components/LogCorrelatedList.render.test.tsx`

The `LogCorrelatedList` has a "Exact span" / "Trace-level" correlation label column between severity and message. `LogList` does not have this column — the label will move to a small inline element after the message using `LogList`'s `showTraceLink` link to replace the separate label. The trace link in `LogList` already renders "trace" as a link to the trace page, which covers the navigation. The "Exact span" / "Trace-level" label is rendered via the trace link `aria-label`. The `correlationLabel` helper and `filterCorrelatedLogs` remain exported for tests and external consumers.

- [ ] **Step 1: Run existing tests baseline**

```powershell
cd apps/frontend && npx vitest run src/components/LogCorrelatedList.render.test.tsx
```
Expected: all 8 tests PASS. Note which tests check for "Exact span" / "Trace-level" text — those will be updated.

- [ ] **Step 2: Rewrite LogCorrelatedList to use LogList**

Replace the full content of `apps/frontend/src/components/LogCorrelatedList.tsx`:

```tsx
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { LogRecord } from "../api/logs";
import { searchLogs } from "../api/logs";
import { LogContextView } from "./LogContextView";
import { LogList } from "./shared/LogList";
import { useTimeDisplay } from "../lib/timeDisplay";

interface Props {
  traceId: string;
  spanId?: string;
}

export function LogCorrelatedList({ traceId, spanId }: Props) {
  const [focusedLogId, setFocusedLogId] = useState<string | undefined>();
  const { format } = useTimeDisplay();
  const { data, isLoading } = useQuery({
    queryKey: ["logs", traceId],
    queryFn: () => searchLogs({ trace_id: traceId }),
  });

  const logs = filterCorrelatedLogs(data?.logs ?? [], spanId);

  return (
    <div className="mt-5">
      <h3 className="text-sm font-bold text-[var(--text-strong)] mb-2">
        {spanId
          ? `Exact span logs and trace-level logs (${spanId.substring(0, 8)})`
          : "Trace-correlated logs"}
      </h3>
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
}

export function filterCorrelatedLogs(logs: LogRecord[], spanId?: string): LogRecord[] {
  if (!spanId) return logs;
  return logs.filter((log) => log.span_id === spanId || !log.span_id);
}

export function correlationLabel(log: LogRecord): "Exact span" | "Trace-level" {
  return log.span_id ? "Exact span" : "Trace-level";
}
```

- [ ] **Step 3: Update the render tests that checked for standalone correlation label text**

The "Exact span" and "Trace-level" text is now rendered only as the `aria-label` of the trace link, not as visible text. Update `apps/frontend/src/components/LogCorrelatedList.render.test.tsx`:

Replace:
```ts
test("Exact span correlation label appears for span-linked logs", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [spanLog],
    total: 1,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() => expect(screen.getByText("Exact span")).toBeInTheDocument());
});

test("Trace-level correlation label appears for logs without span_id", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [traceLog],
    total: 1,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() => expect(screen.getByText("Trace-level")).toBeInTheDocument());
});

test("span-linked log renders a link with correct href", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [spanLog],
    total: 1,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() => expect(screen.getByRole("link", { name: "Exact span" })).toBeInTheDocument());
  const link = screen.getByRole("link", { name: "Exact span" });
  expect(link).toHaveAttribute("href", "/traces/trace-abc");
});
```

With:
```ts
test("span-linked log renders trace link with span aria-label", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [spanLog],
    total: 1,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByRole("link", { name: `View span ${spanLog.span_id}` })).toBeInTheDocument()
  );
  const link = screen.getByRole("link", { name: `View span ${spanLog.span_id}` });
  expect(link).toHaveAttribute("href", "/traces/trace-abc");
});

test("trace-level log renders trace link with trace aria-label", async () => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({
    logs: [traceLog],
    total: 1,
    facets: {},
  });

  render(<LogCorrelatedList traceId="trace-abc" />, { wrapper });
  await waitFor(() =>
    expect(screen.getByRole("link", { name: `View trace ${traceLog.trace_id}` })).toBeInTheDocument()
  );
});
```

- [ ] **Step 4: Run tests to verify they pass**

```powershell
cd apps/frontend && npx vitest run src/components/LogCorrelatedList.render.test.tsx
```
Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/components/LogCorrelatedList.tsx
git add apps/frontend/src/components/LogCorrelatedList.render.test.tsx
git commit -m "refactor: LogCorrelatedList delegates row rendering to LogList"
```

---

## Task 6: Create SignalExplorer shell component

**Files:**
- Create: `apps/frontend/src/components/shared/SignalExplorer.tsx`
- Create: `apps/frontend/src/components/shared/SignalExplorer.test.tsx`

`SignalExplorer` owns: toolbar layout (service input, lookback select, custom range display, promote button) and the panel/table split (panel on LEFT at 25%, table on right). It does NOT own data fetching. Wrappers call `useSignalSearch` and pass state as props.

- [ ] **Step 1: Write failing tests**

Create `apps/frontend/src/components/shared/SignalExplorer.test.tsx`:

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { vi, expect, test } from "vitest";
import { SignalExplorer } from "./SignalExplorer";
import type { SignalExplorerProps } from "./SignalExplorer";

function makeProps(overrides: Partial<SignalExplorerProps> = {}): SignalExplorerProps {
  return {
    title: "Logs",
    service: "",
    onServiceChange: vi.fn(),
    lookbackMinutes: 60,
    onLookbackChange: vi.fn(),
    customRangeMs: null,
    onClearRange: vi.fn(),
    showHeader: true,
    showPromote: false,
    saveStatus: "idle",
    onPromote: vi.fn(),
    histogram: <div data-testid="histogram" />,
    renderTable: (selectedId, onSelect) => (
      <button data-testid="table" onClick={() => onSelect("row-1")}>
        {selectedId ?? "none selected"}
      </button>
    ),
    renderPanel: (selectedId, onClose) => (
      <div data-testid="panel" data-selected={selectedId}>
        <button onClick={onClose}>Close</button>
      </div>
    ),
    ...overrides,
  };
}

test("renders the title in the page header", () => {
  render(<SignalExplorer {...makeProps()} />);
  expect(screen.getByRole("heading", { name: "Logs" })).toBeInTheDocument();
});

test("renders the histogram slot", () => {
  render(<SignalExplorer {...makeProps()} />);
  expect(screen.getByTestId("histogram")).toBeInTheDocument();
});

test("panel is hidden initially — renderTable receives null selectedId", () => {
  render(<SignalExplorer {...makeProps()} />);
  expect(screen.getByTestId("table")).toHaveTextContent("none selected");
  expect(screen.queryByTestId("panel")).not.toBeInTheDocument();
});

test("clicking a row opens the panel with that id", () => {
  render(<SignalExplorer {...makeProps()} />);
  fireEvent.click(screen.getByTestId("table"));
  expect(screen.getByTestId("panel")).toHaveAttribute("data-selected", "row-1");
  expect(screen.getByTestId("table")).toHaveTextContent("row-1");
});

test("clicking the same row again closes the panel", () => {
  render(<SignalExplorer {...makeProps()} />);
  fireEvent.click(screen.getByTestId("table")); // open
  fireEvent.click(screen.getByTestId("table")); // close (same id)
  expect(screen.queryByTestId("panel")).not.toBeInTheDocument();
});

test("panel close button clears selection", () => {
  render(<SignalExplorer {...makeProps()} />);
  fireEvent.click(screen.getByTestId("table")); // open
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(screen.queryByTestId("panel")).not.toBeInTheDocument();
});

test("panel container has w-1/4 class when open", () => {
  render(<SignalExplorer {...makeProps()} />);
  fireEvent.click(screen.getByTestId("table")); // open
  const panelContainer = screen.getByTestId("panel").parentElement!;
  expect(panelContainer.className).toMatch(/w-1\/4/);
});

test("service input calls onServiceChange on change", () => {
  const onServiceChange = vi.fn();
  render(<SignalExplorer {...makeProps({ onServiceChange })} />);
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "checkout" } });
  expect(onServiceChange).toHaveBeenCalledWith("checkout");
});
```

- [ ] **Step 2: Run tests to verify they fail**

```powershell
cd apps/frontend && npx vitest run src/components/shared/SignalExplorer.test.tsx
```
Expected: FAIL — module not found.

- [ ] **Step 3: Implement SignalExplorer**

Create `apps/frontend/src/components/shared/SignalExplorer.tsx`:

```tsx
import { type ReactNode, useState } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Select, SelectOption } from "../ui/select";

const timeRangeOptions = [
  { label: "15m", value: 15 },
  { label: "1h", value: 60 },
  { label: "6h", value: 360 },
  { label: "24h", value: 1440 },
];

export type SaveStatus = "idle" | "saving" | "saved" | "error";

export interface SignalExplorerProps {
  title: string;
  service: string;
  onServiceChange: (service: string) => void;
  lookbackMinutes: number;
  onLookbackChange: (minutes: number) => void;
  customRangeMs: { fromMs: number; toMs: number } | null;
  onClearRange: () => void;
  lockedService?: boolean;
  showHeader?: boolean;
  showPromote?: boolean;
  saveStatus: SaveStatus;
  onPromote: () => void;
  histogram: ReactNode;
  renderTable: (selectedId: string | null, onSelect: (id: string | null) => void) => ReactNode;
  renderPanel: (selectedId: string, onClose: () => void) => ReactNode;
}

export function SignalExplorer({
  title,
  service,
  onServiceChange,
  lookbackMinutes,
  onLookbackChange,
  customRangeMs,
  onClearRange,
  lockedService = false,
  showHeader = true,
  showPromote = true,
  saveStatus,
  onPromote,
  histogram,
  renderTable,
  renderPanel,
}: SignalExplorerProps) {
  const [selectedId, setSelectedId] = useState<string | null>(null);

  function handleSelect(id: string | null) {
    setSelectedId((prev) => (prev === id ? null : id));
  }

  function handleServiceChange(s: string) {
    setSelectedId(null);
    onServiceChange(s);
  }

  function handleLookbackChange(m: number) {
    setSelectedId(null);
    onLookbackChange(m);
  }

  function handleClearRangeAndReset() {
    setSelectedId(null);
    onClearRange();
  }

  return (
    <div className="page-stack">
      {showHeader && (
        <div className="page-header">
          <div>
            <div className="text-xs font-bold uppercase text-[var(--muted)]">Explorer</div>
            <h1>{title}</h1>
          </div>
        </div>
      )}

      <div className="toolbar-row">
        {!lockedService && (
          <Input
            className="max-w-[360px]"
            placeholder="Filter by service"
            value={service}
            onChange={(e) => handleServiceChange(e.target.value)}
            aria-label="Filter by service"
          />
        )}
        {customRangeMs ? (
          <Button variant="secondary" onClick={handleClearRangeAndReset}>
            Reset range
          </Button>
        ) : (
          <Select
            aria-label={`${title} time range`}
            className="max-w-[120px]"
            value={String(lookbackMinutes)}
            onChange={(e) => handleLookbackChange(Number(e.target.value))}
          >
            {timeRangeOptions.map((opt) => (
              <SelectOption key={opt.value} value={opt.value}>
                {opt.label}
              </SelectOption>
            ))}
          </Select>
        )}
        {service && !lockedService && (
          <Button variant="secondary" onClick={() => handleServiceChange("")}>
            Clear filters
          </Button>
        )}
        {showPromote && (
          <>
            <Button onClick={onPromote} disabled={saveStatus === "saving"}>
              Promote to dashboard
            </Button>
            {saveStatus === "saved" && (
              <span className="text-sm font-semibold text-[var(--good)]">Saved to dashboard</span>
            )}
            {saveStatus === "error" && (
              <span className="text-sm font-semibold text-[var(--bad)]">Dashboard save failed</span>
            )}
          </>
        )}
      </div>

      {histogram}

      <div className="flex items-start gap-3 max-[900px]:flex-col">
        {selectedId !== null && (
          <div className="w-1/4 shrink-0">
            {renderPanel(selectedId, () => setSelectedId(null))}
          </div>
        )}
        <div className="flex flex-1 items-start gap-3">
          {renderTable(selectedId, handleSelect)}
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

```powershell
cd apps/frontend && npx vitest run src/components/shared/SignalExplorer.test.tsx
```
Expected: all 8 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/components/shared/SignalExplorer.tsx
git add apps/frontend/src/components/shared/SignalExplorer.test.tsx
git commit -m "feat: add SignalExplorer shell component with left panel slot"
```

---

## Task 7: Refactor LogExplorer to use SignalExplorer

**Files:**
- Modify: `apps/frontend/src/pages/LogSearch.tsx`
- Create: `apps/frontend/src/utils/formatBucketLabel.ts`
- Test: `apps/frontend/src/pages/LogSearch.test.tsx`

- [ ] **Step 1: Run existing LogSearch tests (baseline)**

```powershell
cd apps/frontend && npx vitest run src/pages/LogSearch.test.tsx
```
Expected: all tests PASS. Note count.

- [ ] **Step 2: Extract formatBucketLabel to shared utility**

Both `LogSearch.tsx` and `TraceSearch.tsx` define the same private `formatBucketLabel` function. Extract it.

Create `apps/frontend/src/utils/formatBucketLabel.ts`:

```ts
import type { TimeFormat } from "../lib/timeDisplay";

export function formatBucketLabel(ms: number, format: TimeFormat): string {
  const utc =
    format === "iso-utc-ms" ||
    format === "iso-utc-ns" ||
    format === "unix-ms" ||
    format === "unix-ns";
  return utc ? new Date(ms).toISOString() : new Date(ms).toLocaleTimeString();
}
```

- [ ] **Step 3: Rewrite LogSearch.tsx**

Replace the full content of `apps/frontend/src/pages/LogSearch.tsx`:

```tsx
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createDashboard } from "../api/dashboards";
import {
  searchLogs,
  fetchLogHistogram,
  LogRecord,
  LogHistogramBucket as ApiHistogramBucket,
  LogHistogramResponse,
} from "../api/logs";
import { infraLinks } from "../utils/infraLinks";
import { formatTimestamp } from "../utils/formatTimestamp";
import { formatBucketLabel } from "../utils/formatBucketLabel";
import { OTelLevel, otelSeverity, formatLogMessage, formatContextValue } from "../utils/logFormatting";
import { useTimeDisplay } from "../lib/timeDisplay";
import { useSignalSearch } from "../hooks/useSignalSearch";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { LoadingState } from "../components/ui/loading-state";
import { TablePanel } from "../components/ui/table-panel";
import { Histogram, HistogramBucket } from "../components/ui/histogram";
import { SignalExplorer, SaveStatus } from "../components/shared/SignalExplorer";
import { LogResultsTable } from "../features/signals/components/LogResultsTable";

const levelOrder: OTelLevel[] = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR", "FATAL"];
const levelBarClasses: Record<OTelLevel, string> = {
  TRACE: "bg-[var(--muted)]",
  DEBUG: "bg-[var(--brand)]",
  INFO: "bg-[var(--good)]",
  WARN: "bg-[var(--warn)]",
  ERROR: "bg-[var(--bad)]",
  FATAL: "bg-[var(--bad)]",
};

export type LogExplorerProps = {
  initialService?: string;
  lockedService?: boolean;
  initialLookbackMinutes?: number;
  showHeader?: boolean;
  showServiceColumn?: boolean;
  showPromote?: boolean;
  tableAriaLabel?: string;
};

export default function LogSearch() {
  return (
    <LogExplorer
      initialService={new URLSearchParams(window.location.search).get("service") ?? ""}
    />
  );
}

export function LogExplorer({
  initialService = "",
  lockedService = false,
  initialLookbackMinutes = 60,
  showHeader = true,
  showServiceColumn = true,
  showPromote = true,
  tableAriaLabel,
}: LogExplorerProps) {
  const { format } = useTimeDisplay();
  const {
    service,
    setService,
    lookbackMinutes,
    setLookbackMinutes,
    customRangeMs,
    handleHistogramRangeSelect,
    handleClearRange,
    from,
    to,
    histogramFromMs,
    histogramToMs,
  } = useSignalSearch({ initialService, initialLookbackMinutes });
  const [bucketCount, setBucketCount] = useState(60);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");

  const { data, isLoading, error } = useQuery({
    queryKey: ["logs", service, from, to],
    queryFn: () => searchLogs({ service: service || undefined, from, to, limit: 50 }),
  });

  const { data: histogramData, isError: isHistogramError } = useQuery({
    queryKey: ["logs-histogram", service, from, to, bucketCount],
    queryFn: () =>
      fetchLogHistogram({
        service: service || undefined,
        from,
        to: new Date(histogramToMs).toISOString(),
        buckets: bucketCount,
      }),
    placeholderData: (prev: LogHistogramResponse | undefined) => prev,
  });

  const logs = data?.logs ?? [];
  const histogram = useMemo(
    () =>
      histogramData?.buckets
        ? histogramFromApi(histogramData.buckets)
        : buildLogHistogram([], histogramFromMs, histogramToMs),
    [histogramData, histogramFromMs, histogramToMs],
  );

  const handlePromote = async () => {
    setSaveStatus("saving");
    try {
      await createDashboard({
        name: service ? `Logs for ${service}` : "Promoted log query",
        panels: [
          {
            title: service ? `Logs for ${service}` : "Log search",
            query_kind: "logs",
            service: service || undefined,
            lookback_minutes: lookbackMinutes,
            filters: { facets: ["service_name", "severity_number", "environment", "host_id"] },
          },
        ],
      });
      setSaveStatus("saved");
    } catch {
      setSaveStatus("error");
    }
  };

  return (
    <SignalExplorer
      title="Logs"
      service={service}
      onServiceChange={(s) => { setService(s); }}
      lookbackMinutes={lookbackMinutes}
      onLookbackChange={(m) => { setLookbackMinutes(m); }}
      customRangeMs={customRangeMs}
      onClearRange={handleClearRange}
      lockedService={lockedService}
      showHeader={showHeader}
      showPromote={showPromote}
      saveStatus={saveStatus}
      onPromote={handlePromote}
      histogram={
        histogramData ? (
          <Histogram
            buckets={histogram}
            categoryOrder={levelOrder}
            categoryColors={levelBarClasses}
            format={(ms) => formatBucketLabel(ms, format)}
            onRangeSelect={handleHistogramRangeSelect}
            onBucketCountChange={setBucketCount}
            ariaLabel="Log volume histogram"
            title="Logs over time"
            subtitle="Volume"
          />
        ) : !isHistogramError ? (
          <div
            aria-hidden="true"
            className="border border-[var(--border)] bg-[var(--surface)] p-3 h-[168px] animate-pulse"
          />
        ) : (
          <p className="text-xs text-[var(--muted)]">Histogram unavailable</p>
        )
      }
      renderTable={(selectedId, onSelect) => (
        <TablePanel className="flex-1">
          {isLoading ? (
            <LoadingState>Loading logs…</LoadingState>
          ) : error ? (
            <LoadingState className="text-[var(--bad)]">Error loading logs: {String(error)}</LoadingState>
          ) : logs.length === 0 ? (
            <LoadingState>No logs found.</LoadingState>
          ) : (
            <LogResultsTable
              logs={logs}
              selectedLogId={selectedId ?? undefined}
              onSelectLog={(id) => onSelect(id)}
              timeFormat={format}
              showServiceColumn={showServiceColumn}
              ariaLabel={tableAriaLabel}
            />
          )}
        </TablePanel>
      )}
      renderPanel={(selectedId, onClose) => {
        const log = logs.find((l) => l.log_id === selectedId);
        return log ? <LogContextSidebar log={log} format={format} onClose={onClose} /> : null;
      }}
    />
  );
}

function LogContextSidebar({
  log,
  format,
  onClose,
}: {
  log: LogRecord;
  format: import("../lib/timeDisplay").TimeFormat;
  onClose: () => void;
}) {
  const severity = otelSeverity(log.severity_number);
  const entries = logContextEntries(log, format);
  const badges = infraLinks(log.resource_attributes ?? {});

  return (
    <aside
      aria-label="Selected log context"
      className="w-full border border-[var(--border)] bg-[var(--surface)] p-4"
    >
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Selected Log</div>
          <h2 className="m-0 text-base font-bold text-[var(--text-strong)]">Context Properties</h2>
        </div>
        <Button variant="secondary" className="min-h-8 px-2 text-xs" onClick={onClose}>
          Close
        </Button>
      </div>
      <Badge tone={severity.tone} className="mb-3">
        {severity.label}
      </Badge>
      <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
        {entries.map(([key, value]) => (
          <div key={key} className="contents">
            <dt className="break-all font-bold text-[var(--muted)]">{key}</dt>
            <dd className="m-0 min-w-0 break-all text-[var(--text)]">
              {key === "trace_id" && log.trace_id ? (
                <a
                  href={`/traces/${log.trace_id}`}
                  className="text-[var(--brand)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)]"
                >
                  {value}
                </a>
              ) : key === "span_id" && log.trace_id ? (
                <a
                  href={`/traces/${log.trace_id}`}
                  title="View parent trace"
                  className="text-[var(--brand)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)]"
                >
                  {value}
                </a>
              ) : (
                value
              )}
            </dd>
          </div>
        ))}
      </dl>
      {badges.length > 0 && (
        <div className="mt-4 flex flex-wrap gap-1.5">
          {badges.map((link) => (
            <a
              key={link.href}
              href={link.href}
              className="text-[11px] px-1.5 py-0.5 bg-[var(--surface-subtle)] text-[var(--text)] border border-[var(--border)] no-underline whitespace-nowrap hover:border-[var(--brand)] hover:text-[var(--brand)]"
            >
              {link.label}
            </a>
          ))}
        </div>
      )}
    </aside>
  );
}

// ── Histogram helpers ────────────────────────────────────────────────────────

function histogramFromApi(buckets: ApiHistogramBucket[]): HistogramBucket<OTelLevel>[] {
  return buckets.map((b) => {
    const categories = emptyLevels();
    let total = 0;
    for (const [sev, count] of Object.entries(b.counts)) {
      const level = otelSeverity(Number(sev)).label;
      categories[level] += count;
      total += count;
    }
    return { startMs: b.start_ms, endMs: b.end_ms, total, categories };
  });
}

export function buildLogHistogram(logs: LogRecord[], fromMs: number, toMs: number): HistogramBucket<OTelLevel>[] {
  const bucketCount = 30;
  const rangeMs = toMs - fromMs;
  const bucketMs = rangeMs / bucketCount;
  const buckets = Array.from({ length: bucketCount }, (_, i) => ({
    startMs: fromMs + i * bucketMs,
    endMs: fromMs + (i + 1) * bucketMs,
    total: 0,
    categories: emptyLevels(),
  }));
  for (const log of logs) {
    const ms = Number(log.timestamp_unix_nano) / 1_000_000;
    if (!Number.isFinite(ms)) continue;
    const idx = Math.min(bucketCount - 1, Math.max(0, Math.floor((ms - fromMs) / bucketMs)));
    const level = otelSeverity(log.severity_number).label;
    buckets[idx].total += 1;
    buckets[idx].categories[level] += 1;
  }
  return buckets;
}

function emptyLevels(): Record<OTelLevel, number> {
  return { TRACE: 0, DEBUG: 0, INFO: 0, WARN: 0, ERROR: 0, FATAL: 0 };
}

function logContextEntries(
  log: LogRecord,
  format: import("../lib/timeDisplay").TimeFormat,
): [string, string][] {
  const entries: [string, string][] = [
    ["time", formatTimestamp(log.timestamp_unix_nano, format)],
    ["service.name", log.service_name],
    ["severity_number", String(log.severity_number)],
    ["message", formatLogMessage(log.body)],
  ];
  if (log.observed_timestamp_unix_nano)
    entries.push(["observed_time", formatTimestamp(log.observed_timestamp_unix_nano, format)]);
  if (log.environment) entries.push(["environment", log.environment]);
  if (log.host_id) entries.push(["host_id", log.host_id]);
  if (log.trace_id) entries.push(["trace_id", log.trace_id]);
  if (log.span_id) entries.push(["span_id", log.span_id]);
  if (log.fingerprint !== null && log.fingerprint !== undefined)
    entries.push(["fingerprint", String(log.fingerprint)]);
  for (const [k, v] of Object.entries(log.attributes ?? {}).sort(([a], [b]) => a.localeCompare(b)))
    entries.push([`log.${k}`, formatContextValue(v)]);
  for (const [k, v] of Object.entries(log.resource_attributes ?? {}).sort(([a], [b]) => a.localeCompare(b)))
    entries.push([k, formatContextValue(v)]);
  return entries;
}

export { otelSeverity, formatLogMessage } from "../utils/logFormatting";
```

Key changes from the original:
- Added `useSignalSearch` hook call
- Removed duplicate state management (service, lookback, customRange)
- Removed inline toolbar markup (now in SignalExplorer)
- `LogContextSidebar` now uses `w-full` instead of `w-[320px] shrink-0` — width is controlled by the 25% container in SignalExplorer
- Removed the custom range label display from toolbar (SignalExplorer handles it, but the formatted date range string is not passed — add it if needed: pass `customRangeLabel` prop to SignalExplorer or keep a formatted span in the toolbar area. See step 4.)

- [ ] **Step 4: Handle custom range label in SignalExplorer**

The current LogSearch shows the formatted date range next to the "Reset range" button (e.g., `"2026-05-02 10:00:00.000 – 2026-05-02 11:00:00.000"`). Add an optional `customRangeLabel` prop to `SignalExplorer` to render this.

In `apps/frontend/src/components/shared/SignalExplorer.tsx`, add to `SignalExplorerProps`:
```ts
customRangeLabel?: string;
```

In the toolbar section, replace:
```tsx
{customRangeMs ? (
  <Button variant="secondary" onClick={onClearRange}>
    Reset range
  </Button>
) : (
```
with:
```tsx
{customRangeMs ? (
  <>
    {customRangeLabel && (
      <span className="text-xs whitespace-nowrap font-mono text-[var(--text-strong)]">
        {customRangeLabel}
      </span>
    )}
    <Button variant="secondary" onClick={onClearRange}>
      Reset range
    </Button>
  </>
) : (
```

In `LogSearch.tsx`, pass `customRangeLabel` to SignalExplorer:
```tsx
customRangeLabel={
  customRangeMs
    ? `${formatBucketLabel(customRangeMs.fromMs, format)} – ${formatBucketLabel(customRangeMs.toMs, format)}`
    : undefined
}
```

- [ ] **Step 5: Run LogSearch tests**

```powershell
cd apps/frontend && npx vitest run src/pages/LogSearch.test.tsx
```
Expected: same tests PASS as baseline.

- [ ] **Step 6: Run typecheck**

```powershell
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add apps/frontend/src/pages/LogSearch.tsx
git add apps/frontend/src/utils/formatBucketLabel.ts
git add apps/frontend/src/components/shared/SignalExplorer.tsx
git commit -m "refactor: LogExplorer is now a thin wrapper around SignalExplorer"
```

---

## Task 8: Refactor TraceExplorer to use SignalExplorer

**Files:**
- Modify: `apps/frontend/src/pages/TraceSearch.tsx`
- Test: `apps/frontend/src/pages/TraceSearch.test.tsx`

- [ ] **Step 1: Run existing TraceSearch tests (baseline)**

```powershell
cd apps/frontend && npx vitest run src/pages/TraceSearch.test.tsx
```
Expected: all tests PASS. Note count.

- [ ] **Step 2: Rewrite TraceSearch.tsx**

Replace the full content of `apps/frontend/src/pages/TraceSearch.tsx`:

```tsx
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { createDashboard } from "../api/dashboards";
import {
  searchTraces,
  fetchTraceHistogram,
  TraceResponse,
  TraceHistogramBucket as ApiHistogramBucket,
  TraceHistogramResponse,
} from "../api/traces";
import { FacetSidebar } from "../components/FacetSidebar";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { LoadingState } from "../components/ui/loading-state";
import { TablePanel } from "../components/ui/table-panel";
import { Histogram, HistogramBucket } from "../components/ui/histogram";
import { useTimeDisplay } from "../lib/timeDisplay";
import { useSignalSearch } from "../hooks/useSignalSearch";
import { formatBucketLabel } from "../utils/formatBucketLabel";
import { formatTimestamp } from "../utils/formatTimestamp";
import { formatContextValue } from "../utils/logFormatting";
import { infraLinks } from "../utils/infraLinks";
import { SignalExplorer, SaveStatus } from "../components/shared/SignalExplorer";
import { TraceResultsTable } from "../features/signals/components/TraceResultsTable";

export type TraceExplorerProps = {
  initialService?: string;
  lockedService?: boolean;
  initialLookbackMinutes?: number;
  showHeader?: boolean;
  showServiceColumn?: boolean;
  showPromote?: boolean;
  showFacets?: boolean;
  tableAriaLabel?: string;
  tableMode?: "select" | "link";
};

export default function TraceSearch() {
  return (
    <TraceExplorer
      initialService={new URLSearchParams(window.location.search).get("service") ?? ""}
    />
  );
}

export function TraceExplorer({
  initialService = "",
  lockedService = false,
  initialLookbackMinutes = 60,
  showHeader = true,
  showServiceColumn = true,
  showPromote = true,
  showFacets = true,
  tableAriaLabel,
  tableMode = "select",
}: TraceExplorerProps) {
  const { format } = useTimeDisplay();
  const {
    service,
    setService,
    lookbackMinutes,
    setLookbackMinutes,
    customRangeMs,
    handleHistogramRangeSelect,
    handleClearRange,
    from,
    to,
    histogramFromMs,
    histogramToMs,
  } = useSignalSearch({ initialService, initialLookbackMinutes });
  const [bucketCount, setBucketCount] = useState(60);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");

  const { data, isLoading, error } = useQuery({
    queryKey: ["traces", service, from, to],
    queryFn: () =>
      searchTraces({
        service: service || undefined,
        from,
        to,
        limit: 50,
        facets: ["service_name", "status_code", "span_kind"],
      }),
  });

  const { data: histogramData, isError: isHistogramError } = useQuery({
    queryKey: ["traces-histogram", service, from, to, bucketCount],
    queryFn: () =>
      fetchTraceHistogram({
        service: service || undefined,
        from,
        to: new Date(histogramToMs).toISOString(),
        buckets: bucketCount,
      }),
    placeholderData: (prev: TraceHistogramResponse | undefined) => prev,
  });

  const traces = data?.traces ?? [];
  const canRenderHistogram = Boolean(histogramData) || traces.length > 0;
  const histogram = useMemo(
    () =>
      histogramData?.buckets?.length
        ? histogramFromApi(histogramData.buckets)
        : buildTraceHistogram(traces, histogramFromMs, histogramToMs),
    [histogramData, histogramFromMs, histogramToMs, traces],
  );

  const handleFacetClick = (field: string, value: string) => {
    if (field === "service_name") setService(value);
  };

  const handlePromote = async () => {
    setSaveStatus("saving");
    try {
      await createDashboard({
        name: service ? `Traces for ${service}` : "Promoted trace query",
        panels: [
          {
            title: service ? `Traces for ${service}` : "Trace search",
            query_kind: "traces",
            service: service || undefined,
            lookback_minutes: lookbackMinutes,
            filters: { facets: ["service_name", "status_code", "span_kind"] },
          },
        ],
      });
      setSaveStatus("saved");
    } catch {
      setSaveStatus("error");
    }
  };

  return (
    <SignalExplorer
      title="Traces"
      service={service}
      onServiceChange={(s) => { setService(s); }}
      lookbackMinutes={lookbackMinutes}
      onLookbackChange={(m) => { setLookbackMinutes(m); }}
      customRangeMs={customRangeMs}
      customRangeLabel={
        customRangeMs
          ? `${formatBucketLabel(customRangeMs.fromMs, format)} – ${formatBucketLabel(customRangeMs.toMs, format)}`
          : undefined
      }
      onClearRange={handleClearRange}
      lockedService={lockedService}
      showHeader={showHeader}
      showPromote={showPromote}
      saveStatus={saveStatus}
      onPromote={handlePromote}
      histogram={
        canRenderHistogram ? (
          <Histogram
            buckets={histogram}
            categoryOrder={["Traces"]}
            categoryColors={{ Traces: "bg-[var(--brand)]" }}
            format={(ms) => formatBucketLabel(ms, format)}
            onRangeSelect={handleHistogramRangeSelect}
            onBucketCountChange={setBucketCount}
            ariaLabel="Trace volume histogram"
            title="Traces over time"
            subtitle="Volume"
          />
        ) : !isHistogramError ? (
          <div
            aria-hidden="true"
            className="border border-[var(--border)] bg-[var(--surface)] p-3 h-[168px] animate-pulse"
          />
        ) : (
          <p className="text-xs text-[var(--muted)]">Histogram unavailable</p>
        )
      }
      renderTable={(selectedId, onSelect) => (
        <>
          {showFacets && (
            <FacetSidebar
              facets={data?.facets}
              onFacetClick={handleFacetClick}
              ariaLabel="Trace facets"
            />
          )}
          <TablePanel className="flex-1">
            {isLoading ? (
              <LoadingState>Loading traces…</LoadingState>
            ) : error ? (
              <LoadingState className="text-[var(--bad)]">Error loading traces: {String(error)}</LoadingState>
            ) : traces.length === 0 ? (
              <LoadingState>No traces found.</LoadingState>
            ) : (
              <TraceResultsTable
                traces={traces}
                selectedTraceId={selectedId ?? undefined}
                onSelectTrace={(id) => onSelect(id)}
                mode={tableMode}
                showServiceColumn={showServiceColumn}
                timeFormat={format}
                ariaLabel={tableAriaLabel}
              />
            )}
          </TablePanel>
        </>
      )}
      renderPanel={(selectedId, onClose) => {
        const trace = traces.find((t) => t.trace_id === selectedId);
        return trace ? <TraceContextSidebar trace={trace} onClose={onClose} /> : null;
      }}
    />
  );
}

function TraceContextSidebar({
  trace,
  onClose,
}: {
  trace: TraceResponse;
  onClose: () => void;
}) {
  const root = trace.spans[0];
  if (!root) return null;

  const { format } = useTimeDisplay();
  const badges = infraLinks(root.resource_attributes ?? {});

  return (
    <aside
      aria-label="Selected trace context"
      className="w-full border border-[var(--border)] bg-[var(--surface)] p-4"
    >
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Selected Trace</div>
          <h2 className="m-0 text-base font-bold text-[var(--text-strong)]">Root Span Details</h2>
        </div>
        <Button variant="secondary" className="min-h-8 px-2 text-xs" onClick={onClose}>
          Close
        </Button>
      </div>

      <div className="mb-4">
        <Link
          to="/traces/$traceId"
          params={{ traceId: trace.trace_id }}
          className="text-sm font-bold text-[var(--brand)] hover:underline"
        >
          View Full Trace Explorer
        </Link>
      </div>

      <Badge tone={root.status_code === "ERROR" ? "bad" : "good"} className="mb-3">
        {root.status_code}
      </Badge>

      <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">trace_id</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{trace.trace_id}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">start_time</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">
            {formatTimestamp(root.start_time_unix_nano, format)}
          </dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">service.name</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{root.service_name}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">operation</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{root.operation_name}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">duration</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">
            {(root.duration_ns / 1e6).toFixed(2)}ms
          </dd>
        </div>
        {Object.entries(root.resource_attributes ?? {})
          .sort(([a], [b]) => a.localeCompare(b))
          .map(([key, value]) => (
            <div key={key} className="contents">
              <dt className="break-all font-bold text-[var(--muted)]">{key}</dt>
              <dd className="m-0 min-w-0 break-all text-[var(--text)]">{formatContextValue(value)}</dd>
            </div>
          ))}
      </dl>

      {badges.length > 0 && (
        <div className="mt-4 flex flex-wrap gap-1.5">
          {badges.map((link) => (
            <a
              key={link.href}
              href={link.href}
              className="text-[11px] px-1.5 py-0.5 bg-[var(--surface-subtle)] text-[var(--text)] border border-[var(--border)] no-underline whitespace-nowrap hover:border-[var(--brand)] hover:text-[var(--brand)]"
            >
              {link.label}
            </a>
          ))}
        </div>
      )}
    </aside>
  );
}

// ── Histogram helpers ────────────────────────────────────────────────────────

function histogramFromApi(buckets: ApiHistogramBucket[]): HistogramBucket<"Traces">[] {
  return buckets.map((b) => ({
    startMs: b.start_ms,
    endMs: b.end_ms,
    total: b.count,
    categories: { Traces: b.count },
  }));
}

export function buildTraceHistogram(
  _traces: TraceResponse[],
  fromMs: number,
  toMs: number,
): HistogramBucket<"Traces">[] {
  const bucketCount = 30;
  const rangeMs = Math.max(1, toMs - fromMs);
  const bucketMs = rangeMs / bucketCount;
  const buckets = Array.from({ length: bucketCount }, (_, i) => ({
    startMs: fromMs + i * bucketMs,
    endMs: fromMs + (i + 1) * bucketMs,
    total: 0,
    categories: { Traces: 0 },
  }));
  for (const trace of _traces) {
    const root = trace.spans[0];
    if (!root) continue;
    const startMs = Number(root.start_time_unix_nano) / 1_000_000;
    if (!Number.isFinite(startMs)) continue;
    const idx = Math.min(bucketCount - 1, Math.max(0, Math.floor((startMs - fromMs) / bucketMs)));
    buckets[idx].total += 1;
    buckets[idx].categories.Traces += 1;
  }
  return buckets;
}

```

- [ ] **Step 3: Run TraceSearch tests**

```powershell
cd apps/frontend && npx vitest run src/pages/TraceSearch.test.tsx
```
Expected: same tests PASS as baseline.

- [ ] **Step 4: Run typecheck**

```powershell
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/pages/TraceSearch.tsx
git commit -m "refactor: TraceExplorer is now a thin wrapper around SignalExplorer"
```

---

## Task 9: Cleanup and update tests

**Files:**
- Delete: `apps/frontend/src/components/shared/SignalExplorerLayout.tsx`
- Modify: `apps/frontend/src/pages/view-unification.test.ts`

- [ ] **Step 1: Delete the superseded SignalExplorerLayout**

```bash
rm apps/frontend/src/components/shared/SignalExplorerLayout.tsx
```

Verify no imports remain:
```powershell
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors.

- [ ] **Step 2: Update view-unification.test.ts to assert SignalExplorer usage**

In `apps/frontend/src/pages/view-unification.test.ts`, add these imports and tests:

```ts
import signalExplorerSource from "../components/shared/SignalExplorer.tsx?raw";
import logListSource from "../components/shared/LogList.tsx?raw";
```

Add two new test cases inside the `describe` block:

```ts
it("LogExplorer and TraceExplorer both delegate to the shared SignalExplorer shell", () => {
  expect(logSearchSource).toContain("import { SignalExplorer");
  expect(traceSearchSource).toContain("import { SignalExplorer");
});

it("SignalExplorer owns the panel/table layout and toolbar structure", () => {
  expect(signalExplorerSource).toContain("renderTable");
  expect(signalExplorerSource).toContain("renderPanel");
  expect(signalExplorerSource).toContain("w-1/4");
});

it("LogList is the shared mono log-row renderer", () => {
  expect(logListSource).toContain("export function LogList");
  expect(logListSource).toContain("pivotId");
  expect(logListSource).toContain("showTraceLink");
});
```

- [ ] **Step 3: Run all view-unification tests**

```powershell
cd apps/frontend && npx vitest run src/pages/view-unification.test.ts
```
Expected: all tests (existing + new) PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/components/shared/
git add apps/frontend/src/pages/view-unification.test.ts
git commit -m "chore: delete superseded SignalExplorerLayout, update unification tests"
```

---

## Task 10: Full CI gate

- [ ] **Step 1: Run the full frontend check suite**

```bash
bash scripts/local-ci.sh --skip-docker
```
Expected: all checks pass — TypeScript, ESLint, build, and tests.

- [ ] **Step 2: If any test fails, fix before proceeding**

Common failure points:
- Import paths: run `npx tsc --noEmit` to identify unresolved imports
- Test assertions that reference removed elements: check the affected test file
- The `formatTimestamp` stub in Task 8 Step 2 — ensure it was removed and the real import is present

- [ ] **Step 3: Push and open PR**

```bash
git push -u origin <branch-name>
gh pr create --title "refactor: UI deduplication — SignalExplorer, LogList, thin wrappers" \
  --body "$(cat <<'EOF'
## Summary
- Moves LogResultsTable and TraceResultsTable to features/signals/components/
- Extracts shared LogList component used by LogContextView, LogCorrelatedList, and correlated log panels
- Adds SignalExplorer shell (toolbar + 25% left panel + table slot) — LogExplorer and TraceExplorer become thin wrappers
- Adds start-time date column to TraceResultsTable using formatTimestamp
- Extracts duplicate formatBucketLabel helper to utils/

## Test plan
- [ ] All vitest unit tests pass (local-ci.sh --skip-docker)
- [ ] LogExplorer: clicking a log row opens the context panel on the LEFT at 25% width
- [ ] TraceExplorer: clicking a trace row opens trace context panel on the LEFT at 25% width; timestamp column shows date format
- [ ] TraceExplorer: clicking the same row again closes the panel
- [ ] LogCorrelatedList: trace links render with correct aria-label ("View span …" / "View trace …")
- [ ] ServiceDetailPage: log and trace tabs still work (LogExplorer and TraceExplorer render correctly in the scoped context)

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```
