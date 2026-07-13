# Column Key Toggle and Reorder Design

## Problem

PR #538 restored the ability to add/remove a results-table column from the trace and log context panels, but two rough edges remain:

1. The `+`/`-` toggle button renders inside the value cell (`<dd>`), next to the field's value, rather than next to the field's key (`<dt>`). Visually it reads as an action on the value, not the column.
2. Column selection has no ordering control and no persistence. Every reload (or navigating away and back) resets to the default column set in default order; there is no way to reorder columns at all.

## Scope

- Move the column toggle affordance in `DlRow` from the value cell to the label cell.
- Add drag-and-drop column reordering to `ColumnPickerControl`.
- Persist column order and visibility per browser (localStorage), separately for the log explorer and the trace explorer, following the existing `timeDisplay.tsx` localStorage pattern.
- Apply both changes to the log explorer and the trace explorer, since they share `DlRow` and `ColumnPickerControl`.

Out of scope: server-side/per-user persistence (tracked as a roadmap follow-up — see below), changes to Saved Views (which continue to store an explicit, named `visible_columns` snapshot independent of this browser-local default), and any change to which fields are resolvable as columns (that was PR #538's scope).

## Design

### `DlRow` button placement

Move the existing `<button onToggleColumn>` out of the `<dd>` element into the `<dt>` element, next to the label text. Behavior (title, aria-label, icon swap, `stopPropagation`, hover-to-reveal styling) is unchanged — only its container moves. No prop or callback signature changes.

### Column state model

Today each explorer holds one array, `visibleColumns: string[]`, that serves double duty as both "which columns are shown" and "in what order." That conflation is why there's no way to reorder independently of the checkbox list. Replace it with two arrays per explorer:

- `columnOrder: string[]` — every column key known this session (seeded from the signal's default columns, plus any custom field key added via a `DlRow` `+` click), in display order.
- `hiddenColumns: string[]` — the subset of `columnOrder` currently hidden.

Derive `visibleColumns = columnOrder.filter(k => !hiddenColumns.includes(k))` wherever the current code reads `visibleColumns` (results table columns, `DlRow`'s `columnVisible` check, `ColumnPickerControl`'s checked state). Table column order therefore always matches `columnOrder`.

`toggleColumn(key)` behavior:
- If `key` is not yet in `columnOrder`, append it and leave it visible (this is the `DlRow` "+" path for a not-yet-known field).
- Otherwise, flip its membership in `hiddenColumns` (this covers both `DlRow` toggle-off and the picker checkbox).

Reordering only mutates `columnOrder`; it never touches `hiddenColumns`.

### Persistence

Two localStorage keys, `observable.log-columns` and `observable.trace-columns`, each storing `JSON.stringify({ columnOrder, hiddenColumns })`. On mount, each explorer reads its key and validates the parsed value is `{ columnOrder: string[], hiddenColumns: string[] }`; on any parse failure, missing key, or shape mismatch, it falls back to today's defaults. Every change to `columnOrder` or `hiddenColumns` writes back to the same key (mirroring `setFormat` in `timeDisplay.tsx`, which writes on every change rather than batching).

This is intentionally per-browser, not per-user-account: it will not follow a user across devices or browsers. That gap is being added to the roadmap now, to migrate this to server-side per-user storage (likely alongside or reusing the Saved Views persistence path) once prioritized.

### Drag-and-drop reordering (`ColumnPickerControl`)

Add a small drag handle (`GripVertical` from `lucide-react`) to each row in the dropdown, to the left of the checkbox. Implement reordering with native HTML5 DnD (`draggable`, `onDragStart`, `onDragOver` with `preventDefault`, `onDrop`) rather than adding a drag-and-drop dependency — the interaction is a single flat list reorder, which native DnD handles without extra library weight. Dropping a row moves it to the dropped-on row's position in `columnOrder` and calls a new `onReorder(newOrder: string[])` prop; the existing `onChange` (checkbox toggle) prop is unchanged. Checked and unchecked rows share one draggable list, so hiding a column doesn't lose its position.

### Rollout to both explorers

`LogSearch.tsx` and `TraceSearch.tsx` each get the `columnOrder`/`hiddenColumns` state and localStorage effect described above, replacing their current single `visibleColumns` state. `LogResultsTable`, `TraceResultsTable`, and the two context-panel components (`LogContextSidebar`, `TraceContextSidebar`) keep taking a `visibleColumns: string[]` prop — only its source in the parent changes, so no changes are needed in those table/panel components beyond passing the derived array.

## Testing

Follow test-driven development.

- `dl-row.test.tsx`: update the existing toggle tests to assert the button is now a child of the `<dt>` (e.g. `screen.getByText("log.error.type").closest("dt")` contains the button) rather than asserting nothing about position (today's tests don't check position, so this adds coverage rather than just moving assertions).
- `ColumnPickerControl` (new test file): dragging a row to a new index calls `onReorder` with the expected key order; checkbox clicks still call `onChange` as before; unchanged columns keep their relative order after one drag.
- `LogSearch.test.tsx` / `TraceSearch.test.tsx`: column order/visibility survives a remount when localStorage has a prior value; an invalid/corrupt localStorage value falls back to defaults without throwing.
- Existing `LogResultsTable.test.tsx` / `TraceResultsTable.test.tsx` continue to assert on `visibleColumns` prop behavior unchanged.
- Full `bash scripts/local-ci.sh` gate runs before the code is pushed.

## Repository Impact

- No backend, Testcontainers, generated-model, dependency, ADR, or API changes.
- No new runtime dependency (native HTML5 DnD, no drag-and-drop library added).
- `docs/agent-context.md` does not need an update.
- Add a roadmap entry (deferred tier) for migrating log/trace column preferences from localStorage to per-user server-side persistence.
