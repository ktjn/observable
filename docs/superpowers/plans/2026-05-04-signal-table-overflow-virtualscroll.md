# Signal Table: Overflow Fix + Virtual Scroll Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix message line overflow in the log and trace signal explorer tables, and add virtual scrolling so the DOM stays fast with large result sets (capped at 500 rows client-side).

**Architecture:** Add a shared `VirtualTable<T>` component backed by `@tanstack/react-virtual`. `LogResultsTable` and `TraceResultsTable` delegate rendering to it. The message/operation `<td>` overrides the global `white-space: nowrap` CSS with `whitespace-normal break-all`. `LogSearch` and `TraceSearch` slice results to 500 rows and show a footer notice when the cap is hit.

**Tech Stack:** React 18, `@tanstack/react-virtual` v3, Tailwind CSS, Vitest, `@testing-library/react`

---

## Files

| File | Action | Responsibility |
|---|---|---|
| `apps/frontend/src/components/ui/VirtualTable.tsx` | Create | Generic virtualised `<table>` scroll container |
| `apps/frontend/src/components/ui/VirtualTable.test.tsx` | Create | Tests for VirtualTable |
| `apps/frontend/src/features/signals/components/LogResultsTable.tsx` | Modify | Use VirtualTable; add message overflow fix; LogResultsRow gets measureRef+index |
| `apps/frontend/src/features/signals/components/LogResultsTable.test.tsx` | Modify | Add @tanstack/react-virtual mock |
| `apps/frontend/src/features/signals/components/TraceResultsTable.tsx` | Modify | Use VirtualTable; add operation overflow fix; TraceResultsRow gets measureRef+index |
| `apps/frontend/src/features/signals/components/TraceResultsTable.test.tsx` | Modify | Add @tanstack/react-virtual mock |
| `apps/frontend/src/pages/LogSearch.tsx` | Modify | Slice logs to 500; show cap footer |
| `apps/frontend/src/pages/TraceSearch.tsx` | Modify | Slice traces to 500; show cap footer |
| `apps/frontend/package.json` | Modify | Add @tanstack/react-virtual |

---

### Task 1: Install @tanstack/react-virtual

**Files:**
- Modify: `apps/frontend/package.json`

- [ ] **Step 1: Install the package**

Run from the repo root:
```bash
npm install @tanstack/react-virtual --workspace=apps/frontend
```

Expected output: `added 1 package` or similar with no errors.

- [ ] **Step 2: Verify it appears in package.json**

Open `apps/frontend/package.json`. Confirm a line like this appears under `"dependencies"`:
```json
"@tanstack/react-virtual": "^3.x.x"
```

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/package.json apps/frontend/package-lock.json
git commit -m "chore: add @tanstack/react-virtual"
```

---

### Task 2: Create VirtualTable component

**Files:**
- Create: `apps/frontend/src/components/ui/VirtualTable.tsx`
- Create: `apps/frontend/src/components/ui/VirtualTable.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `apps/frontend/src/components/ui/VirtualTable.test.tsx` with these contents:

```tsx
import { render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { VirtualTable } from "./VirtualTable";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getVirtualItems: () =>
      Array.from({ length: count }, (_, i) => ({
        key: i,
        index: i,
        start: i * 40,
        end: (i + 1) * 40,
      })),
    getTotalSize: () => count * 40,
    measureElement: (_el: Element | null) => {},
  }),
}));

test("renders table with aria-label, headers, and all rows", () => {
  const rows = ["apple", "banana", "cherry"];
  render(
    <VirtualTable
      rows={rows}
      renderHead={() => (
        <tr>
          <th>Fruit</th>
        </tr>
      )}
      renderRow={(row, ref, index) => (
        <tr key={index} ref={ref} data-index={index}>
          <td>{row}</td>
        </tr>
      )}
      ariaLabel="Fruit table"
    />,
  );

  expect(screen.getByRole("table", { name: "Fruit table" })).toBeInTheDocument();
  expect(screen.getByRole("columnheader", { name: "Fruit" })).toBeInTheDocument();
  expect(screen.getByText("apple")).toBeInTheDocument();
  expect(screen.getByText("banana")).toBeInTheDocument();
  expect(screen.getByText("cherry")).toBeInTheDocument();
});

test("renders an empty table when rows is empty", () => {
  render(
    <VirtualTable
      rows={[]}
      renderHead={() => (
        <tr>
          <th>Fruit</th>
        </tr>
      )}
      renderRow={(row, ref, index) => (
        <tr key={index} ref={ref} data-index={index}>
          <td>{String(row)}</td>
        </tr>
      )}
      ariaLabel="Empty table"
    />,
  );

  expect(screen.getByRole("table", { name: "Empty table" })).toBeInTheDocument();
  expect(screen.queryByRole("cell")).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd apps/frontend && npx vitest run src/components/ui/VirtualTable.test.tsx
```

