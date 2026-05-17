# Dashboard Grid Redesign + Shorthand Error Surfacing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the custom pointer-event dashboard grid with `react-grid-layout`, add an explicit edit-mode toggle, and surface actual backend error messages (including shorthand fallback context) to the user.

**Architecture:** The backend change is two isolated line-edits in `map_mcp_error` and the shorthand fallback path in `llm_adapter.rs`. The frontend replaces the custom 12-column CSS grid and all three resize handle divs with `react-grid-layout`; layout saves are batched behind a Done/Cancel edit-mode toggle instead of firing on every pointer-up.

**Tech Stack:** React 18, react-grid-layout, Vitest + @testing-library/react, Rust/Axum (backend), Cargo test

---

## File Map

| File | Change |
|---|---|
| `services/query-api/src/llm_adapter.rs` | Fix 2 error messages: `map_mcp_error` catch-all + shorthand fallback inline |
| `apps/frontend/package.json` + `package-lock.json` | Add `react-grid-layout` dependency |
| `apps/frontend/src/pages/DashboardDetailPage.tsx` | Replace custom grid + add edit mode |
| `apps/frontend/src/pages/DashboardDetailPage.test.tsx` | Remove old resize tests, add edit-mode tests |

---

## Task 1: Fix backend error messages

**Files:**
- Modify: `services/query-api/src/llm_adapter.rs` (lines ~1516–1555 and ~1685–1691)

### Step 1.1 — Write a failing test for `map_mcp_error`

At the bottom of `services/query-api/src/llm_adapter.rs`, inside the existing `#[cfg(test)]` block, add:

```rust
#[test]
fn map_mcp_error_catch_all_includes_underlying_detail() {
    let e = crate::mcp_query::McpQueryError::UnknownMetric("missing_metric".to_string());
    let tenant_id = uuid::Uuid::nil();
    let (status, axum::Json(body)) = map_mcp_error(e, tenant_id);
    assert_eq!(status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
    let msg = body["error"].as_str().unwrap();
    assert!(
        msg.contains("missing_metric"),
        "error body should include underlying detail, got: {msg}"
    );
}
```

- [ ] Add the test above into the `#[cfg(test)]` module at the bottom of `llm_adapter.rs`

- [ ] Run it to confirm it fails:
```
cargo test -p query-api map_mcp_error_catch_all_includes_underlying_detail
```
Expected: FAIL — the current message is `"query execution failed"` which does not contain `"missing_metric"`.

### Step 1.2 — Fix `map_mcp_error` catch-all

In `map_mcp_error` (around line 1548), change:
```rust
        _ => {
            tracing::error!(error = %e, tenant_id = %tenant_id, "NLQ pipeline failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "query execution failed"})),
            )
        }
```
to:
```rust
        _ => {
            tracing::error!(error = %e, tenant_id = %tenant_id, "NLQ pipeline failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("query execution failed: {e}")})),
            )
        }
```

- [ ] Make the edit above

- [ ] Run the test to confirm it passes:
```
cargo test -p query-api map_mcp_error_catch_all_includes_underlying_detail
```
Expected: PASS

### Step 1.3 — Fix shorthand fallback inline error

In `handle_nlq_query`, around line 1685, the shorthand-fallback error path does **not** call `map_mcp_error` — it has its own inline response. Change:

```rust
                Err(e) => {
                    tracing::error!(error = %e, tenant_id = %ctx.tenant_id, "shorthand fallback MCP query failed");
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "query execution failed"})),
                    ))
                }
```
to:
```rust
                Err(e) => {
                    tracing::error!(error = %e, tenant_id = %ctx.tenant_id, "shorthand fallback MCP query failed");
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "error": format!("No AI model is configured — query was interpreted as shorthand syntax. {e}")
                        })),
                    ))
                }
```

- [ ] Make the edit above

- [ ] Run all query-api tests to confirm nothing broke:
```
cargo test -p query-api
```
Expected: all pass

### Step 1.4 — Commit

- [ ] Commit:
```bash
git add services/query-api/src/llm_adapter.rs
git commit -m "fix: surface actual error in map_mcp_error and shorthand fallback path"
```

---

## Task 2: Install react-grid-layout

**Files:**
- Modify: `apps/frontend/package.json`, `apps/frontend/package-lock.json`

