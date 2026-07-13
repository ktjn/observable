# Column Key Toggle and Reorder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the log/trace context panel's column `+`/`-` toggle next to the field key instead of its value, and make table column order and visibility persist per browser and be reorderable via drag-and-drop.

**Architecture:** A new `useColumnPreferences` hook owns a single `{ columnOrder, hiddenColumns }` state per signal type, persisted to localStorage, and derives `visibleColumns`. `LogSearch.tsx` and `TraceSearch.tsx` each swap their existing `useState<string[]>` column state for this hook. `ColumnPickerControl` gains a drag handle per row (native HTML5 DnD, no new dependency) and calls a new `onReorder` prop; its old single `onChange` prop is split into `onToggle` (visibility) and `onReorder` (order). `DlRow` moves its existing toggle button from the `<dd>` (value) to the `<dt>` (label) cell — no prop or behavior changes there.

**Tech Stack:** React 19, TypeScript, Vitest + Testing Library, existing `lucide-react` icons, no new dependencies.

## Global Constraints

- No new npm dependencies (native HTML5 drag-and-drop, not a DnD library) — spec explicitly rules this out.
- Persistence is localStorage only for this plan; server-side per-user persistence is out of scope and tracked as a roadmap follow-up (Task 6).
- Follow the existing localStorage pattern in `apps/frontend/src/lib/timeDisplay.tsx` (write on every change, validate on read, fall back to defaults on any parse/shape failure).
- Design doc: `docs/superpowers/specs/2026-07-13-column-key-toggle-and-reorder-design.md`.

---

### Task 1: Move the `+`/`-` toggle button into the `<dt>` label cell

**Files:**
- Modify: `apps/frontend/src/components/ui/dl-row.tsx`
- Test: `apps/frontend/src/components/ui/dl-row.test.tsx`

**Interfaces:**
- Consumes: nothing new.
- Produces: `DlRow` keeps its exact existing props (`label`, `children`, `copyValue`, `onToggleColumn`, `columnVisible`) and behavior — only the button's DOM position changes. Later tasks do not depend on any new export from this file.

- [ ] **Step 1: Write the failing test**

Add these two assertions inside the existing add/remove tests in `apps/frontend/src/components/ui/dl-row.test.tsx` (insert right after each `getByRole("button", ...)` line, before the existing icon assertion):

```tsx
test("clicking the add-column button calls onToggleColumn once", () => {
  const onToggleColumn = vi.fn();
  render(
    <dl>
      <DlRow label="log.error.type" onToggleColumn={onToggleColumn} columnVisible={false}>
        TimeoutError
      </DlRow>
    </dl>
  );

  const button = screen.getByRole("button", { name: "Add log.error.type as a column" });
  expect(button.closest("dt")).not.toBeNull();
  expect(button.querySelector(".lucide-plus")).toBeInTheDocument();
  fireEvent.click(button);
  expect(onToggleColumn).toHaveBeenCalledTimes(1);
});

test("clicking the remove-column button calls onToggleColumn once", () => {
  const onToggleColumn = vi.fn();
  render(
    <dl>
      <DlRow label="log.error.type" onToggleColumn={onToggleColumn} columnVisible={true}>
        TimeoutError
      </DlRow>
    </dl>
  );

  const button = screen.getByRole("button", { name: "Remove log.error.type column" });
  expect(button.closest("dt")).not.toBeNull();
  expect(button.querySelector(".lucide-minus")).toBeInTheDocument();
  fireEvent.click(button);
  expect(onToggleColumn).toHaveBeenCalledTimes(1);
});
```

(This replaces the last two `test(...)` blocks in the file — same names, with the added `closest("dt")` assertion.)

- [ ] **Step 2: Run test to verify it fails**

Run: `npm --prefix apps/frontend test -- dl-row.test.tsx`
Expected: FAIL — `expect(button.closest("dt")).not.toBeNull()` fails because the button is currently inside `<dd>`.

- [ ] **Step 3: Move the button into `<dt>`**

Replace the full contents of `apps/frontend/src/components/ui/dl-row.tsx` with:

```tsx
import type { ReactNode } from "react";
import { Minus, Plus } from "lucide-react";
import { CopyButton } from "./copy-button";

export interface DlRowProps {
  label: string;
  children: ReactNode;
  copyValue?: string;
  /** Adds an affordance to toggle this property as a table column. Omit when not applicable. */
  onToggleColumn?: () => void;
  /** Whether this property is currently visible as a table column. */
  columnVisible?: boolean;
}

export function DlRow({ label, children, copyValue, onToggleColumn, columnVisible }: DlRowProps) {
  const toggleLabel = columnVisible ? `Remove ${label} column` : `Add ${label} as a column`;

  return (
    <div className="contents group">
      <dt className="flex items-start gap-1 break-all font-bold text-[var(--muted)]">
        {label}
        {onToggleColumn && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onToggleColumn();
              e.currentTarget.blur();
            }}
            title={toggleLabel}
            aria-label={toggleLabel}
            className="inline-flex shrink-0 items-center justify-center text-[var(--muted)] outline-none transition-opacity opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 focus-visible:ring-1 focus-visible:ring-[var(--focus-ring)] enabled:hover:text-[var(--brand)] disabled:opacity-40"
          >
            {columnVisible ? <Minus className="size-3" /> : <Plus className="size-3" />}
          </button>
        )}
      </dt>
      <dd className="m-0 flex min-w-0 items-start gap-1 break-all text-[var(--text)]">
        {children}
        {copyValue !== undefined && <CopyButton value={copyValue} size="xs" />}
      </dd>
    </div>
  );
}
```

Note: `group` moves from `<dd>` to the outer `contents` wrapper, so hovering either the label or the value still reveals the button (both `<dt>` and `<dd>` are descendants of the `group`-classed wrapper).

- [ ] **Step 4: Run test to verify it passes**

Run: `npm --prefix apps/frontend test -- dl-row.test.tsx`
Expected: PASS (all 5 tests in the file, including the two updated ones).

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/components/ui/dl-row.tsx apps/frontend/src/components/ui/dl-row.test.tsx
git commit -m "fix(ui): move column toggle button next to the field key"
```

---

### Task 2: Add drag-and-drop reordering to `ColumnPickerControl`

**Files:**
- Modify: `apps/frontend/src/features/signals/components/ColumnPickerControl.tsx`
- Test: `apps/frontend/src/features/signals/components/ColumnPickerControl.test.tsx`

**Interfaces:**
- Consumes: nothing new.
- Produces: `ColumnPickerControlProps<T>` changes from `{ columns, visibleColumns, onChange }` to `{ columns, visibleColumns, onToggle, onReorder }`, where `onToggle: (column: T) => void` fires on a single checkbox click and `onReorder: (order: T[]) => void` fires with the full reordered key array after a drag-drop. Task 4 and Task 5 wire these two callbacks to the hook from Task 3 (`toggleColumn` and `reorderColumns` respectively — same signatures).

- [ ] **Step 1: Write the failing test**

Replace the full contents of `apps/frontend/src/features/signals/components/ColumnPickerControl.test.tsx` with:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { ColumnPickerControl } from "./ColumnPickerControl";

const LOG_COLUMNS = [
  { key: "level", label: "Level" },
  { key: "service", label: "Service" },
];

function rowFor(labelText: string): HTMLElement {
  return screen.getByLabelText(labelText).closest("[draggable]") as HTMLElement;
}

test("toggling an unchecked column calls onToggle with its key", () => {
  const onToggle = vi.fn();
  render(
    <ColumnPickerControl columns={LOG_COLUMNS} visibleColumns={["service"]} onToggle={onToggle} onReorder={vi.fn()} />
  );

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.click(screen.getByLabelText("Level"));

  expect(onToggle).toHaveBeenCalledWith("level");
});

test("toggling a checked column calls onToggle with its key", () => {
  const onToggle = vi.fn();
  render(
    <ColumnPickerControl
      columns={LOG_COLUMNS}
      visibleColumns={["level", "service"]}
      onToggle={onToggle}
      onReorder={vi.fn()}
    />
  );

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.click(screen.getByLabelText("Service"));

  expect(onToggle).toHaveBeenCalledWith("service");
});

test("renders arbitrary promoted columns alongside fixed ones", () => {
  const onToggle = vi.fn();
  render(
    <ColumnPickerControl
      columns={[...LOG_COLUMNS, { key: "log.error.type", label: "log.error.type" }]}
      visibleColumns={["level", "service", "log.error.type"]}
      onToggle={onToggle}
      onReorder={vi.fn()}
    />
  );

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  expect(screen.getByLabelText("log.error.type")).toBeChecked();

  fireEvent.click(screen.getByLabelText("log.error.type"));
  expect(onToggle).toHaveBeenCalledWith("log.error.type");
});

test("dragging a row onto another calls onReorder with the new key order", () => {
  const onReorder = vi.fn();
  render(
    <ColumnPickerControl
      columns={LOG_COLUMNS}
      visibleColumns={["level", "service"]}
      onToggle={vi.fn()}
      onReorder={onReorder}
    />
  );

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.dragStart(rowFor("Level"));
  fireEvent.dragOver(rowFor("Service"));
  fireEvent.drop(rowFor("Service"));

  expect(onReorder).toHaveBeenCalledWith(["service", "level"]);
});

test("dropping a row onto itself does not call onReorder", () => {
  const onReorder = vi.fn();
  render(
    <ColumnPickerControl
      columns={LOG_COLUMNS}
      visibleColumns={["level", "service"]}
      onToggle={vi.fn()}
      onReorder={onReorder}
    />
  );

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.dragStart(rowFor("Level"));
  fireEvent.dragOver(rowFor("Level"));
  fireEvent.drop(rowFor("Level"));

  expect(onReorder).not.toHaveBeenCalled();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npm --prefix apps/frontend test -- ColumnPickerControl.test.tsx`