Expected: FAIL with `Cannot find module './VirtualTable'`

- [ ] **Step 3: Create VirtualTable.tsx**

Create `apps/frontend/src/components/ui/VirtualTable.tsx`:

```tsx
import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

interface VirtualTableProps<T> {
  rows: T[];
  renderHead: () => React.ReactNode;
  renderRow: (row: T, ref: (el: Element | null) => void, index: number) => React.ReactNode;
  estimateSize?: number;
  height?: string;
  ariaLabel?: string;
}

export function VirtualTable<T>({
  rows,
  renderHead,
  renderRow,
  estimateSize = 40,
  height = "600px",
  ariaLabel,
}: VirtualTableProps<T>) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => estimateSize,
    measureElement: (el) => el?.getBoundingClientRect().height ?? estimateSize,
  });

  const virtualItems = virtualizer.getVirtualItems();
  const totalSize = virtualizer.getTotalSize();
  const paddingTop = virtualItems.length > 0 ? virtualItems[0].start : 0;
  const paddingBottom =
    virtualItems.length > 0 ? totalSize - virtualItems[virtualItems.length - 1].end : 0;

  return (
    <div ref={parentRef} style={{ height, overflowY: "auto" }}>
      <table aria-label={ariaLabel}>
        <thead className="sticky top-0 z-10 bg-[var(--surface)]">{renderHead()}</thead>
        <tbody>
          {paddingTop > 0 && (
            <tr aria-hidden="true">
              <td colSpan={999} style={{ height: paddingTop, padding: 0, border: 0 }} />
            </tr>
          )}
          {virtualItems.map((virtualRow) =>
            renderRow(rows[virtualRow.index], virtualizer.measureElement, virtualRow.index),
          )}
          {paddingBottom > 0 && (
            <tr aria-hidden="true">
              <td colSpan={999} style={{ height: paddingBottom, padding: 0, border: 0 }} />
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd apps/frontend && npx vitest run src/components/ui/VirtualTable.test.tsx
```

Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/components/ui/VirtualTable.tsx apps/frontend/src/components/ui/VirtualTable.test.tsx
git commit -m "feat: add VirtualTable component backed by @tanstack/react-virtual"
```

---

### Task 3: Update LogResultsTable — overflow fix + virtual scroll

**Files:**
- Modify: `apps/frontend/src/features/signals/components/LogResultsTable.tsx`
- Modify: `apps/frontend/src/features/signals/components/LogResultsTable.test.tsx`

- [ ] **Step 1: Add the @tanstack/react-virtual mock to the test file**

Replace the entire contents of `apps/frontend/src/features/signals/components/LogResultsTable.test.tsx`:

```tsx
import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { LogRecord } from "../../../api/logs";
import { LogResultsTable } from "./LogResultsTable";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getVirtualItems: () =>
      Array.from({ length: count }, (_, i) => ({
        key: i,
        index: i,
        start: i * 40,
        end: (i + 1) * 40,
      })),
    getTotalSize: () => count * 40,
    measureElement: (_el: Element | null) => {},
  }),
}));