- [ ] Install the package (run from repo root):
```bash
cd apps/frontend && npm install react-grid-layout
```
`react-grid-layout` ships its own TypeScript types; no separate `@types/` package needed.

- [ ] Verify the frontend builds cleanly:
```bash
npm run build
```
Expected: build succeeds with no errors.

- [ ] Commit:
```bash
git add apps/frontend/package.json apps/frontend/package-lock.json
git commit -m "chore: add react-grid-layout dependency"
```

---

## Task 3: Rewrite DashboardDetailPage with react-grid-layout and edit mode

**Files:**
- Modify: `apps/frontend/src/pages/DashboardDetailPage.tsx`

This task replaces the custom grid and all three resize handle `<div>` elements. Read the file carefully before editing — the changes touch multiple sections.

### Step 3.1 — Update imports and remove obsolete helpers

At the top of `DashboardDetailPage.tsx`:

1. Add after the existing React import line:
```tsx
import ReactGridLayout, { WidthProvider, type Layout } from 'react-grid-layout';
import 'react-grid-layout/css/styles.css';
import 'react-resizable/css/styles.css';

const GridLayout = WidthProvider(ReactGridLayout);
```

2. Remove the import of `type PointerEvent as ReactPointerEvent` from the React import (it was only needed by the resize handlers).

3. Delete these constants and functions entirely — they are replaced by RGL:
   - `RESIZE_COLUMN_PX`
   - `RESIZE_ROW_PX`
   - `resizeFromLeft`
   - `resizeFromBottom`
   - `resizeFromRight`
   - `nextRowAfterPanels` — keep this, still used by `addPanel`

- [ ] Apply the import additions and deletions above

### Step 3.2 — Add edit-mode state and handlers to `DashboardDetailPage`

Inside `DashboardDetailPage`, add two new state variables after the existing `useState` calls:
```tsx
const [editMode, setEditMode] = useState(false);
const [stagedLayout, setStagedLayout] = useState<Layout[] | null>(null);
```

Replace the `resizePanel` and `movePanel` functions with these three handlers:

```tsx
function enterEditMode() {
  setEditMode(true);
}

function saveLayout() {
  if (!data) { setEditMode(false); return; }
  const panels = data.panels.map((panel) => {
    const staged = stagedLayout?.find((l) => l.i === panel.panel_id);
    if (!staged) return panelToUpdate(panel);
    return { ...panelToUpdate(panel), layout: { x: staged.x, y: staged.y, w: staged.w, h: staged.h } };
  });
  updateMutation.mutate(
    { name: data.name, panels },
    { onSettled: () => setEditMode(false) },
  );
  setStagedLayout(null);
}

function cancelEdit() {
  setStagedLayout(null);
  setEditMode(false);
}
```

- [ ] Add the two state variables and replace `resizePanel`/`movePanel` with the three handlers above

### Step 3.3 — Update the page header

In the JSX, replace the single `<Button variant="primary" onClick={() => setAddPanelOpen(...)}` "Add panel" button section:

```tsx
<div className="flex items-center gap-2">
  {editMode ? (
    <>
      <Button variant="primary" onClick={saveLayout} disabled={updateMutation.isPending}>
        {updateMutation.isPending ? "Saving…" : "Done"}
      </Button>
      <Button variant="secondary" onClick={cancelEdit}>
        Cancel
      </Button>
    </>
  ) : (
    <>
      <Button variant="secondary" onClick={enterEditMode}>
        Edit layout
      </Button>
      <Button variant="primary" onClick={() => setAddPanelOpen((o) => !o)}>
        Add panel
      </Button>
    </>
  )}
</div>
```

- [ ] Replace the header button section with the snippet above

### Step 3.4 — Replace the CSS grid with ReactGridLayout

Replace the entire `<div className="grid grid-cols-12 gap-3" ...>` block (and everything inside it) with:

```tsx
{data.panels.length > 0 && (
  <GridLayout
    cols={12}
    rowHeight={100}
    isDraggable={editMode}
    isResizable={editMode}
    layout={
      stagedLayout ??
      data.panels.map((p) => ({
        i: p.panel_id,
        x: p.layout.x,
        y: p.layout.y,
        w: p.layout.w,
        h: p.layout.h,
        minW: 2,
        minH: 1,
      }))
    }
    onLayoutChange={(layout) => {
      if (editMode) setStagedLayout(layout);
    }}
    draggableHandle=".panel-drag-handle"
    margin={[12, 12]}
  >
    {data.panels.map((panel) =>
      editingPanelId === panel.panel_id ? (
        <div key={panel.panel_id} className="min-w-0">
          <EditPanelForm
            panel={panel}
            onSave={(changes) => editPanel(panel.panel_id, changes)}
            onCancel={() => setEditingPanelId(null)}
            isPending={updateMutation.isPending}
          />
        </div>
      ) : (
        <div key={panel.panel_id} className="min-w-0">
          <DashboardPanelView
            dashboardId={data.dashboard_id}
            panel={panel}
            globalRange={{ fromMs: globalDateRange.fromMs, toMs: globalDateRange.toMs }}
            editMode={editMode}
            onDelete={() => deletePanel(panel.panel_id)}
            onEdit={() => setEditingPanelId(panel.panel_id)}
          />
        </div>
      ),
    )}
  </GridLayout>
)}
```

- [ ] Replace the grid block with the snippet above

### Step 3.5 — Simplify `DashboardPanelView`

Replace the entire `DashboardPanelView` function and its props type with:

```tsx
function DashboardPanelView({
  dashboardId,
  panel,
  globalRange,
  editMode,
  onDelete,
  onEdit,
}: {
  dashboardId: string;
  panel: DashboardPanel;
  globalRange: { fromMs: number; toMs: number };
  editMode: boolean;
  onDelete: () => void;
  onEdit: () => void;
}) {
  return (
    <Panel
      title={panel.title}
      eyebrow={panel.panel_kind === "text" ? "Text" : (panel.query_kind ?? "Query")}
      className="h-full"
      actions={
        <>
          {editMode && (
            <div
              className="panel-drag-handle flex h-7 w-7 cursor-grab items-center justify-center touch-none text-[var(--muted)] hover:text-[var(--text)] active:cursor-grabbing select-none"
              title="Drag to move"
            >
              ⠿
            </div>
          )}
          <button
            type="button"
            aria-label={`Edit panel ${panel.title}`}
            className="flex h-7 items-center px-2 text-xs text-[var(--muted)] hover:text-[var(--text)] focus:outline-none"
            onClick={onEdit}
          >
            Edit
          </button>
          <button
            type="button"
            aria-label={`Delete panel ${panel.title}`}
            className="flex h-7 w-7 items-center justify-center text-[var(--muted)] hover:text-[var(--bad)] focus:outline-none"
            onClick={onDelete}
          >
            ×
          </button>
        </>
      }
    >
      <div className="h-full min-h-[100px]">
        {panel.panel_kind === "text" ? (
          <TextPanel panel={panel} />
        ) : (
          <QueryPanel dashboardId={dashboardId} panel={panel} globalRange={globalRange} />
        )}
      </div>
    </Panel>
  );
}
```

- [ ] Replace `DashboardPanelView` with the simplified version above

### Step 3.6 — Start dev server and verify

- [ ] Start the frontend dev server:
```bash
cd apps/frontend && npm run dev
```

- [ ] Open a dashboard in the browser. Verify:
  - Panels render at the correct positions
  - "Edit layout" and "Add panel" buttons visible in the header
  - Click "Edit layout" → drag handle (⠿) appears on each panel, "Done" and "Cancel" appear in header
  - Drag a panel — ghost placeholder appears, panel snaps to grid on drop
  - Resize a panel by dragging the bottom-right corner handle
  - Click "Done" — layout saves, drag handle disappears
  - Click "Edit layout" again, drag a panel, click "Cancel" — panel returns to original position

- [ ] Stop the dev server

### Step 3.7 — Commit

- [ ] Commit:
```bash
git add apps/frontend/src/pages/DashboardDetailPage.tsx
git commit -m "feat: replace custom grid with react-grid-layout, add edit-mode toggle"
```

---

## Task 4: Update the test suite

**Files:**
- Modify: `apps/frontend/src/pages/DashboardDetailPage.test.tsx`

### Step 4.1 — Remove obsolete resize tests

Delete the three tests that exercise the old custom pointer-event handles:
- `"dragging a panel left border persists a new layout width"`
- `"dragging a panel right border persists a new layout width"`
- `"dragging a panel bottom border persists a new layout height"`

These test deleted code and will error on missing aria-labels.

- [ ] Delete the three test blocks above

### Step 4.2 — Add react-grid-layout mock

At the top of the test file, alongside the existing `vi.mock(...)` calls, add:

```tsx
vi.mock('react-grid-layout', async () => {
  const React = await import('react');
  const MockRGL = ({
    children,
    onLayoutChange,
    isDraggable,
  }: {
    children: React.ReactNode;
    onLayoutChange: (layout: Array<{ i: string; x: number; y: number; w: number; h: number }>) => void;
    isDraggable: boolean;
  }) => (
    <div data-testid="rgl" data-draggable={String(isDraggable)}>
      <button
        type="button"
        onClick={() =>
          onLayoutChange([
            { i: 'query-1', x: 3, y: 0, w: 6, h: 4 },
            { i: 'text-1', x: 6, y: 0, w: 6, h: 2 },
          ])
        }
      >
        Simulate layout change
      </button>
      {children}
    </div>
  );
  MockRGL.displayName = 'MockRGL';
  const WidthProvider = (Component: React.ComponentType<any>) => Component;
  return { default: MockRGL, WidthProvider };
});
```

- [ ] Add the mock above to the test file

### Step 4.3 — Add edit-mode tests

Add the following three tests after the existing `"add panel cancel button closes the form"` test:

```tsx
test("Edit layout button enters edit mode showing Done and Cancel", async () => {
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);

  renderPage();
  await screen.findByRole("heading", { name: "Checkout Health" });

  expect(screen.queryByRole("button", { name: "Done" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Edit layout" }));

  expect(screen.getByRole("button", { name: "Done" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Edit layout" })).not.toBeInTheDocument();
});

test("Cancel exits edit mode without calling updateDashboard", async () => {
  const updateSpy = vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);

  renderPage();
  await screen.findByRole("heading", { name: "Checkout Health" });

  fireEvent.click(screen.getByRole("button", { name: "Edit layout" }));
  fireEvent.click(screen.getByRole("button", { name: "Simulate layout change" }));
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

  expect(screen.getByRole("button", { name: "Edit layout" })).toBeInTheDocument();
  expect(updateSpy).not.toHaveBeenCalled();
});

test("Done saves staged layout to API and exits edit mode", async () => {
  const updateSpy = vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);

  renderPage();
  await screen.findByRole("heading", { name: "Checkout Health" });

  fireEvent.click(screen.getByRole("button", { name: "Edit layout" }));
  fireEvent.click(screen.getByRole("button", { name: "Simulate layout change" }));
  fireEvent.click(screen.getByRole("button", { name: "Done" }));

  await waitFor(() =>
    expect(updateSpy).toHaveBeenCalledWith(
      "test-tenant",
      "dash-1",
      expect.objectContaining({
        panels: expect.arrayContaining([
          expect.objectContaining({
            panel_id: "query-1",
            layout: { x: 3, y: 0, w: 6, h: 4 },
          }),
        ]),
      }),
    ),
  );

  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Edit layout" })).toBeInTheDocument(),
  );
});
```

- [ ] Add the three tests above

### Step 4.4 — Run the full test suite

- [ ] Run vitest:
```bash
cd apps/frontend && npm test -- --run
```
Expected: all tests pass. If `"renders query panels"` or other existing tests fail, check that the `GridLayout` mock renders children correctly — the mock's `WidthProvider` passthrough and child rendering should be transparent.

### Step 4.5 — Commit

- [ ] Commit:
```bash
git add apps/frontend/src/pages/DashboardDetailPage.test.tsx
git commit -m "test: update dashboard tests for edit-mode grid, remove old resize handle tests"
```

---

## Self-Review Checklist

- [x] **Spec coverage:** Backend catch-all (Task 1 Step 1.2), shorthand context (Task 1 Step 1.3), RGL install (Task 2), edit mode toggle (Task 3 Step 3.2–3.3), RGL grid (Task 3 Step 3.4), simplified panel (Task 3 Step 3.5), test update (Task 4). All spec requirements mapped.
- [x] **No placeholders:** All code blocks are complete.
- [x] **Type consistency:** `Layout` from `react-grid-layout` used throughout; `stagedLayout: Layout[] | null` matches `onLayoutChange: (layout: Layout[]) => void`; mock returns same shape `{ i, x, y, w, h }` as the real RGL callback.
- [x] **`nextRowAfterPanels`** is still used by `addPanel` — not deleted in Task 3.
- [x] **CSS imports** for both `react-grid-layout` and `react-resizable` included in Task 3 Step 3.1.
