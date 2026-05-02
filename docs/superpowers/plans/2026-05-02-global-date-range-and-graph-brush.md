# Global Date Range Selector & Graph Brush Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace per-page lookback-minutes state with a global URL-backed date range selector in the AppShell header, and add brush selection to TimeSeriesGraph.

**Architecture:** A `validateSearch` schema on the root TanStack Router route holds `preset` (e.g. `1h`) or `from`/`to` (ms timestamps). A `useGlobalDateRange` hook reads and writes these params. The AppShell header renders a `GlobalDateRangePicker` component. All pages consuming date range call `useGlobalDateRange` instead of local state. Histogram and TimeSeriesGraph brush selections call `setCustomRange` from the hook, updating the URL.

**Tech Stack:** React 19, TanStack Router v1, TanStack Query v5, Vitest, Testing Library

---

## File Map

| Status | File | Change |
|--------|------|--------|
| Modify | `apps/frontend/src/router.ts` | Add `validateSearch` to root route |
| Create | `apps/frontend/src/hooks/useGlobalDateRange.ts` | New hook |
| Create | `apps/frontend/src/hooks/useGlobalDateRange.test.ts` | Hook unit tests (pure logic) |
| Create | `apps/frontend/src/components/GlobalDateRangePicker.tsx` | New component |
| Create | `apps/frontend/src/components/GlobalDateRangePicker.test.tsx` | Component tests |
| Modify | `apps/frontend/src/components/AppShell.tsx` | Replace static pill |
| Modify | `apps/frontend/src/components/ui/time-series-graph.tsx` | Add brush selection |
| Modify | `apps/frontend/src/components/ui/time-series-graph.test.tsx` | Add brush tests |
| Modify | `apps/frontend/src/components/shared/SignalExplorer.tsx` | Remove date controls |
| Modify | `apps/frontend/src/components/shared/SignalExplorer.test.tsx` | Remove date prop fixtures |
| Modify | `apps/frontend/src/pages/LogSearch.tsx` | Consume global range |
| Modify | `apps/frontend/src/pages/TraceSearch.tsx` | Consume global range |
| Modify | `apps/frontend/src/pages/ServiceDetailPage.tsx` | Consume global range |
| Modify | `apps/frontend/src/hooks/useSignalSearch.ts` | Trim to service-only |

---

## Task 1: Root route search schema

**Files:**
- Modify: `apps/frontend/src/router.ts`

- [ ] **Step 1: Add `validateSearch` to `createRootRoute`**

Open `apps/frontend/src/router.ts`. Replace:
```ts
const rootRoute = createRootRoute({
  component: AppShell,
});
```
With:
```ts
export type Preset = "5m" | "15m" | "30m" | "1h" | "3h" | "12h";
export const DEFAULT_PRESET: Preset = "1h";

export type RootSearch = {
  preset?: Preset;
  from?: number;
  to?: number;
};

const VALID_PRESETS = new Set<string>(["5m", "15m", "30m", "1h", "3h", "12h"]);

const rootRoute = createRootRoute({
  component: AppShell,
  validateSearch: (search: Record<string, unknown>): RootSearch => {
    const raw = search.preset;
    const preset = typeof raw === "string" && VALID_PRESETS.has(raw)
      ? (raw as Preset)
      : undefined;
    const from = typeof search.from === "number" ? search.from
      : typeof search.from === "string" ? Number(search.from) || undefined
      : undefined;
    const to = typeof search.to === "number" ? search.to
      : typeof search.to === "string" ? Number(search.to) || undefined
      : undefined;
    return { preset, from, to };
  },
});
```

- [ ] **Step 2: Run typecheck**

```
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors related to `validateSearch`.

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/router.ts
git commit -m "feat: add root route search schema for global date range"
```

---

## Task 2: `useGlobalDateRange` hook

**Files:**
- Create: `apps/frontend/src/hooks/useGlobalDateRange.ts`
- Create: `apps/frontend/src/hooks/useGlobalDateRange.test.ts`

- [ ] **Step 1: Write tests for the pure `deriveRange` helper**

