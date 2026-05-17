# Dashboard Grid Redesign + Shorthand Error Surfacing

**Date:** 2026-05-12  
**Status:** Approved

---

## Problem

1. The dashboard's custom drag-resize implementation is clunky: resize handles are 3px-wide invisible borders, drag previews are not grid-snapped, and every pointer-up fires an API save. The overall experience is far below the standard set by tools like New Relic or Grafana.

2. When no LLM is configured, the backend silently falls back to the shorthand query parser. If that execution fails, the client receives `"query execution failed"` — no indication that shorthand was used, no actionable error detail.

---

## Goals

- Replace the custom grid with `react-grid-layout` for rock-solid drag, snap-to-grid, and resize.
- Introduce an explicit **Edit layout** mode so the grid is static during normal use.
- Surface the actual backend error and shorthand context to the user.

---

## Dashboard Grid

### Edit mode toggle

A button in the dashboard page header switches between **view mode** and **edit mode**.

- **View mode** (default): grid is static. No handles visible. Panels render cleanly. The "Add panel" button remains available.
- **Edit mode**: panels show a drag handle in their header and a resize grip in the bottom-right corner. A ghost placeholder shows the drop target during drag. The header shows **Done** and **Cancel** buttons.

Saving happens only when the user clicks **Done**. **Cancel** discards layout changes made during the current edit session.

### react-grid-layout integration

Install `react-grid-layout` (and `@types/react-grid-layout`).

The existing layout model `{x, y, w, h}` maps directly to RGL's `Layout` items — no data migration needed. Use a 12-column grid with `rowHeight={100}` (up from the current 80px, giving panels more breathing room).

Replace the `<div className="grid grid-cols-12 ...">` in `DashboardDetailPage.tsx` with a `<ReactGridLayout>` component. The `isDraggable` and `isResizable` props are set to `true` only in edit mode.

Remove the left-edge resize handle (non-standard). RGL's built-in bottom-right corner resize handle covers the standard use case.

### Panel chrome

The `DashboardPanelView` component is simplified:

- The braille drag handle (⠿) and the three custom resize `<div>` elements are removed.
- In edit mode, RGL injects its own drag affordance; we add a visible drag handle icon in the panel header (only in edit mode) using the `draggableHandle` prop.
- The **Edit** and **Delete** buttons in the panel actions remain visible in both modes.

### State management

`DashboardDetailPage` gains a small piece of state:

```
editMode: boolean
stagedLayout: DashboardPanelLayout[] | null
```

In edit mode, layout changes from RGL's `onLayoutChange` callback are stored in `stagedLayout` (local state). On **Done**, `stagedLayout` is flushed to the API via `updateMutation`. On **Cancel**, `stagedLayout` is discarded.

---

## Shorthand Error Surfacing

### Backend (`services/query-api/src/llm_adapter.rs`)

**Shorthand fallback path** (lines 1685–1691): replace the inline generic error with a message that includes the actual error and explains the fallback context:

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

**`map_mcp_error` catch-all** (line 1548–1554): include `e` in the response body so callers always receive the root cause:

```rust
_ => {
    tracing::error!(error = %e, tenant_id = %tenant_id, "NLQ pipeline failed");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": format!("query execution failed: {e}")})),
    )
}
```

### Frontend

No frontend changes required for the error path. The richer message from the backend flows through unchanged:

- `QueryPanel` in `DashboardDetailPage` already renders `Panel query failed: {String(error)}`.
- `NlqPanel` already renders `state.message` in the error state.

Both surfaces will automatically display the improved message once the backend is updated.

---

## Out of scope

- Panel time-range overrides, add-panel form, and rename — unchanged.
- The `DashboardsPage` (list view) — unchanged.
- Any changes to the shorthand parser logic itself.
