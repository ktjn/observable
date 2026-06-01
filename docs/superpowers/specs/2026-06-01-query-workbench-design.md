# Query Workbench Design

**Date:** 2026-06-01  
**Status:** Draft for review  
**Scope:** Phase 2-3 UI expansion, per `spec/05-frontend.md`

## Goal

Build a Monaco-based multi-signal notebook for ad-hoc exploration that supports metrics, logs, and traces in one place, keeps notebook state in the URL when possible, and starts with a fixed three-pane layout that can be reordered in a later slice.

The first slice should feel like a real notebook, not another explorer shell: a user can draft one or more cells, switch each cell between NLQ and raw IR mode, run the cells, share the resulting URL, and reopen the same notebook state later without server persistence.

## Non-Goals

- No server-side notebook persistence in the first slice.
- No dashboard-style drag-and-drop panel editor yet.
- No cross-user notebook collaboration or comments.
- No new query engine.
- No separate backend API for notebook storage.

## Design Decisions

### 1. URL State Is the Source of Truth

The notebook should be encoded in the URL when possible.

Use the existing root search params for shared app context:
- `from`
- `to`
- `service`
- `preset`

Add one route-local `state` param for notebook content. The notebook state is serialised into a compact URL-safe blob so the page can restore:
- notebook title
- active block order
- per-block signal target
- per-block editor mode
- per-block draft content
- collapsed/expanded block state

This keeps the notebook shareable without server persistence and preserves the existing global time picker model.

### 2. Fixed Starter Layout First

The first notebook opens with a fixed starter layout:
- one metrics block
- one logs block
- one traces block

Each block is independently editable, but the layout is not yet user-reorderable. This matches the hybrid approach you chose:
- fixed starter layout now
- reordering later

The page should still be structured as a notebook, not as three unrelated explorers. The shared chrome includes:
- notebook title
- URL/share controls
- global time range
- optional service context

### 3. Hybrid Query Editing Per Block

Each block has two editor modes:
- NLQ mode by default
- raw IR mode behind an advanced toggle

NLQ mode is the primary path. Raw IR is the deterministic fallback for power users or environments without an LLM.

The editor should use Monaco so the raw IR mode can display structured JSON with syntax highlighting and the NLQ mode can still feel like a real notebook input. The first implementation slice should add the editor dependency intentionally rather than hiding the need behind a textarea.

### 4. Reuse the Existing NLQ Pipeline

Do not invent a separate notebook query language.

Each notebook block should call the existing `submitNlqQuery()` flow with:
- tenant-scoped auth context from `useTenantContext()`
- shared global time range from `useGlobalDateRange()`
- the block’s signal target
- the block’s draft text or raw IR

When a block returns a `frame`, render it with the existing `VisualizationPanel` contract and preserve the provenance display pattern from `NlqPanel`.

### 5. Notebook Blocks Are Independent, But Synchronized By Shared Context

The notebook is multi-signal, not multi-query-engine.

Each block owns:
- its signal target
- its draft text
- its mode
- its execution state
- its result and provenance

The notebook shell owns:
- the shared time range
- optional service context
- the URL state serializer
- the starter layout
- the notebook title

This separation keeps the notebook understandable and makes later reordering/add/remove work local to the notebook shell.

## Recommended User Flow

1. Open `/workbench`.
2. The page restores the notebook from `state` if present; otherwise it opens the starter layout.
3. The user edits one or more blocks.
4. Each block can stay in NLQ mode or switch to raw IR mode.
5. Running a block updates only that block’s result.
6. Copying or sharing the URL preserves the notebook state and the current global time range.

## Architecture

### Route

Add a new top-level route for the workbench page. The route should:
- render a dedicated notebook page
- preserve the app shell and global time controls
- read/write the notebook state from the URL

Recommended path: `/workbench`.

### Component Boundaries

Expected implementation surface:

- `apps/frontend/src/pages/QueryWorkbenchPage.tsx`
- `apps/frontend/src/features/nlq/workbench/QueryWorkbench.tsx`
- `apps/frontend/src/features/nlq/workbench/queryWorkbenchState.ts`
- `apps/frontend/src/features/nlq/workbench/NotebookBlock.tsx`
- `apps/frontend/src/features/nlq/workbench/NotebookEditor.tsx`
- `apps/frontend/src/features/nlq/workbench/NotebookResults.tsx`
- `apps/frontend/src/features/nlq/workbench/*.test.tsx`

Reuse from the current codebase:
- `useTenantContext()`
- `useGlobalDateRange()`
- `submitNlqQuery()`
- `VisualizationPanel`
- `SignalQueryForm` styling and shorthand affordances where they fit
- `Panel`, `MetricCard`, `Button`, `Badge`, `LoadingState`, `EmptyState`

### Notebook State Model

Use a versioned state shape so future slices can evolve the notebook without breaking old URLs.

Minimum shape:

```ts
type NotebookStateV1 = {
  version: 1;
  title: string;
  blocks: NotebookBlockState[];
  activeBlockId?: string;
};

type NotebookBlockState = {
  id: string;
  signal: "metrics" | "logs" | "traces";
  mode: "nlq" | "raw";
  draft: string;
  collapsed?: boolean;
};
```

Serialisation should be deterministic so the same notebook state always produces the same shareable URL.

## Rendering Model

The starter layout should render three notebook blocks in a fixed order:
- metrics
- logs
- traces

Each block renders:
- a small header with the signal label and execution controls
- a Monaco editor
- a mode toggle between NLQ and raw IR
- the result area
- a disclosure for provenance when a result returns a frame

The result area should reuse the existing advisory display conventions:
- loading state
- error/decline state
- frame result
- provenance disclosure

For the first slice, each block may render its result vertically beneath the editor. Reordering and adding/removing blocks are deferred so the implementation can stay small and testable.

## Error Handling

Handle errors at the block level.

Expected states:
- invalid state blob in the URL: fall back to the starter notebook and preserve the raw blob only if it can be safely ignored
- query failure: show a block-level error state
- decline from NLQ: show the decline reason inline, not as a generic error
- invalid response from NLQ: show the existing invalid-response diagnostics pattern

The notebook shell itself should only fail if the state cannot be parsed at all and no starter notebook can be recovered.

## Auth and Tenancy

All calls remain tenant-scoped through the existing `tenantId` path.

This slice does not add new authorization rules or tenant model changes. The notebook is just another authenticated UI surface that reuses the current tenancy context.

## Testing Plan

The first implementation slice should add:

- unit tests for notebook state serialisation and parsing
- RTL tests for the starter layout and per-block mode toggles
- RTL tests for URL restoration and shareable state
- RTL tests for the three signal blocks rendering and each block preserving its own result state
- existing `submitNlqQuery` and visualization mocks where needed
- a Playwright accessibility check for the new workbench page

No new backend endpoint is introduced, so no new MSW handler is required for storage. The tests should mock the existing NLQ and signal APIs already used by the explorers.

## Rollback Path

Rollback is straightforward:
- remove the `/workbench` route
- remove the notebook page and feature folder
- restore the nav item if one was added

Because the feature is URL-only and frontend-only in the first slice, rollback does not require a data migration or server-side cleanup.

## ADR and Spec Sync

No new ADR is required for this slice.

Why:
- the feature does not change the architecture, deployment model, tenancy model, or persistence model
- it reuses the existing NLQ advisory pipeline and frontend routing model
- the new behavior is a UI composition change already covered by `spec/05-frontend.md`

The implementation plan should still point back to `spec/05-frontend.md` and the new workbench design doc.

## Open Questions Resolved

- **All signals or one signal?** All three signals in the starter notebook.
- **Server persistence or URL state?** URL state first, server persistence deferred.
- **Fixed layout or flexible layout?** Fixed starter layout now, reordering later.
- **NLQ-only or raw query only?** Hybrid, with NLQ default and raw IR behind an advanced toggle.

## Next Slice

Add the first implementation slice for the fixed three-block notebook with URL state, Monaco editor, and per-block NLQ/raw execution.