Create `apps/frontend/src/hooks/useGlobalDateRange.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { deriveRange, presetToMs, PRESET_OPTIONS, DEFAULT_PRESET } from "./useGlobalDateRange";

describe("presetToMs", () => {
  it("converts 5m to 5 minutes in ms", () => {
    expect(presetToMs("5m")).toBe(5 * 60 * 1000);
  });
  it("converts 1h to 60 minutes in ms", () => {
    expect(presetToMs("1h")).toBe(60 * 60 * 1000);
  });
  it("converts 12h to 720 minutes in ms", () => {
    expect(presetToMs("12h")).toBe(12 * 60 * 60 * 1000);
  });
});

describe("deriveRange", () => {
  it("uses from/to when both are present", () => {
    const result = deriveRange({ from: 1000, to: 2000 });
    expect(result).toEqual({ fromMs: 1000, toMs: 2000 });
  });

  it("falls back to preset when from/to are absent", () => {
    const now = 100_000;
    vi.spyOn(Date, "now").mockReturnValue(now);
    const result = deriveRange({ preset: "5m" });
    expect(result).toEqual({ fromMs: now - 5 * 60 * 1000, toMs: now });
    vi.restoreAllMocks();
  });

  it("uses DEFAULT_PRESET when nothing is provided", () => {
    const now = 100_000;
    vi.spyOn(Date, "now").mockReturnValue(now);
    const result = deriveRange({});
    expect(result).toEqual({ fromMs: now - presetToMs(DEFAULT_PRESET), toMs: now });
    vi.restoreAllMocks();
  });
});

describe("PRESET_OPTIONS", () => {
  it("has 6 options", () => {
    expect(PRESET_OPTIONS).toHaveLength(6);
  });
  it("starts with 5m and ends with 12h", () => {
    expect(PRESET_OPTIONS[0].value).toBe("5m");
    expect(PRESET_OPTIONS[5].value).toBe("12h");
  });
});
```

- [ ] **Step 2: Run the tests to confirm they fail**

```
cd apps/frontend && npx vitest run src/hooks/useGlobalDateRange.test.ts
```
Expected: FAIL — module not found.

- [ ] **Step 3: Create the hook file**

Create `apps/frontend/src/hooks/useGlobalDateRange.ts`:

```ts
import { useMemo } from "react";
import { useNavigate, useSearch } from "@tanstack/react-router";
import type { Preset, RootSearch } from "../router";
export { DEFAULT_PRESET } from "../router";
export type { Preset } from "../router";

export type PresetOption = { value: Preset; label: string };

export const PRESET_OPTIONS: PresetOption[] = [
  { value: "5m",  label: "Last 5 min" },
  { value: "15m", label: "Last 15 min" },
  { value: "30m", label: "Last 30 min" },
  { value: "1h",  label: "Last 1 hour" },
  { value: "3h",  label: "Last 3 hours" },
  { value: "12h", label: "Last 12 hours" },
];

const PRESET_MS: Record<Preset, number> = {
  "5m":  5  * 60 * 1000,
  "15m": 15 * 60 * 1000,
  "30m": 30 * 60 * 1000,
  "1h":  60 * 60 * 1000,
  "3h":  3  * 60 * 60 * 1000,
  "12h": 12 * 60 * 60 * 1000,
};

export function presetToMs(preset: Preset): number {
  return PRESET_MS[preset];
}

export function deriveRange(search: Partial<RootSearch>): { fromMs: number; toMs: number } {
  if (search.from != null && search.to != null) {
    return { fromMs: search.from, toMs: search.to };
  }
  const preset = search.preset ?? "1h";
  const toMs = Date.now();
  return { fromMs: toMs - presetToMs(preset), toMs };
}

export interface GlobalDateRange {
  preset: Preset | null;
  fromMs: number;
  toMs: number;
  setPreset: (p: Preset) => void;
  setCustomRange: (from: number, to: number) => void;
  clearCustomRange: () => void;
}

export function useGlobalDateRange(): GlobalDateRange {
  const search = useSearch({ strict: false }) as RootSearch;
  const navigate = useNavigate();

  const isCustom = search.from != null && search.to != null;
  const preset: Preset | null = isCustom ? null : (search.preset ?? "1h");

  const { fromMs, toMs } = useMemo(() => deriveRange(search), [search.preset, search.from, search.to]);

  function setPreset(p: Preset) {
    navigate({
      search: (prev: Record<string, unknown>) => ({
        ...prev,
        preset: p,
        from: undefined,
        to: undefined,
      }),
    });
  }

  function setCustomRange(from: number, to: number) {
    navigate({
      search: (prev: Record<string, unknown>) => ({
        ...prev,
        preset: undefined,
        from,
        to,
      }),
    });
  }

  function clearCustomRange() {
    navigate({
      search: (prev: Record<string, unknown>) => ({
        ...prev,
        preset: "1h",
        from: undefined,
        to: undefined,
      }),
    });
  }

  return { preset, fromMs, toMs, setPreset, setCustomRange, clearCustomRange };
}
```

- [ ] **Step 4: Run the tests to confirm they pass**

```
cd apps/frontend && npx vitest run src/hooks/useGlobalDateRange.test.ts
```
Expected: PASS — all 6 tests green.

- [ ] **Step 5: Run typecheck**

```
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/hooks/useGlobalDateRange.ts apps/frontend/src/hooks/useGlobalDateRange.test.ts
git commit -m "feat: add useGlobalDateRange hook with URL-backed preset and custom range"
```

---

## Task 3: GlobalDateRangePicker component + AppShell update

**Files:**
- Create: `apps/frontend/src/components/GlobalDateRangePicker.tsx`
- Create: `apps/frontend/src/components/GlobalDateRangePicker.test.tsx`
- Modify: `apps/frontend/src/components/AppShell.tsx`