Expected: FAIL — `onToggle`/`onReorder` props don't exist yet on the component (TypeScript/runtime error, since current component only accepts `onChange`).

- [ ] **Step 3: Implement drag-and-drop reordering**

Replace the full contents of `apps/frontend/src/features/signals/components/ColumnPickerControl.tsx` with:

```tsx
import { useState } from "react";
import { GripVertical } from "lucide-react";
import { Button } from "../../../components/ui/button";

export interface ColumnDef<T extends string> {
  key: T;
  label: string;
}

export interface ColumnPickerControlProps<T extends string> {
  columns: ColumnDef<T>[];
  visibleColumns: T[];
  onToggle: (column: T) => void;
  onReorder: (order: T[]) => void;
}

export function ColumnPickerControl<T extends string>({
  columns,
  visibleColumns,
  onToggle,
  onReorder,
}: ColumnPickerControlProps<T>) {
  const [isOpen, setIsOpen] = useState(false);
  const [dragKey, setDragKey] = useState<T | null>(null);

  function handleDrop(targetKey: T) {
    if (dragKey === null || dragKey === targetKey) {
      setDragKey(null);
      return;
    }
    const order = columns.map((c) => c.key);
    const fromIndex = order.indexOf(dragKey);
    const toIndex = order.indexOf(targetKey);
    order.splice(fromIndex, 1);
    order.splice(toIndex, 0, dragKey);
    onReorder(order);
    setDragKey(null);
  }

  return (
    <div className="relative">
      <Button variant="secondary" onClick={() => setIsOpen((v) => !v)}>
        Columns
      </Button>
      {isOpen && (
        <div className="absolute z-10 mt-1 w-48 border border-[var(--border)] bg-[var(--surface)] p-2 shadow-lg">
          {columns.map(({ key, label }) => (
            <div
              key={key}
              draggable
              onDragStart={() => setDragKey(key)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={() => handleDrop(key)}
              className="flex items-center gap-1.5 py-1 text-sm"
            >
              <GripVertical
                className="size-3.5 shrink-0 cursor-grab text-[var(--muted)]"
                aria-hidden="true"
              />
              <label className="flex flex-1 items-center gap-2">
                <input type="checkbox" checked={visibleColumns.includes(key)} onChange={() => onToggle(key)} />
                {label}
              </label>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npm --prefix apps/frontend test -- ColumnPickerControl.test.tsx`
