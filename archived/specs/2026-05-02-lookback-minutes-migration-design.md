# Lookback Minutes Migration Design

**Date:** 2026-05-02
**Status:** Approved

## Overview

Remove `lookback_minutes` from all API contracts (frontend and backend) and replace with explicit `from`/`to` timestamp ranges. Dashboard panels adopt a hybrid model: they default to the global date range but can optionally override with a stored preset string.

## Motivation

The global date range system (`useGlobalDateRange`, `RootSearch`) is now the authoritative time window across all pages. Several API call sites still convert that range back into `lookback_minutes` before sending to the backend, which is a lossy round-trip. The backend endpoints in `discovery.rs` and `dashboards.rs` only accept `lookback_minutes`, preventing direct use of `from`/`to`. This migration aligns the full stack on a single time representation.

## Database Migration

New migration file: `migrations/postgres/012_dashboard_preset.sql`

1. Add `preset TEXT` column (nullable) to `dashboard_panels`.
2. Backfill: snap each row's `lookback_minutes` to the nearest preset string using this mapping:
   - ≤ 5 min → `"5m"`, ≤ 15 min → `"15m"`, ≤ 30 min → `"30m"`, ≤ 60 min → `"1h"`, ≤ 180 min → `"3h"`, otherwise → `"12h"`
3. Drop the `lookback_minutes` column.

## Backend Changes

All changes are clean breaks — no fallback support for `lookback_minutes`.

### `services/query-api/src/discovery.rs`

- `SummaryParams`: replace `lookback_minutes: Option<u32>` with `from: Option<DateTime<Utc>>`, `to: Option<DateTime<Utc>>`.
- `TopologyParams`: same replacement.
- Query logic updated to use `from`/`to` directly.

### `services/query-api/src/traces.rs`

- `SearchParams`: remove `lookback_minutes: Option<u32>`. `from`/`to` already exist.

### `services/query-api/src/dashboards.rs`

- `DashboardPanelItem`: replace `lookback_minutes: i32` with `preset: Option<String>`.
- UPDATE SQL and SELECT queries updated accordingly.

## Frontend API Layer

### `apps/frontend/src/api/services.ts`

Replace `lookback_minutes?: number` with `from?: number; to?: number` (milliseconds) on:
- `listServiceSummaries`
- `getServiceSummary`
- `getTopology`
- `getServiceResponseTimeHistory`

Serialize to ISO strings when building query params.

### `apps/frontend/src/api/traces.ts`

Remove `lookback_minutes` from `fetchTraceHistogram` params. `from`/`to` already exist.

### `apps/frontend/src/api/dashboards.ts`

- `DashboardPanel`: replace `lookback_minutes: number` with `preset?: Preset | null`.

## Frontend Pages & Dashboard Logic

### `apps/frontend/src/pages/ServiceDetailPage.tsx`

Remove the `Math.round((toMs - fromMs) / 60_000)` conversion. Pass `fromMs`/`toMs` directly to all API calls.

### Dashboard Hybrid Logic

When fetching data for a panel:
- `panel.preset !== null` → derive `fromMs`/`toMs` from `presetToMs(panel.preset)` (reuse existing helper).
- `panel.preset === null` → use `fromMs`/`toMs` from `useGlobalDateRange()`.

When creating/editing a panel:
- UI offers "Use global date range" option (saves `preset: null`) plus a preset dropdown.
- New panels default to `preset: null`.

## Preset Mapping Reference

| Preset string | Duration |
|---|---|
| `"5m"` | 5 minutes |
| `"15m"` | 15 minutes |
| `"30m"` | 30 minutes |
| `"1h"` | 60 minutes |
| `"3h"` | 180 minutes |
| `"12h"` | 720 minutes |

These match the existing `Preset` type in `apps/frontend/src/router.ts`.

## Out of Scope

- E2E test fixtures that reference `?lookback_minutes=60` in URLs — update as part of this migration since those endpoints will no longer accept the param.
- Any other callers discovered during implementation should be migrated in the same PR.