- [ ] **Step 1: Write component tests**

Create `apps/frontend/src/components/GlobalDateRangePicker.test.tsx`:

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { vi, expect, test, beforeEach } from "vitest";

const mockSetPreset = vi.fn();
const mockSetCustomRange = vi.fn();
const mockClearCustomRange = vi.fn();

vi.mock("../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: vi.fn(() => ({
    preset: "1h",
    fromMs: 1000,
    toMs: 4600000,
    setPreset: mockSetPreset,
    setCustomRange: mockSetCustomRange,
    clearCustomRange: mockClearCustomRange,
  })),
  PRESET_OPTIONS: [
    { value: "5m",  label: "Last 5 min" },
    { value: "15m", label: "Last 15 min" },
    { value: "30m", label: "Last 30 min" },
    { value: "1h",  label: "Last 1 hour" },
    { value: "3h",  label: "Last 3 hours" },
    { value: "12h", label: "Last 12 hours" },
  ],
}));

import { GlobalDateRangePicker } from "./GlobalDateRangePicker";
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";

beforeEach(() => {
  vi.clearAllMocks();
});

test("renders a dropdown with preset options when no custom range", () => {
  render(<GlobalDateRangePicker />);
  const select = screen.getByRole("combobox", { name: /time range/i });
  expect(select).toBeInTheDocument();
  expect(screen.getByRole("option", { name: "Last 1 hour" })).toBeInTheDocument();
  expect(screen.getByRole("option", { name: "Last 5 min" })).toBeInTheDocument();
  expect(screen.getByRole("option", { name: "Last 12 hours" })).toBeInTheDocument();
});

test("selecting a different preset calls setPreset", () => {
  render(<GlobalDateRangePicker />);
  fireEvent.change(screen.getByRole("combobox"), { target: { value: "3h" } });
  expect(mockSetPreset).toHaveBeenCalledWith("3h");
});

test("shows custom range label and reset button when preset is null", () => {
  vi.mocked(useGlobalDateRange).mockReturnValue({
    preset: null,
    fromMs: 1746100800000,
    toMs: 1746104400000,
    setPreset: mockSetPreset,
    setCustomRange: mockSetCustomRange,
    clearCustomRange: mockClearCustomRange,
  });
  render(<GlobalDateRangePicker />);
  expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: /reset/i })).toBeInTheDocument();
});

test("reset button calls clearCustomRange", () => {
  vi.mocked(useGlobalDateRange).mockReturnValue({
    preset: null,
    fromMs: 1746100800000,
    toMs: 1746104400000,
    setPreset: mockSetPreset,
    setCustomRange: mockSetCustomRange,
    clearCustomRange: mockClearCustomRange,
  });
  render(<GlobalDateRangePicker />);
  fireEvent.click(screen.getByRole("button", { name: /reset/i }));
  expect(mockClearCustomRange).toHaveBeenCalledOnce();
});
```

- [ ] **Step 2: Run the tests to confirm they fail**

```
cd apps/frontend && npx vitest run src/components/GlobalDateRangePicker.test.tsx
```
Expected: FAIL — module not found.

- [ ] **Step 3: Create the component**

Create `apps/frontend/src/components/GlobalDateRangePicker.tsx`:

```tsx
import { useGlobalDateRange, PRESET_OPTIONS } from "../hooks/useGlobalDateRange";