Expected: PASS (all 5 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/features/signals/components/ColumnPickerControl.tsx apps/frontend/src/features/signals/components/ColumnPickerControl.test.tsx
git commit -m "feat(ui): add drag-and-drop column reordering to ColumnPickerControl"
```

---

### Task 3: `useColumnPreferences` hook (localStorage-backed order + visibility)

**Files:**
- Create: `apps/frontend/src/hooks/useColumnPreferences.ts`
- Test: `apps/frontend/src/hooks/useColumnPreferences.test.ts`

**Interfaces:**
- Consumes: nothing new (plain `localStorage`, no other app modules).
- Produces: `useColumnPreferences(storageKey: string, defaultOrder: readonly string[]): { columnOrder: string[]; visibleColumns: string[]; toggleColumn: (key: string) => void; reorderColumns: (order: string[]) => void }`. Task 4 and Task 5 call this once per page with a fixed storage key (`"observable.log-columns"` / `"observable.trace-columns"`) and pass `toggleColumn`/`reorderColumns` straight through to `DlRow`'s `onToggleColumn` and `ColumnPickerControl`'s `onToggle`/`onReorder`.

- [ ] **Step 1: Write the failing test**

Create `apps/frontend/src/hooks/useColumnPreferences.test.ts`:

```ts
import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, test } from "vitest";
import { useColumnPreferences } from "./useColumnPreferences";

beforeEach(() => {
  window.localStorage.clear();
});

test("seeds from defaultOrder when nothing is stored", () => {
  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b", "c"]));

  expect(result.current.columnOrder).toEqual(["a", "b", "c"]);
  expect(result.current.visibleColumns).toEqual(["a", "b", "c"]);
});

test("toggling a known column hides it without changing order", () => {
  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b", "c"]));

  act(() => result.current.toggleColumn("b"));

  expect(result.current.columnOrder).toEqual(["a", "b", "c"]);
  expect(result.current.visibleColumns).toEqual(["a", "c"]);
});

test("toggling a hidden column shows it again", () => {
  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b", "c"]));

  act(() => result.current.toggleColumn("b"));
  act(() => result.current.toggleColumn("b"));

  expect(result.current.visibleColumns).toEqual(["a", "b", "c"]);
});

test("toggling an unknown column appends it and shows it", () => {
  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b"]));

  act(() => result.current.toggleColumn("custom.field"));

  expect(result.current.columnOrder).toEqual(["a", "b", "custom.field"]);
  expect(result.current.visibleColumns).toEqual(["a", "b", "custom.field"]);
});

test("reorderColumns replaces columnOrder and preserves hidden state", () => {
  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b", "c"]));

  act(() => result.current.toggleColumn("b"));
  act(() => result.current.reorderColumns(["c", "b", "a"]));

  expect(result.current.columnOrder).toEqual(["c", "b", "a"]);
  expect(result.current.visibleColumns).toEqual(["c", "a"]);
});

test("persists across remounts under the same storage key", () => {
  const first = renderHook(() => useColumnPreferences("test.columns", ["a", "b", "c"]));
  act(() => first.result.current.toggleColumn("a"));
  act(() => first.result.current.reorderColumns(["c", "b", "a"]));
  first.unmount();

  const second = renderHook(() => useColumnPreferences("test.columns", ["a", "b", "c"]));

  expect(second.result.current.columnOrder).toEqual(["c", "b", "a"]);
  expect(second.result.current.visibleColumns).toEqual(["c", "b"]);
});

test("falls back to defaultOrder when stored data is corrupt", () => {
  window.localStorage.setItem("test.columns", "not json");

  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b"]));

  expect(result.current.columnOrder).toEqual(["a", "b"]);
  expect(result.current.visibleColumns).toEqual(["a", "b"]);
});

