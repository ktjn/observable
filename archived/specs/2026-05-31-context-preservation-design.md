# Context Preservation Design

**Date:** 2026-05-31
**Status:** Approved for implementation

---

## Goal

Ensure the service filter persists when navigating between the Logs, Traces, and Metrics search pages. Currently the `service` query parameter is read via raw `window.location.search` — it is not part of TanStack Router's validated search schema and is silently dropped on navigation. Date range (`preset`, `from`, `to`) already works correctly; this design makes service filter follow the identical pattern.

---

## Context

`apps/frontend/src/router.ts` defines a `RootSearch` type used by TanStack Router to validate and forward URL search params across all routes. It currently contains:

```ts
{ preset?: string, from?: number, to?: number }
```

The `useGlobalDateRange` hook reads and writes these params via `useSearch({ from: '__root__' })` and `useNavigate()`. All pages inherit date range automatically on navigation because it lives in the root schema.

The service filter is handled differently in three pages:

```ts
// LogSearch.tsx, TraceSearch.tsx, ServiceMetricsWorkspace.tsx (approx.)
const service = new URLSearchParams(window.location.search).get('service') ?? undefined
```

This bypasses the router entirely. When navigating between pages, TanStack Router does not preserve params it does not know about, so the service filter is lost.

---

## Architecture

### `apps/frontend/src/router.ts`

Add `service?: string` to the root route's search schema validator:

```ts
// Before
validateSearch: (search) => ({
  preset: typeof search.preset === 'string' ? search.preset : undefined,
  from: typeof search.from === 'number' ? search.from : undefined,
  to: typeof search.to === 'number' ? search.to : undefined,
}),

// After
validateSearch: (search) => ({
  preset: typeof search.preset === 'string' ? search.preset : undefined,
  from: typeof search.from === 'number' ? search.from : undefined,
  to: typeof search.to === 'number' ? search.to : undefined,
  service: typeof search.service === 'string' ? search.service : undefined,
}),
```

TanStack Router forwards all validated root params automatically on every navigation — no per-link changes needed.

### `apps/frontend/src/hooks/useGlobalServiceFilter.ts` (new file)

Mirrors `useGlobalDateRange.ts` exactly:

```ts
import { useNavigate, useSearch } from '@tanstack/react-router'

export function useGlobalServiceFilter() {
  const { service } = useSearch({ from: '__root__' })
  const navigate = useNavigate()

  const setService = (next: string | undefined) => {
    navigate({ search: (prev) => ({ ...prev, service: next || undefined }) })
  }

  return { service, setService }
}
```

`setService(undefined)` or `setService('')` clears the param from the URL.

### Updated call sites (3 files)

Replace the `window.location.search` pattern with the hook in:

- `apps/frontend/src/pages/LogSearch.tsx`
- `apps/frontend/src/pages/TraceSearch.tsx`
- `apps/frontend/src/pages/MetricsSearch.tsx`

**Before:**
```ts
const service = new URLSearchParams(window.location.search).get('service') ?? undefined
```

**After:**
```ts
const { service } = useGlobalServiceFilter()
```

### No changes needed

- `ServiceDetailPage.tsx` — passes `service` as a prop to child components directly, not from URL params
- Date range components — unchanged
- Navigation links (tab bar) — no manual param forwarding needed; root schema inheritance handles it

---

## Data Flow

1. User is on `/logs?service=checkout&preset=1h`
2. User navigates to Traces tab → URL becomes `/traces?service=checkout&preset=1h`
3. `TraceSearch` calls `useGlobalServiceFilter()` → receives `service = "checkout"` → passes to the trace API call
4. User clears the filter → `setService(undefined)` → URL becomes `/traces?preset=1h`

---

## Testing

### Hook unit test (`useGlobalServiceFilter.test.ts`)

- Render inside a TanStack Router test harness with initial search `?service=checkout` → assert `service === 'checkout'`
- Call `setService('api')` → assert URL updated to `?service=api`
- Call `setService(undefined)` → assert `service` param removed from URL

### Smoke test

Add an assertion to `tests/e2e/smoke_test_unit.sh` that `apps/frontend/src/hooks/useGlobalServiceFilter.ts` exists.

### Manual verification

Navigate between Logs, Traces, and Metrics pages with a service filter set in the URL; confirm the `service` param persists in all tab URLs.

---

## Backward Compatibility

- URLs with `?service=...` already work on current pages (raw string read) — behavior is unchanged, only the mechanism improves
- No API contract changes
- No migration needed for existing bookmarks (URLs remain the same shape)