export function GlobalDateRangePicker() {
  const { preset, fromMs, toMs, setPreset, clearCustomRange } = useGlobalDateRange();

  if (preset === null) {
    return (
      <div className="flex items-center gap-2">
        <span className="context-pill font-mono text-xs">
          {formatMs(fromMs)} → {formatMs(toMs)}
        </span>
        <button
          type="button"
          className="context-pill"
          style={{ cursor: "pointer" }}
          aria-label="Reset time range"
          onClick={clearCustomRange}
        >
          Reset range
        </button>
      </div>
    );
  }

  return (
    <select
      aria-label="Global time range"
      className="context-pill"
      value={preset}
      onChange={(e) => setPreset(e.target.value as typeof preset)}
      style={{ cursor: "pointer", background: "var(--surface)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: "var(--radius, 4px)", padding: "2px 6px", fontSize: "inherit" }}
    >
      {PRESET_OPTIONS.map((opt) => (
        <option key={opt.value} value={opt.value}>{opt.label}</option>
      ))}
    </select>
  );
}

function formatMs(ms: number): string {
  const d = new Date(ms);
  return `${d.getMonth() + 1}/${d.getDate()} ${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`;
}
```

- [ ] **Step 4: Run the tests to confirm they pass**

```
cd apps/frontend && npx vitest run src/components/GlobalDateRangePicker.test.tsx
```
Expected: PASS — all 4 tests green.

- [ ] **Step 5: Update AppShell to use GlobalDateRangePicker**

In `apps/frontend/src/components/AppShell.tsx`:

Add import at top:
```ts
import { GlobalDateRangePicker } from "./GlobalDateRangePicker";
```

Replace the static span:
```tsx
<span className="context-pill">Last 1h</span>
```
With:
```tsx
<GlobalDateRangePicker />
```

- [ ] **Step 6: Run typecheck**

```
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors.

- [ ] **Step 7: Run all tests**

```
cd apps/frontend && npx vitest run
```
Expected: no new failures.

- [ ] **Step 8: Commit**

```bash
git add apps/frontend/src/components/GlobalDateRangePicker.tsx apps/frontend/src/components/GlobalDateRangePicker.test.tsx apps/frontend/src/components/AppShell.tsx
git commit -m "feat: add GlobalDateRangePicker to AppShell header"
```

---

## Task 4: Add brush selection to TimeSeriesGraph

**Files:**
- Modify: `apps/frontend/src/components/ui/time-series-graph.tsx`
- Modify: `apps/frontend/src/components/ui/time-series-graph.test.tsx`

- [ ] **Step 1: Write tests for `pixelToMs` and brush interaction**

Add to `apps/frontend/src/components/ui/time-series-graph.test.tsx`:

```ts
import { pixelToMs } from "./time-series-graph";
import { render, screen } from "@testing-library/react";
import { fireEvent } from "@testing-library/react";
import { vi } from "vitest";

describe("pixelToMs", () => {
  it("maps x=0 to rangeStartMs", () => {
    expect(pixelToMs(0, 1000, 2000, 400)).toBe(1000);
  });
  it("maps x=width to rangeEndMs", () => {
    expect(pixelToMs(400, 1000, 2000, 400)).toBe(2000);
  });
  it("maps x=200 to midpoint", () => {
    expect(pixelToMs(200, 1000, 2000, 400)).toBe(1500);
  });
});

describe("TimeSeriesGraph brush", () => {
  it("does not fire onRangeSelect when prop is not provided", () => {
    const onRangeSelect = vi.fn();
    render(
      <TimeSeriesGraph
        series={[]}
        rangeStartMs={1000}
        rangeEndMs={2000}
        ariaLabel="test graph"
      />
    );
    const svg = document.querySelector("svg")!;
    fireEvent.pointerDown(svg, { clientX: 10 });
    fireEvent.pointerUp(svg, { clientX: 50 });
    expect(onRangeSelect).not.toHaveBeenCalled();
  });

  it("fires onRangeSelect with from/to ms when brush drag completes", () => {
    const onRangeSelect = vi.fn();
    const rangeStartMs = 0;
    const rangeEndMs = 1000;
    render(
      <TimeSeriesGraph
        series={[]}
        rangeStartMs={rangeStartMs}
        rangeEndMs={rangeEndMs}
        onRangeSelect={onRangeSelect}
        ariaLabel="test graph"
      />
    );
    const svg = document.querySelector("svg")!;
    Object.defineProperty(svg, "getBoundingClientRect", {
      value: () => ({ left: 0, width: 400, top: 0, height: 80 } as DOMRect),
    });
    fireEvent.pointerDown(svg, { clientX: 0, pointerId: 1 });
    fireEvent.pointerMove(svg, { clientX: 200, pointerId: 1 });
    fireEvent.pointerUp(svg, { clientX: 200, pointerId: 1 });
    expect(onRangeSelect).toHaveBeenCalledOnce();
    const [from, to] = onRangeSelect.mock.calls[0];
    expect(from).toBe(0);
    expect(to).toBe(500);
  });
});
```

- [ ] **Step 2: Run the new tests to confirm they fail**

```
cd apps/frontend && npx vitest run src/components/ui/time-series-graph.test.tsx
```
Expected: `pixelToMs` tests FAIL (not exported), brush tests FAIL (prop not yet added).

- [ ] **Step 3: Add `onRangeSelect` prop and brush logic to TimeSeriesGraph**

In `apps/frontend/src/components/ui/time-series-graph.tsx`:

Add `pixelToMs` export function after `toX`:
```ts
export function pixelToMs(
  x: number,
  rangeStartMs: number,
  rangeEndMs: number,
  width: number,
): number {
  const span = rangeEndMs - rangeStartMs;
  if (width <= 0) return rangeStartMs;
  return Math.round(rangeStartMs + (x / width) * span);
}
```

Add `onRangeSelect` to `TimeSeriesGraphProps`:
```ts
export interface TimeSeriesGraphProps {
  series: TimeSeriesSeries[];
  deploymentMarkers?: DeploymentMarker[];
  rangeStartMs: number;
  rangeEndMs: number;
  height?: number;
  title?: string;
  eyebrow?: string;
  ariaLabel?: string;
  onRangeSelect?: (fromMs: number, toMs: number) => void;  // ADD THIS LINE
}
```

Add drag state after existing `useState` declarations in `TimeSeriesGraph`:
```ts
const dragRef = useRef<{ startX: number; endX: number } | null>(null);
const [dragDisplay, setDragDisplay] = useState<{ startX: number; endX: number } | null>(null);
```

Add three event handlers before the `return`:
```ts
function handlePointerDown(e: React.PointerEvent<SVGSVGElement>) {
  if (!onRangeSelect) return;
  const rect = e.currentTarget.getBoundingClientRect();
  const x = Math.round(e.clientX - rect.left);
  try { e.currentTarget.setPointerCapture(e.pointerId); } catch { /* jsdom */ }
  dragRef.current = { startX: x, endX: x };
  setDragDisplay({ startX: x, endX: x });
}

function handlePointerMove(e: React.PointerEvent<SVGSVGElement>) {
  if (!dragRef.current) return;
  const rect = e.currentTarget.getBoundingClientRect();
  const x = Math.round(e.clientX - rect.left);
  dragRef.current = { ...dragRef.current, endX: x };
  setDragDisplay({ ...dragRef.current });
}

function handlePointerUp(e: React.PointerEvent<SVGSVGElement>) {
  const drag = dragRef.current;
  if (drag && onRangeSelect) {
    const rect = e.currentTarget.getBoundingClientRect();
    const w = rect.width || width;
    const fromMs = pixelToMs(Math.min(drag.startX, drag.endX), rangeStartMs, rangeEndMs, w);
    const toMs   = pixelToMs(Math.max(drag.startX, drag.endX), rangeStartMs, rangeEndMs, w);
    if (toMs > fromMs) onRangeSelect(fromMs, toMs);
  }
  dragRef.current = null;
  setDragDisplay(null);
}
```

Update the `<svg>` element to wire the handlers and add a drag selection rect. Replace the `<svg ...>` opening tag and add the selection rect inside it:

```tsx
<svg
  width="100%"
  height={height}
  viewBox={`0 0 ${width} ${height}`}
  aria-hidden="true"
  onPointerMove={(e) => {
    if (dragRef.current) {
      handlePointerMove(e);
    } else {
      const rect = e.currentTarget.getBoundingClientRect();
      setHoverX(Math.round(e.clientX - rect.left));
    }
  }}
  onPointerLeave={() => { setHoverX(null); }}
  onPointerDown={handlePointerDown}
  onPointerUp={handlePointerUp}
  onPointerCancel={() => { dragRef.current = null; setDragDisplay(null); }}
  style={{ cursor: onRangeSelect ? "crosshair" : "default", overflow: "visible" }}
>
```

Add the drag selection rect just before the closing `</svg>` (after the time labels):
```tsx
{dragDisplay != null && (() => {
  const x1 = Math.min(dragDisplay.startX, dragDisplay.endX);
  const x2 = Math.max(dragDisplay.startX, dragDisplay.endX);
  return (
    <rect
      x={x1}
      y={PLOT_TOP}
      width={x2 - x1}
      height={plotBottom - PLOT_TOP}
      fill="var(--brand)"
      opacity={0.15}
    />
  );
})()}
```

- [ ] **Step 4: Run the tests to confirm they pass**

```
cd apps/frontend && npx vitest run src/components/ui/time-series-graph.test.tsx
```
Expected: PASS — all existing tests plus new brush and pixelToMs tests green.

- [ ] **Step 5: Run typecheck**

```
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/components/ui/time-series-graph.tsx apps/frontend/src/components/ui/time-series-graph.test.tsx
git commit -m "feat: add brush range selection to TimeSeriesGraph"
```

---

## Task 5: Remove date controls from SignalExplorer

**Files:**
- Modify: `apps/frontend/src/components/shared/SignalExplorer.tsx`
- Modify: `apps/frontend/src/components/shared/SignalExplorer.test.tsx`

- [ ] **Step 1: Remove date-related props from `SignalExplorerProps`**

In `apps/frontend/src/components/shared/SignalExplorer.tsx`, replace the entire file with:

```tsx
import { type ReactNode, useState } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";

export type SaveStatus = "idle" | "saving" | "saved" | "error";

export interface SignalExplorerProps {
  title: string;
  service: string;
  onServiceChange: (service: string) => void;
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
        <div className="flex flex-1 items-start gap-3">
          {renderTable(selectedId, handleSelect)}
        </div>
        {selectedId !== null && (
          <div className="w-1/4 shrink-0">
            {renderPanel(selectedId, () => setSelectedId(null))}
          </div>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Update `SignalExplorer.test.tsx` — remove date fixtures**

In `apps/frontend/src/components/shared/SignalExplorer.test.tsx`, replace `makeProps`:

```ts
function makeProps(overrides: Partial<SignalExplorerProps> = {}): SignalExplorerProps {
  return {
    title: "Logs",
    service: "",
    onServiceChange: vi.fn(),
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
```

Also remove the import of `Select` and `SelectOption` if they were in the test file (they weren't — the test doesn't import them directly).

- [ ] **Step 3: Run SignalExplorer tests**

```
cd apps/frontend && npx vitest run src/components/shared/SignalExplorer.test.tsx
```
Expected: PASS — all 8 tests green.

- [ ] **Step 4: Run typecheck**

```
cd apps/frontend && npx tsc --noEmit
```
Expected: errors in `LogSearch.tsx` and `TraceSearch.tsx` about removed props. That's expected — they'll be fixed in Tasks 6 and 7.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/components/shared/SignalExplorer.tsx apps/frontend/src/components/shared/SignalExplorer.test.tsx
git commit -m "feat: remove per-page date controls from SignalExplorer"
```

---

## Task 6: Migrate LogExplorer to useGlobalDateRange

**Files:**
- Modify: `apps/frontend/src/pages/LogSearch.tsx`

- [ ] **Step 1: Update LogExplorer to consume useGlobalDateRange**

In `apps/frontend/src/pages/LogSearch.tsx`:

Replace the `useSignalSearch` import with `useGlobalDateRange`:
```ts
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";
```

Remove the `useSignalSearch` import line entirely.

Remove `initialLookbackMinutes` from `LogExplorerProps`:
```ts
export type LogExplorerProps = {
  initialService?: string;
  lockedService?: boolean;
  showHeader?: boolean;
  showServiceColumn?: boolean;
  showPromote?: boolean;
  tableAriaLabel?: string;
};
```

Replace the `useSignalSearch` call block inside `LogExplorer`:
```ts
const { format } = useTimeDisplay();
const { fromMs, toMs, setCustomRange } = useGlobalDateRange();
const [service, setService] = useState(initialService);
const [bucketCount, setBucketCount] = useState(60);
const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");

const from = new Date(fromMs).toISOString();
const to   = new Date(toMs).toISOString();
```

Update query keys and calls to use `fromMs`/`toMs`/`from`/`to`:
```ts
const { data, isLoading, error } = useQuery({
  queryKey: ["logs", service, fromMs, toMs],
  queryFn: () => searchLogs({ service: service || undefined, from, to, limit: 50 }),
});

const { data: histogramData, isError: isHistogramError } = useQuery({
  queryKey: ["logs-histogram", service, fromMs, toMs, bucketCount],
  queryFn: () =>
    fetchLogHistogram({
      service: service || undefined,
      from,
      to,
      buckets: bucketCount,
    }),
  placeholderData: (prev: LogHistogramResponse | undefined) => prev,
});

const histogram = useMemo(
  () =>
    histogramData?.buckets
      ? histogramFromApi(histogramData.buckets)
      : buildLogHistogram([], fromMs, toMs),
  [histogramData, fromMs, toMs],
);
```

Update `handlePromote` to drop `lookback_minutes`:
```ts
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
          lookback_minutes: Math.round((toMs - fromMs) / 60_000),
          filters: { facets: ["service_name", "severity_number", "environment", "host_id"] },
        },
      ],
    });
    setSaveStatus("saved");
  } catch {
    setSaveStatus("error");
  }
};
```

Update the `<SignalExplorer>` call — remove date-related props:
```tsx
return (
  <SignalExplorer
    title="Logs"
    service={service}
    onServiceChange={(s) => { setService(s); }}
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
          onRangeSelect={setCustomRange}
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
```

Also remove the `formatBucketLabel` usage for `customRangeLabel` since that prop no longer exists on `SignalExplorer`.

- [ ] **Step 2: Run typecheck**

```
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors in `LogSearch.tsx`.

- [ ] **Step 3: Run LogSearch tests**

```
cd apps/frontend && npx vitest run src/pages/LogSearch.test.tsx
```

If tests fail because they relied on `initialLookbackMinutes` being passed to `LogExplorer`, remove those props from the test call sites.

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/pages/LogSearch.tsx
git commit -m "feat: migrate LogExplorer to useGlobalDateRange"
```

---

## Task 7: Migrate TraceExplorer to useGlobalDateRange

**Files:**
- Modify: `apps/frontend/src/pages/TraceSearch.tsx`

- [ ] **Step 1: Update TraceExplorer to consume useGlobalDateRange**

In `apps/frontend/src/pages/TraceSearch.tsx`:

Replace the `useSignalSearch` import with `useGlobalDateRange`:
```ts
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";
```

Remove `initialLookbackMinutes` from `TraceExplorerProps`:
```ts
export type TraceExplorerProps = {
  initialService?: string;
  lockedService?: boolean;
  showHeader?: boolean;
  showServiceColumn?: boolean;
  showPromote?: boolean;
  showFacets?: boolean;
  tableAriaLabel?: string;
  tableMode?: "select" | "link";
};
```

Replace the `useSignalSearch` destructure inside `TraceExplorer`:
```ts
const { format } = useTimeDisplay();
const { fromMs, toMs, setCustomRange } = useGlobalDateRange();
const [service, setService] = useState(initialService);
const [bucketCount, setBucketCount] = useState(60);
const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");

const from = new Date(fromMs).toISOString();
const to   = new Date(toMs).toISOString();
```

Update queries and histogram memo to use `fromMs`/`toMs`/`from`/`to` (same pattern as LogExplorer in Task 6 — use `fromMs`/`toMs` in query keys, `from`/`to` ISO strings in API calls).

Update `handlePromote` to use `Math.round((toMs - fromMs) / 60_000)` for `lookback_minutes` (same as LogExplorer).

Update `<SignalExplorer>` call — remove `lookbackMinutes`, `onLookbackChange`, `customRangeMs`, `customRangeLabel`, `onClearRange` props. Pass `onRangeSelect={setCustomRange}` to `<Histogram>`.

- [ ] **Step 2: Run typecheck**

```
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors in `TraceSearch.tsx`.

- [ ] **Step 3: Run TraceSearch tests**

```
cd apps/frontend && npx vitest run src/pages/TraceSearch.test.tsx
```

If tests pass `initialLookbackMinutes` to `TraceExplorer`, remove those. Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/pages/TraceSearch.tsx
git commit -m "feat: migrate TraceExplorer to useGlobalDateRange"
```

---

## Task 8: Migrate ServiceDetailPage to useGlobalDateRange

**Files:**
- Modify: `apps/frontend/src/pages/ServiceDetailPage.tsx`

- [ ] **Step 1: Replace lookbackMinutes with useGlobalDateRange**

In `apps/frontend/src/pages/ServiceDetailPage.tsx`:

Add import:
```ts
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";
```

Remove these imports that are no longer needed:
```ts
import { useLocation, useParams, useSearch } from "@tanstack/react-router";
// Change to:
import { useLocation, useParams } from "@tanstack/react-router";
```

In `ServiceDetailPage`, remove `useSearch` and `readLookbackMinutes`. Replace with `useGlobalDateRange`:
```ts
export default function ServiceDetailPage() {
  const { serviceId } = useParams({ strict: false });
  if (!serviceId) {
    return <LoadingState>Loading service overview…</LoadingState>;
  }
  const serviceName = decodeURIComponent(serviceId);
  const location = useLocation();
  const activeTab = signalTabFromPath(location.pathname);
  const { fromMs, toMs } = useGlobalDateRange();

  const { data, isLoading, isError } = useQuery({
    queryKey: ["service-summary", serviceName, fromMs, toMs],
    queryFn: () => getServiceSummary(serviceName, {
      lookback_minutes: Math.round((toMs - fromMs) / 60_000),
    }),
  });

  if (isLoading) {
    return <LoadingState>Loading service overview…</LoadingState>;
  }

  if (isError || !data) {
    return (
      <section className="page-stack">
        <Link to="/services" className="secondary-link">Back to services</Link>
        <EmptyState title="Service not found" metadata={[serviceName]} />
      </section>
    );
  }

  return (
    <ServiceDetailView
      service={data.service}
      activeTab={activeTab}
      fromMs={fromMs}
      toMs={toMs}
    />
  );
}
```

Update `ServiceDetailView` props and usages — replace `lookbackMinutes: number` with `fromMs: number; toMs: number` throughout:

```ts
function ServiceDetailView({
  service,
  activeTab,
  fromMs,
  toMs,
}: {
  service: ServiceSummary;
  activeTab: ServiceSignalTab;
  fromMs: number;
  toMs: number;
}) {
```

In `ServiceDetailView`, replace the `<ResponseTimeGraphSection>` call:
```tsx
<ResponseTimeGraphSection
  serviceName={service.service_name}
  fromMs={fromMs}
  toMs={toMs}
/>
```

In `ServiceSignalTabs`, replace `lookbackMinutes` with `fromMs`/`toMs`:
```ts
function ServiceSignalTabs({
  serviceName,
  activeTab,
  fromMs,
  toMs,
}: {
  serviceName: string;
  activeTab: ServiceSignalTab;
  fromMs: number;
  toMs: number;
}) {
```

Remove `preservedSearch` — tab links no longer need to carry a date param:
```tsx
<Link
  key={link.tab}
  to={link.to}
  params={{ serviceId: encodedService }}
  className={activeTab === link.tab ? "modern-signal-tab active" : "modern-signal-tab"}
  aria-current={activeTab === link.tab ? "page" : undefined}
>
  {link.label}
</Link>
```

`ServiceLogsTab` and `ServiceTracesTab` no longer take `lookbackMinutes` — remove that prop entirely. Since `LogExplorer` and `TraceExplorer` now read date from the global hook internally, no prop needs to be passed:

```tsx
function ServiceLogsTab({ serviceName }: { serviceName: string }) {
  return (
    <LogExplorer
      initialService={serviceName}
      lockedService
      showHeader={false}
      showServiceColumn={false}
      showPromote={false}
      tableAriaLabel="Service logs"
    />
  );
}

function ServiceTracesTab({ serviceName }: { serviceName: string }) {
  return (
    <TraceExplorer
      initialService={serviceName}
      lockedService
      showHeader={false}
      showServiceColumn={false}
      showPromote={false}
      showFacets={false}
      tableAriaLabel="Service traces"
    />
  );
}
```

Update `ResponseTimeGraphSection` to use `fromMs`/`toMs`:
```ts
function ResponseTimeGraphSection({
  serviceName,
  fromMs,
  toMs,
}: {
  serviceName: string;
  fromMs: number;
  toMs: number;
}) {
  const { setCustomRange } = useGlobalDateRange();
  const lookbackMinutes = Math.round((toMs - fromMs) / 60_000);

  const { data: historyData } = useQuery({
    queryKey: ["service-response-time", serviceName, fromMs, toMs],
    queryFn: () =>
      getServiceResponseTimeHistory(serviceName, {
        lookback_minutes: lookbackMinutes,
        buckets: 60,
      }),
  });

  const { data: deploymentData } = useQuery({
    queryKey: ["deployments", serviceName, fromMs, toMs],
    queryFn: () =>
      listDeployments({
        service_name: serviceName,
        start_time: new Date(fromMs).toISOString(),
        end_time: new Date(toMs).toISOString(),
        limit: 20,
      }),
  });

  if (!historyData?.buckets?.length) return null;

  // ... (p95Series, p50Series, rateSeries definitions unchanged)

  return (
    <TimeSeriesGraph
      series={[p95Series, p50Series, rateSeries]}
      deploymentMarkers={deploymentData?.items ?? []}
      rangeStartMs={fromMs}
      rangeEndMs={toMs}
      eyebrow="Performance"
      title="Response Time & Throughput"
      ariaLabel="Service response time and throughput graph"
      onRangeSelect={setCustomRange}
    />
  );
}
```

Remove the `ServiceDetailSearch` type and `readLookbackMinutes` function — they are no longer used.

Also update the "Lookback" display in the health panel to show a computed string:
```tsx
<div>
  <dt>Time window</dt>
  <dd>{describeRange(fromMs, toMs)}</dd>
</div>
```

And add the helper:
```ts
function describeRange(fromMs: number, toMs: number): string {
  const minutes = Math.round((toMs - fromMs) / 60_000);
  if (minutes < 60) return `Last ${minutes}m`;
  const hours = Math.round(minutes / 60);
  return `Last ${hours}h`;
}
```

- [ ] **Step 2: Run typecheck**

```
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors.

- [ ] **Step 3: Run all tests**

```
cd apps/frontend && npx vitest run
```
Expected: PASS — no regressions.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/pages/ServiceDetailPage.tsx
git commit -m "feat: migrate ServiceDetailPage to useGlobalDateRange"
```

---

## Task 9: Trim useSignalSearch to service-only

**Files:**
- Modify: `apps/frontend/src/hooks/useSignalSearch.ts`

- [ ] **Step 1: Replace the hook body with service-only state**

Replace the entire contents of `apps/frontend/src/hooks/useSignalSearch.ts` with:

```ts
import { useState } from "react";

export interface UseSignalSearchOptions {
  initialService?: string;
}

export interface UseSignalSearchResult {
  service: string;
  setService: (service: string) => void;
}

export function useSignalSearch({
  initialService = "",
}: UseSignalSearchOptions = {}): UseSignalSearchResult {
  const [service, setService] = useState(initialService);
  return { service, setService };
}
```

- [ ] **Step 2: Run typecheck**

```
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors (LogSearch and TraceSearch no longer import the date fields).

- [ ] **Step 3: Run all tests**

```
cd apps/frontend && npx vitest run
```
Expected: PASS — all tests green.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/hooks/useSignalSearch.ts
git commit -m "refactor: trim useSignalSearch to service filter only"
```

---

## Task 10: Final verification

- [ ] **Step 1: Run full test suite**

```
cd apps/frontend && npx vitest run
```
Expected: PASS.

- [ ] **Step 2: Run typecheck**

```
cd apps/frontend && npx tsc --noEmit
```
Expected: no errors.

- [ ] **Step 3: Start dev server and verify manually**

```
cd apps/frontend && npm run dev
```

Check:
1. AppShell header shows a "Last 1 hour" dropdown
2. Changing the dropdown updates all pages (log queries re-fire)
3. Dragging on a histogram selects a range — dropdown replaced by "Reset range"
4. Reset range button returns to preset dropdown
5. Dragging on a TimeSeriesGraph in ServiceDetailPage selects a range
6. URL reflects `?preset=1h` for presets and `?from=...&to=...` for custom range
7. Copying and pasting the URL in a new tab preserves the selected range

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat: global date range selector complete"
```