const logs: LogRecord[] = [
  {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    log_id: "log-1",
    timestamp_unix_nano: "1700000000000000000",
    observed_timestamp_unix_nano: "1700000000000000100",
    severity_number: 9,
    severity_text: "INFO",
    body: "checkout completed",
    trace_id: "trace-1",
    span_id: "span-1",
    service_name: "checkout",
    environment: "prod",
    host_id: "node-1",
    attributes: {},
    resource_attributes: {},
  },
];

test("renders the canonical log result columns and selection action", () => {
  const onSelect = vi.fn();

  render(
    <LogResultsTable
      logs={logs}
      selectedLogId={undefined}
      onSelectLog={onSelect}
      timeFormat="iso-utc-ms"
    />,
  );

  const table = screen.getByRole("table", { name: "Log results" });
  expect(within(table).getByRole("columnheader", { name: "Time" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Level" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Service" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Message" })).toBeInTheDocument();

  fireEvent.click(within(table).getByRole("button", { name: "Open log context for checkout completed" }));

  expect(onSelect).toHaveBeenCalledWith("log-1");
});

test("can hide the service column for already scoped service log views", () => {
  render(
    <LogResultsTable
      logs={logs}
      selectedLogId="log-1"
      onSelectLog={vi.fn()}
      timeFormat="iso-utc-ms"
      showServiceColumn={false}
    />,
  );

  const table = screen.getByRole("table", { name: "Service logs" });
  expect(within(table).queryByRole("columnheader", { name: "Service" })).not.toBeInTheDocument();
  expect(within(table).getByText("checkout completed")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run existing tests to confirm the mock is correct before refactoring**

```bash
cd apps/frontend && npx vitest run src/features/signals/components/LogResultsTable.test.tsx
```

Expected: PASS (2 tests). The mock must be in place before the component is changed because `VirtualTable` (which we add next) imports `@tanstack/react-virtual`.

- [ ] **Step 3: Rewrite LogResultsTable.tsx**

Replace the entire contents of `apps/frontend/src/features/signals/components/LogResultsTable.tsx`:

```tsx
import type { LogRecord } from "../../../api/logs";
import { Badge } from "../../../components/ui/badge";
import { VirtualTable } from "../../../components/ui/VirtualTable";
import { formatTimestamp } from "../../../utils/formatTimestamp";
import { formatLogMessage, otelSeverity } from "../../../utils/logFormatting";
import type { TimeFormat } from "../../../lib/timeDisplay";

export function LogResultsTable({
  logs,
  selectedLogId,
  onSelectLog,
  timeFormat,
  showServiceColumn = true,
  ariaLabel = showServiceColumn ? "Log results" : "Service logs",
}: {
  logs: LogRecord[];
  selectedLogId: string | undefined;
  onSelectLog: (logId: string) => void;
  timeFormat: TimeFormat;
  showServiceColumn?: boolean;
  ariaLabel?: string;
}) {
  return (
    <VirtualTable
      rows={logs}
      ariaLabel={ariaLabel}
      renderHead={() => (
        <tr>
          <th aria-label="Time">Time</th>
          <th>Level</th>
          {showServiceColumn && <th>Service</th>}
          <th>Message</th>
        </tr>
      )}
      renderRow={(log, ref, index) => (
        <LogResultsRow
          key={log.log_id}
          log={log}
          timeFormat={timeFormat}
          selected={selectedLogId === log.log_id}
          onSelect={() => onSelectLog(log.log_id)}
          showServiceColumn={showServiceColumn}
          measureRef={ref}
          index={index}
        />
      )}
    />
  );
}

function LogResultsRow({
  log,
  timeFormat,
  selected,
  onSelect,
  showServiceColumn,
  measureRef,
  index,
}: {
  log: LogRecord;
  timeFormat: TimeFormat;
  selected: boolean;
  onSelect: () => void;
  showServiceColumn: boolean;
  measureRef: (el: Element | null) => void;
  index: number;
}) {
  const severity = otelSeverity(log.severity_number);
  const message = formatLogMessage(log.body);

  return (
    <tr
      ref={measureRef}
      data-index={index}
      className={`modern-table-row cursor-pointer ${selected ? "bg-[var(--surface-subtle)]" : ""}`}
      onClick={onSelect}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && onSelect()}
      tabIndex={0}
      role="button"
      aria-label={`Open log context for ${message}`}
      aria-pressed={selected}
    >
      <td className="whitespace-nowrap">{formatTimestamp(log.timestamp_unix_nano, timeFormat)}</td>
      <td>
        <Badge tone={severity.tone}>{severity.label}</Badge>
      </td>
      {showServiceColumn && <td>{log.service_name}</td>}
      <td className="whitespace-normal break-all">{message}</td>
    </tr>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd apps/frontend && npx vitest run src/features/signals/components/LogResultsTable.test.tsx
```

Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/features/signals/components/LogResultsTable.tsx apps/frontend/src/features/signals/components/LogResultsTable.test.tsx
git commit -m "feat: use VirtualTable in LogResultsTable; fix message cell overflow"
```

---

### Task 4: Update TraceResultsTable — overflow fix + virtual scroll

**Files:**
- Modify: `apps/frontend/src/features/signals/components/TraceResultsTable.tsx`
- Modify: `apps/frontend/src/features/signals/components/TraceResultsTable.test.tsx`

- [ ] **Step 1: Add both mocks to the test file**

Replace the entire contents of `apps/frontend/src/features/signals/components/TraceResultsTable.test.tsx`:

```tsx
import { fireEvent, render, screen, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { expect, test, vi } from "vitest";
import type { TraceResponse } from "../../../api/traces";
import { TraceResultsTable } from "./TraceResultsTable";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getVirtualItems: () =>
      Array.from({ length: count }, (_, i) => ({
        key: i,
        index: i,
        start: i * 40,
        end: (i + 1) * 40,
      })),
    getTotalSize: () => count * 40,
    measureElement: (_el: Element | null) => {},
  }),
}));

vi.mock("@tanstack/react-router", () => ({
  Link: ({
    to,
    params,
    children,
  }: {
    to: string;
    params?: Record<string, string>;
    children: ReactNode;
  }) => {
    let href = to;
    if (params) {
      for (const [key, value] of Object.entries(params)) {
        href = href.replace(`$${key}`, value);
      }
    }
    return <a href={href}>{children}</a>;
  },
}));

const traces: TraceResponse[] = [
  {
    trace_id: "trace-abc-1234567890",
    events: [],
    spans: [
      {
        tenant_id: "00000000-0000-0000-0000-000000000001",
        trace_id: "trace-abc-1234567890",
        span_id: "span-root",
        service_name: "checkout",
        service_namespace: "shop",
        service_version: "2026.04.30",
        operation_name: "GET /checkout",
        span_kind: "SERVER",
        start_time_unix_nano: 1,
        end_time_unix_nano: 5000001,
        duration_ns: 5000000,
        status_code: "OK",
        status_message: "",
        attributes: {},
        resource_attributes: {},
        environment: "prod",
        host_id: "host-1",
        workload: "checkout-api",
        deployment_id: "deploy-1",
      },
    ],
  },
];

test("renders selectable trace rows for the global explorer", () => {
  const onSelect = vi.fn();

  render(
    <TraceResultsTable
      traces={traces}
      selectedTraceId={undefined}
      onSelectTrace={onSelect}
    />,
  );

  const table = screen.getByRole("table", { name: "Trace results" });
  expect(within(table).getByRole("columnheader", { name: "Trace ID" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Service" })).toBeInTheDocument();
  expect(within(table).getByText("GET /checkout")).toBeInTheDocument();

  fireEvent.click(within(table).getByRole("button", { name: "trace-abc-123456…" }));

  expect(onSelect).toHaveBeenCalledWith("trace-abc-1234567890");
});

test("renders linked trace rows for scoped service views", () => {
  render(
    <TraceResultsTable
      traces={traces}
      selectedTraceId={undefined}
      onSelectTrace={vi.fn()}
      mode="link"
      showServiceColumn={false}
      ariaLabel="Service traces"
    />,
  );

  const table = screen.getByRole("table", { name: "Service traces" });
  expect(within(table).queryByRole("columnheader", { name: "Service" })).not.toBeInTheDocument();
  expect(within(table).getByRole("link", { name: "trace-abc-123456" })).toHaveAttribute(
    "href",
    "/traces/trace-abc-1234567890",
  );
});

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
  expect(within(table).getByText("1970-01-01 00:00:00.000Z")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run existing tests to confirm the mock is correct before refactoring**

```bash
cd apps/frontend && npx vitest run src/features/signals/components/TraceResultsTable.test.tsx
```

Expected: PASS (3 tests)

- [ ] **Step 3: Rewrite TraceResultsTable.tsx**

Replace the entire contents of `apps/frontend/src/features/signals/components/TraceResultsTable.tsx`:

```tsx
import { Link } from "@tanstack/react-router";
import type { TraceResponse } from "../../../api/traces";
import { Badge } from "../../../components/ui/badge";
import { VirtualTable } from "../../../components/ui/VirtualTable";
import { formatTimestamp } from "../../../utils/formatTimestamp";
import type { TimeFormat } from "../../../lib/timeDisplay";

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
    <VirtualTable
      rows={traces}
      ariaLabel={ariaLabel}
      renderHead={() => (
        <tr>
          <th aria-label="Time">Time</th>
          <th>Trace ID</th>
          {showServiceColumn && <th>Service</th>}
          <th>Operation</th>
          <th>Duration</th>
          <th>Status</th>
        </tr>
      )}
      renderRow={(trace, ref, index) => (
        <TraceResultsRow
          key={trace.trace_id}
          trace={trace}
          selected={selectedTraceId === trace.trace_id}
          onSelect={() => onSelectTrace(trace.trace_id)}
          mode={mode}
          showServiceColumn={showServiceColumn}
          timeFormat={timeFormat}
          measureRef={ref}
          index={index}
        />
      )}
    />
  );
}

function TraceResultsRow({
  trace,
  selected,
  onSelect,
  mode,
  showServiceColumn,
  timeFormat,
  measureRef,
  index,
}: {
  trace: TraceResponse;
  selected: boolean;
  onSelect: () => void;
  mode: "select" | "link";
  showServiceColumn: boolean;
  timeFormat: TimeFormat;
  measureRef: (el: Element | null) => void;
  index: number;
}) {
  const root = trace.spans[0];
  if (!root) return null;

  return (
    <tr
      ref={measureRef}
      data-index={index}
      className={`modern-table-row ${mode === "select" ? "cursor-pointer" : ""} ${selected ? "bg-[var(--surface-subtle)]" : ""}`}
      onClick={mode === "select" ? onSelect : undefined}
      onKeyDown={
        mode === "select" ? (e) => (e.key === "Enter" || e.key === " ") && onSelect() : undefined
      }
      tabIndex={mode === "select" ? 0 : undefined}
      role={mode === "select" ? "button" : undefined}
      aria-label={mode === "select" ? `${trace.trace_id.substring(0, 16)}…` : undefined}
      aria-pressed={mode === "select" ? selected : undefined}
    >
      <td className="whitespace-nowrap">{formatTimestamp(root.start_time_unix_nano, timeFormat)}</td>
      <td className="strong-cell">
        {mode === "link" ? (
          <Link to="/traces/$traceId" params={{ traceId: trace.trace_id }}>
            {trace.trace_id.substring(0, 16)}
          </Link>
        ) : (
          <Link
            to="/traces/$traceId"
            params={{ traceId: trace.trace_id }}
            onClick={(e) => e.stopPropagation()}
          >
            {trace.trace_id.substring(0, 16)}…
          </Link>
        )}
      </td>
      {showServiceColumn && <td>{root.service_name}</td>}
      <td className="whitespace-normal break-all">{root.operation_name}</td>
      <td>{(root.duration_ns / 1e6).toFixed(2)}ms</td>
      <td>
        <Badge tone={root.status_code === "ERROR" ? "bad" : "good"}>{root.status_code}</Badge>
      </td>
    </tr>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd apps/frontend && npx vitest run src/features/signals/components/TraceResultsTable.test.tsx
```

Expected: PASS (3 tests)

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/features/signals/components/TraceResultsTable.tsx apps/frontend/src/features/signals/components/TraceResultsTable.test.tsx
git commit -m "feat: use VirtualTable in TraceResultsTable; fix operation cell overflow"
```

---

### Task 5: Cap log results to 500 rows in LogSearch

**Files:**
- Modify: `apps/frontend/src/pages/LogSearch.tsx`

- [ ] **Step 1: Update log derivation to apply the cap**

In `apps/frontend/src/pages/LogSearch.tsx`, find this line (around line 117):

```tsx
const logs = data ?? [];
```

Replace it with:

```tsx
const ROW_LIMIT = 500;
const rawLogs = data ?? [];
const logs = rawLogs.slice(0, ROW_LIMIT);
const isCapped = rawLogs.length >= ROW_LIMIT;
```

- [ ] **Step 2: Wrap the LogResultsTable with a fragment and add the cap footer**

In the same file, find the `renderTable` prop (around line 186). The current code inside the ternary's truthy branch is:

```tsx
<LogResultsTable
  logs={logs}
  selectedLogId={selectedId ?? undefined}
  onSelectLog={(id) => onSelect(id)}
  timeFormat={format}
  showServiceColumn={showServiceColumn}
  ariaLabel={tableAriaLabel}
/>
```

Replace it with:

```tsx
<>
  <LogResultsTable
    logs={logs}
    selectedLogId={selectedId ?? undefined}
    onSelectLog={(id) => onSelect(id)}
    timeFormat={format}
    showServiceColumn={showServiceColumn}
    ariaLabel={tableAriaLabel}
  />
  {isCapped && (
    <p className="px-3 py-2 text-xs text-[var(--muted)] border-t border-[var(--border)]">
      Showing {ROW_LIMIT} results — narrow the time range or add filters to see fewer.
    </p>
  )}
</>
```

- [ ] **Step 3: Run all frontend tests**

```bash
cd apps/frontend && npx vitest run
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/pages/LogSearch.tsx
git commit -m "feat: cap log results to 500 rows with footer notice"
```

---

### Task 6: Cap trace results to 500 rows in TraceSearch

**Files:**
- Modify: `apps/frontend/src/pages/TraceSearch.tsx`

- [ ] **Step 1: Update trace derivation to apply the cap**

In `apps/frontend/src/pages/TraceSearch.tsx`, find this line (around line 150):

```tsx
const traces = data ?? [];
```

Replace it with:

```tsx
const ROW_LIMIT = 500;
const rawTraces = data ?? [];
const traces = rawTraces.slice(0, ROW_LIMIT);
const isCapped = rawTraces.length >= ROW_LIMIT;
```

- [ ] **Step 2: Wrap the TraceResultsTable with a fragment and add the cap footer**

In the same file, find the `renderTable` prop (around line 220). The current truthy branch inside `TablePanel` is:

```tsx
<TraceResultsTable
  traces={traces}
  selectedTraceId={selectedId ?? undefined}
  onSelectTrace={(id) => onSelect(id)}
  mode={tableMode}
  showServiceColumn={showServiceColumn}
  timeFormat={format}
  ariaLabel={tableAriaLabel}
/>
```

Replace it with:

```tsx
<>
  <TraceResultsTable
    traces={traces}
    selectedTraceId={selectedId ?? undefined}
    onSelectTrace={(id) => onSelect(id)}
    mode={tableMode}
    showServiceColumn={showServiceColumn}
    timeFormat={format}
    ariaLabel={tableAriaLabel}
  />
  {isCapped && (
    <p className="px-3 py-2 text-xs text-[var(--muted)] border-t border-[var(--border)]">
      Showing {ROW_LIMIT} results — narrow the time range or add filters to see fewer.
    </p>
  )}
</>
```

- [ ] **Step 3: Run all frontend tests**

```bash
cd apps/frontend && npx vitest run
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/pages/TraceSearch.tsx
git commit -m "feat: cap trace results to 500 rows with footer notice"
```
