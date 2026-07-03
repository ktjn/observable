# Saved Views in Explorers — Logs Slice — Design

**Status:** Draft
**Roadmap item:** Tier 1, "Saved Views in Explorers" (`docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`)
**Spec reference:** `spec/05-frontend.md` §9.11 — "Named bookmarks for search configurations (filter set + time range + column selection), scoped per user or shared within project."

## Goal

Let a user save the current log-explorer configuration (query, severity filter, message search, time range, visible columns) as a named, reloadable view — private to them or shared with the tenant. This is the first of three planned slices (logs, then traces, then metrics); the three explorers have different filter shapes today, so this slice ships end-to-end for logs only and establishes the pattern the other two will follow.

## Scope

**In scope:**
- Backend: `saved_views` + `saved_view_grants` Postgres tables and `services/query-api/src/saved_views.rs` CRUD module, mirroring the existing `dashboards.rs` pattern.
- Frontend: a "Saved Views" control in `LogSearch`'s toolbar (`SignalExplorer`) to save, load, rename, delete, and share a view.
- A minimal column-visibility toggle on `LogResultsTable` (show/hide only — no reordering or resizing), since a saved view needs *something* to persist for "columns."

**Out of scope (explicit non-goals):**
- Traces and metrics explorers — separate follow-up slices once this pattern is validated.
- URL deep-linking of a loaded view (loading a view rehydrates component state, it does not produce a shareable URL). The global time range already lives in the URL (`useGlobalDateRange`) and will continue to; only the log-specific filter fields (query, severity, message search) and column visibility are view-local state.
- Column reordering, resizing, or per-column formatting.
- View versioning/history — updating a saved view overwrites it.

## Data Model

New migration `migrations/postgres/038_create_saved_views.sql`, modeled directly on `012_create_dashboards.sql` + `030_add_dashboard_visibility.sql` + `031_create_dashboard_grants.sql`:

```sql
CREATE TABLE saved_views (
    saved_view_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    owner_user_id UUID NOT NULL,
    name TEXT NOT NULL,
    signal_kind TEXT NOT NULL CHECK (signal_kind IN ('logs')), -- widens to 'traces'/'metrics' in later slices
    visibility TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'public')),
    config JSONB NOT NULL, -- { query, severity_filter, message_search, time_range, visible_columns }
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_saved_views_tenant_signal ON saved_views (tenant_id, signal_kind);

CREATE TABLE saved_view_grants (
    saved_view_id UUID NOT NULL REFERENCES saved_views(saved_view_id) ON DELETE CASCADE,
    user_id UUID NOT NULL,
    relation TEXT NOT NULL CHECK (relation IN ('owner', 'editor', 'viewer')),
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (saved_view_id, user_id)
);
```

`config` is a JSONB blob (matching dashboards' `filters`/`time_range` precedent) rather than typed columns, because it is opaque to the backend — query-api never interprets it, only stores and returns it. `time_range` inside `config` reuses the same shape dashboards already use (`{ preset }` or `{ from, to }`).

## API

New module `services/query-api/src/saved_views.rs`, registered in `main.rs` alongside dashboards, same `middleware::auth::require_tenant` layer:

| Method | Path | Behavior |
|---|---|---|
| GET | `/v1/saved-views?signal_kind=logs` | List views visible to the caller: owner's own + `visibility='public'` + any with a grant. API-key callers (no user_id) see all tenant rows, same as dashboards. |
| POST | `/v1/saved-views` | Create; caller becomes owner (`owner_user_id` + an `owner` grant row, matching dashboard creation). |
| GET | `/v1/saved-views/{id}` | Fetch one; 404 if caller has no read access. |
| PUT | `/v1/saved-views/{id}` | Update `name`/`visibility`/`config`; requires `write` grant (`grant_satisfies_write`, reused from dashboards). |
| DELETE | `/v1/saved-views/{id}` | Requires `write` grant. |
| GET/POST/DELETE | `/v1/saved-views/{id}/grants` | Same grant-management shape as dashboards' `/grants` endpoints. |

Permission helpers (`grant_satisfies_read/write/delete`) are reused as-is from `dashboards.rs` — they're pure functions over `(visibility, relation)`, not dashboard-specific.

## Frontend

**Config capture:** a `LogViewConfig` type mirrors the JSONB shape: `{ query: string | null; severityFilter: SeverityFilter; messageSearch: string; timeRange: { preset: string } | { from: number; to: number }; visibleColumns: string[] }`.

**Toolbar control (`SignalExplorer`):** a "Saved Views" dropdown button next to the existing "Promote to dashboard" button:
- **Save current view** — opens a small dialog (name + visibility radio: Private/Shared), POSTs the current `LogViewConfig`.
- **Load** — dropdown lists views for `signal_kind=logs`, sorted owner's-first then alphabetical; selecting one applies its `config` to `LogExplorer`'s state (`setUserQuery`, `setSeverityFilter`, `setMessageSearch`, `setCustomRange`/`setPreset`, column visibility) and closes the dropdown.
- **Manage** — rename, change visibility, delete, and (for shared views) add/remove per-user grants — reuses the same grant-list UI pattern as the existing dashboard-sharing panel if one exists, otherwise a minimal list+add-user-by-id form.

**Column visibility:** `LogResultsTable` gains an optional `visibleColumns?: string[]` prop (undefined = show all, current behavior unchanged) and a column-picker (checkbox list in a popover) that calls back up to `LogExplorer` to update local state. This state is what gets captured into `visibleColumns` on save.

**API client:** `apps/frontend/src/api/savedViews.ts`, mirroring `api/dashboards.ts`'s shape (`fetchSavedViews`, `createSavedView`, `updateSavedView`, `deleteSavedView`, grant helpers).

## Error Handling

- Save with a duplicate name: allowed (no uniqueness constraint) — views are disambiguated by id, list UI shows all; this matches dashboards' behavior (no name-uniqueness there either).
- Load a view whose `config.query` references now-invalid NLQ IR (e.g., a facet that no longer exists): apply it anyway and let the existing NLQ error-handling path in `LogExplorer` surface the error the same way a hand-typed bad query would — no special-casing needed.
- 403/404 on load/save due to a permission change since the view was listed: surface via the existing toast/error-state pattern already used for other query-api calls.

## Testing

- Backend: `services/query-api` unit tests for the CRUD handlers + permission helpers reuse pattern (mirroring existing `dashboards` test module); a Testcontainers Postgres integration test for the migration and grant-based visibility, per the roadmap's operating rule 5.
- Frontend: component tests for the Saved Views dropdown (save/load/delete flows) and the column-picker, following the existing `LogSearch.test.tsx` patterns; MSW handlers for the new `/v1/saved-views` endpoints.

## Spec/ADR Sync

- `spec/05-frontend.md` §9.11: mark Saved Views as shipped (logs-only), reference this design doc.
- `spec/09-api.md`: add a `/v1/saved-views` section following the existing dashboard/alert-rule REST-resource format.