test("falls back to defaultOrder when stored data has the wrong shape", () => {
  window.localStorage.setItem("test.columns", JSON.stringify({ columnOrder: "a,b", hiddenColumns: [] }));

  const { result } = renderHook(() => useColumnPreferences("test.columns", ["a", "b"]));

  expect(result.current.columnOrder).toEqual(["a", "b"]);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npm --prefix apps/frontend test -- useColumnPreferences.test.ts`
Expected: FAIL — `./useColumnPreferences` module does not exist.

- [ ] **Step 3: Implement the hook**

Create `apps/frontend/src/hooks/useColumnPreferences.ts`:

```ts
import { useCallback, useMemo, useState } from "react";

interface StoredColumnPreferences {
  columnOrder: string[];
  hiddenColumns: string[];
}

export interface ColumnPreferences {
  columnOrder: string[];
  visibleColumns: string[];
  toggleColumn: (key: string) => void;
  reorderColumns: (order: string[]) => void;
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function readStoredPreferences(storageKey: string): StoredColumnPreferences | null {
  try {
    const raw = window.localStorage.getItem(storageKey);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (
      typeof parsed !== "object" ||
      parsed === null ||
      !isStringArray((parsed as Record<string, unknown>).columnOrder) ||
      !isStringArray((parsed as Record<string, unknown>).hiddenColumns)
    ) {
      return null;
    }
    return parsed as StoredColumnPreferences;
  } catch {
    return null;
  }
}

/**
 * Persists column order and visibility to localStorage under `storageKey`,
 * seeded from `defaultOrder` the first time (or whenever stored data is missing or malformed).
 */
export function useColumnPreferences(storageKey: string, defaultOrder: readonly string[]): ColumnPreferences {
  const [state, setState] = useState<StoredColumnPreferences>(
    () => readStoredPreferences(storageKey) ?? { columnOrder: [...defaultOrder], hiddenColumns: [] },
  );

  const persist = useCallback(
    (next: StoredColumnPreferences) => {
      setState(next);
      window.localStorage.setItem(storageKey, JSON.stringify(next));
    },
    [storageKey],
  );

  const toggleColumn = useCallback(
    (key: string) => {
      persist(
        state.columnOrder.includes(key)
          ? {
              columnOrder: state.columnOrder,
              hiddenColumns: state.hiddenColumns.includes(key)
                ? state.hiddenColumns.filter((k) => k !== key)
                : [...state.hiddenColumns, key],
            }
          : { columnOrder: [...state.columnOrder, key], hiddenColumns: state.hiddenColumns },
      );
    },
    [persist, state],
  );

  const reorderColumns = useCallback(
    (order: string[]) => {
      persist({ columnOrder: order, hiddenColumns: state.hiddenColumns });
    },
    [persist, state.hiddenColumns],
  );

  const visibleColumns = useMemo(
    () => state.columnOrder.filter((key) => !state.hiddenColumns.includes(key)),
    [state],
  );

  return { columnOrder: state.columnOrder, visibleColumns, toggleColumn, reorderColumns };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npm --prefix apps/frontend test -- useColumnPreferences.test.ts`
Expected: PASS (all 8 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/hooks/useColumnPreferences.ts apps/frontend/src/hooks/useColumnPreferences.test.ts
git commit -m "feat: add useColumnPreferences hook for persisted column order/visibility"
```

---

### Task 4: Wire `useColumnPreferences` into the log explorer

**Files:**
- Modify: `apps/frontend/src/pages/LogSearch.tsx`
- Test: `apps/frontend/src/pages/LogSearch.test.tsx`

**Interfaces:**
- Consumes: `useColumnPreferences` from Task 3 (`columnOrder`, `visibleColumns`, `toggleColumn`, `reorderColumns`); `ColumnPickerControl`'s `onToggle`/`onReorder` props from Task 2.
- Produces: no new exports; `LogExplorer`'s rendered behavior for column order/visibility/persistence is what Task 6's manual smoke check exercises.

- [ ] **Step 1: Write the failing tests**

In `apps/frontend/src/pages/LogSearch.test.tsx`, add `window.localStorage.clear();` to the existing `beforeEach` (around line 196-200) so each test starts with clean persisted state:

```ts
beforeEach(() => {
  vi.clearAllMocks();
  window.history.pushState({}, "", "/logs");
  mockSetCustomRange.mockClear();
  window.localStorage.clear();
});
```

Then add these two new tests after the existing `"toggles log fields as table columns from the context panel"` test:

```tsx
test("persists column visibility across remounts", async () => {
  const { unmount } = renderLogSearch();
  fireEvent.click(await screen.findByRole("button", { name: "Open log context for checkout completed" }));
  const sidebar = screen.getByRole("complementary", { name: "Selected log context" });
  fireEvent.click(within(sidebar).getByRole("button", { name: "Remove service.name column" }));
  unmount();

  renderLogSearch();
  const table = await screen.findByRole("table", { name: "Log results" });
  expect(within(table).queryByRole("columnheader", { name: "service.name" })).not.toBeInTheDocument();
});

test("reordering columns via the picker changes the table header order", async () => {
  renderLogSearch();
  const table = await screen.findByRole("table", { name: "Log results" });

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  const severityRow = screen.getByLabelText("severity_number").closest("[draggable]") as HTMLElement;
  const timeRow = screen.getByLabelText("time").closest("[draggable]") as HTMLElement;
  fireEvent.dragStart(severityRow);
  fireEvent.dragOver(timeRow);
  fireEvent.drop(timeRow);

  const headers = within(table).getAllByRole("columnheader").map((h) => h.textContent);
  expect(headers[0]).toBe("severity_number");
  expect(headers[1]).toBe("time");
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npm --prefix apps/frontend test -- LogSearch.test.tsx`
Expected: FAIL — persistence and reordering aren't wired up yet (columns reset to defaults on remount; there's no drag handle in the picker's log column checkboxes since `LogSearch.tsx` still calls the old `onChange` prop, which no longer exists after Task 2).

- [ ] **Step 3: Wire the hook into `LogExplorer`**

In `apps/frontend/src/pages/LogSearch.tsx`, replace the import of `DEFAULT_LOG_COLUMNS`/`normalizeLogColumnKeys` (keep both, they're still used) and add the hook import:

```ts
import { DEFAULT_LOG_COLUMNS, logContextEntries, normalizeLogColumnKeys } from "../utils/logContext";
import { useColumnPreferences } from "../hooks/useColumnPreferences";
```

Replace this block (around line 127-129):

```ts
  const [visibleColumns, setVisibleColumns] = useState<string[]>(() =>
    showServiceColumn ? [...DEFAULT_LOG_COLUMNS] : DEFAULT_LOG_COLUMNS.filter((key) => key !== "service.name"),
  );
```

with:

```ts
  const { columnOrder, visibleColumns, toggleColumn, reorderColumns } = useColumnPreferences(
    "observable.log-columns",
    showServiceColumn ? DEFAULT_LOG_COLUMNS : DEFAULT_LOG_COLUMNS.filter((key) => key !== "service.name"),
  );
```

Remove the now-separate `toggleColumn` function definition further down (around line 290-293):

```ts
  const toggleColumn = (key: string) =>
    setVisibleColumns((current) =>
      current.includes(key) ? current.filter((column) => column !== key) : [...current, key],
    );
```

(delete this block entirely — `toggleColumn` now comes from the hook).

Update `handleLoadView` (around line 287), which currently does `setVisibleColumns(normalizeLogColumnKeys(config.visible_columns));` — replace that line with:

```ts
    reorderColumns(normalizeLogColumnKeys(config.visible_columns));
```

A Saved View has always stored a fully-visible column list (there was no "hidden" concept before this change), so loading one only needs to set `columnOrder`. Any pre-existing `hiddenColumns` entries are harmless: `visibleColumns` is derived as `columnOrder.filter(k => !hidden.includes(k))`, so a key only stays hidden if it's both still in `hiddenColumns` *and* present in the newly-loaded `columnOrder` — and any hidden key absent from the new order was already invisible either way.

Update the `ColumnPickerControl` usage (around line 314-323):

```tsx
          <ColumnPickerControl
            columns={[
              ...DEFAULT_LOG_COLUMNS.map((key) => ({ key, label: key })),
              ...visibleColumns
                .filter((key) => !DEFAULT_LOG_COLUMNS.includes(key as (typeof DEFAULT_LOG_COLUMNS)[number]))
                .map((key) => ({ key, label: key })),
            ]}
            visibleColumns={visibleColumns}
            onChange={setVisibleColumns}
          />
```

Replace with:

```tsx
          <ColumnPickerControl
            columns={columnOrder.map((key) => ({ key, label: key }))}
            visibleColumns={visibleColumns}
            onToggle={toggleColumn}
            onReorder={reorderColumns}
          />
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npm --prefix apps/frontend test -- LogSearch.test.tsx`
Expected: PASS (all tests in the file, including the two new ones).

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/pages/LogSearch.tsx apps/frontend/src/pages/LogSearch.test.tsx
git commit -m "feat(logs): persist and support reordering visible columns"
```

---

### Task 5: Wire `useColumnPreferences` into the trace explorer

**Files:**
- Modify: `apps/frontend/src/pages/TraceSearch.tsx`
- Test: `apps/frontend/src/pages/TraceSearch.test.tsx`

**Interfaces:**
- Consumes: `useColumnPreferences` from Task 3; `ColumnPickerControl`'s `onToggle`/`onReorder` props from Task 2. Same shapes as Task 4.
- Produces: no new exports.

- [ ] **Step 1: Write the failing tests**

In `apps/frontend/src/pages/TraceSearch.test.tsx`, add `window.localStorage.clear();` to the existing `beforeEach` (around line 110), matching Task 4's change.

Add these two new tests after the existing `"toggles trace fields as table columns from the context panel"` test. Note `TraceResultsTable` maps some column keys to display labels via `COLUMN_LABELS` (`apps/frontend/src/features/signals/components/TraceResultsTable.tsx:11-14`) — `start_time` renders as header text "Time" and `duration` renders as "Duration" — while `ColumnPickerControl`'s checkbox labels use the raw key (`pickerColumns` maps `key` to `{ key, label: key }`), so the picker checkbox is labeled "start_time"/"duration" even though the table header reads "Time"/"Duration":

```tsx
test("persists column visibility across remounts", async () => {
  const { unmount } = renderTraceSearch();
  await screen.findByRole("table", { name: "Trace results" });
  fireEvent.click(screen.getByText("GET /checkout"));
  const sidebar = screen.getByRole("complementary", { name: "Selected trace context" });
  fireEvent.click(within(sidebar).getByRole("button", { name: "Remove service.name column" }));
  unmount();

  renderTraceSearch();
  const table = await screen.findByRole("table", { name: "Trace results" });
  expect(within(table).queryByRole("columnheader", { name: "service.name" })).not.toBeInTheDocument();
});

test("reordering columns via the picker changes the table header order", async () => {
  renderTraceSearch();
  const table = await screen.findByRole("table", { name: "Trace results" });

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  const durationRow = screen.getByLabelText("duration").closest("[draggable]") as HTMLElement;
  const startTimeRow = screen.getByLabelText("start_time").closest("[draggable]") as HTMLElement;
  fireEvent.dragStart(durationRow);
  fireEvent.dragOver(startTimeRow);
  fireEvent.drop(startTimeRow);

  const headers = within(table).getAllByRole("columnheader").map((h) => h.textContent);
  expect(headers[0]).toBe("Duration");
  expect(headers[1]).toBe("Time");
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npm --prefix apps/frontend test -- TraceSearch.test.tsx`
Expected: FAIL, for the same reasons as Task 4 (no persistence, `ColumnPickerControl` prop mismatch).

- [ ] **Step 3: Wire the hook into `TraceExplorer`**

In `apps/frontend/src/pages/TraceSearch.tsx`, add the hook import and drop the two imports that become unused (`FIXED_TRACE_KEYS` and the `TraceTableColumn` type — verified unused elsewhere in this file once the changes below land):

```ts
import { DEFAULT_TRACE_COLUMNS, traceContextEntries } from "../utils/traceContext";
import { useColumnPreferences } from "../hooks/useColumnPreferences";
```

and change the `TraceResultsTable` import from:

```ts
import { TraceResultsTable, type TraceTableColumn } from "../features/signals/components/TraceResultsTable";
```

to:

```ts
import { TraceResultsTable } from "../features/signals/components/TraceResultsTable";
```

Replace this block (around line 152-166):

```ts
  const [visibleColumns, setVisibleColumns] = useState<TraceTableColumn[]>(() =>
    showServiceColumn
      ? [...DEFAULT_TRACE_COLUMNS]
      : DEFAULT_TRACE_COLUMNS.filter((key) => key !== "service.name"),
  );
  const toggleTraceColumn = (key: string) => {
    setVisibleColumns((current) =>
      current.includes(key) ? current.filter((column) => column !== key) : [...current, key],
    );
  };
  const pickerColumns = useMemo(() => {
    const keys: string[] = [...FIXED_TRACE_KEYS];
    for (const key of visibleColumns) if (!keys.includes(key)) keys.push(key);
    return keys.map((key) => ({ key, label: key }));
  }, [visibleColumns]);
```

with:

```ts
  const { columnOrder, visibleColumns, toggleColumn: toggleTraceColumn, reorderColumns } = useColumnPreferences(
    "observable.trace-columns",
    showServiceColumn ? DEFAULT_TRACE_COLUMNS : DEFAULT_TRACE_COLUMNS.filter((key) => key !== "service.name"),
  );
  const pickerColumns = useMemo(() => columnOrder.map((key) => ({ key, label: key })), [columnOrder]);
```

(`useMemo` is already imported at the top of the file, so no import change needed for it.)

Update the `ColumnPickerControl` usage (around line 271-277):

```tsx
        <ColumnPickerControl
          columns={pickerColumns}
          visibleColumns={visibleColumns}
          onChange={setVisibleColumns}
        />
```

Replace with:

```tsx
        <ColumnPickerControl
          columns={pickerColumns}
          visibleColumns={visibleColumns}
          onToggle={toggleTraceColumn}
          onReorder={reorderColumns}
        />
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npm --prefix apps/frontend test -- TraceSearch.test.tsx`
Expected: PASS (all tests in the file, including the two new ones).

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/pages/TraceSearch.tsx apps/frontend/src/pages/TraceSearch.test.tsx
git commit -m "feat(traces): persist and support reordering visible columns"
```

---

### Task 6: Full verification, roadmap entry, and final gate

**Files:**
- Modify: `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`

**Interfaces:**
- Consumes: nothing (documentation-only change plus a verification pass).
- Produces: nothing consumed by other tasks — this is the final task.

- [ ] **Step 1: Add the deferred-tier roadmap entry**

In `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`, under `## 7. Deferred — Stability, Compliance, and Enterprise Packaging`, add a new bullet after the existing `- [ ] Further load/chaos/tenant-escape/security-review cycles...` bullet (before the `### Service Layer Architecture` subheading):

```markdown
- [ ] **Server-side per-user log/trace column preferences** — migrate the localStorage-only column
  order/visibility persistence added in `docs/superpowers/specs/2026-07-13-column-key-toggle-and-reorder-design.md`
  to per-user server-side storage (likely alongside or reusing the Saved Views persistence path),
  so preferences follow a user across browsers/devices. Only urgent if users report losing their
  column setup when switching machines.
```

- [ ] **Step 2: Run the full frontend test suite**

Run: `npm --prefix apps/frontend test`
Expected: all tests pass, with no regressions in `dl-row.test.tsx`, `ColumnPickerControl.test.tsx`, `useColumnPreferences.test.ts`, `LogSearch.test.tsx`, `TraceSearch.test.tsx`, `LogResultsTable.test.tsx`, or `TraceResultsTable.test.tsx`.

- [ ] **Step 3: Run the project's full local CI gate**

Run: `bash scripts/local-ci.sh`
Expected: gate passes (this repo's standard pre-push check, per `docs/agent-context.md`).

- [ ] **Step 4: Manually verify in the browser**

Use the `run` skill (or start the frontend dev server directly) to confirm, on both the Logs and Traces pages:
1. Opening a row's context panel shows the `+`/`-` button immediately next to the field key, not the value.
2. Clicking `+`/`-` toggles a column and the table updates.
3. Opening the Columns picker, dragging a row to a new position, and closing/reopening the picker shows the new order, and the results table header order matches.
4. Reloading the page preserves both the column set and the order from before the reload.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md
git commit -m "docs: add deferred roadmap entry for server-side column preferences"
```
