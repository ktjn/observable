# Context Preservation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the `service` URL parameter a first-class TanStack Router root search param so it auto-inherits when navigating between the Logs, Traces, and Metrics pages — matching how `preset`/`from`/`to` already work.

**Architecture:** Add `service?: string` to `RootSearch` in `router.ts` and its `validateSearch`. Fix `useGlobalDateRange` to use functional navigation so `service` is preserved when the date range changes. Create a `useGlobalServiceFilter` hook (mirrors `useGlobalDateRange`). Replace the three `new URLSearchParams(window.location.search).get('service')` call sites.

**Tech Stack:** TypeScript, React, TanStack Router v1, Vitest.

**Design doc:** `docs/superpowers/specs/2026-05-31-context-preservation-design.md`

---

## Files Changed

| File | Change |
|---|---|
| `apps/frontend/src/router.ts` | Add `service?: string` to `RootSearch` type and `validateSearch` |
| `apps/frontend/src/hooks/useGlobalDateRange.ts` | Fix `updateSearch` to functional navigate so `service` is preserved on date changes |
| `apps/frontend/src/hooks/useGlobalServiceFilter.ts` | New: hook + `normalizeService` pure helper |
| `apps/frontend/src/hooks/useGlobalServiceFilter.test.ts` | New: unit tests for `normalizeService` |
| `apps/frontend/src/pages/LogSearch.tsx` | Replace `window.location.search` with `useGlobalServiceFilter()` |
| `apps/frontend/src/pages/TraceSearch.tsx` | Same |
| `apps/frontend/src/pages/MetricsSearch.tsx` | Same |
| `tests/e2e/smoke_test_unit.sh` | Assert `useGlobalServiceFilter.ts` exists |
| `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` | Mark Context Preservation complete |
| `docs/agent-context.md` | Update active plan note |

---

## Task 1: Add `service` to the router search schema

**Files:**
- Modify: `apps/frontend/src/router.ts`

- [ ] **Step 1: Add `service` to `RootSearch` type**

In `apps/frontend/src/router.ts`, the `RootSearch` type is defined around line 29. Update it:

```ts
export type RootSearch = {
  preset?: Preset;
  from?: number;
  to?: number;
  service?: string;
};
```

- [ ] **Step 2: Add `service` validation to `validateSearch`**

The `validateSearch` function in `rootRoute` currently returns `{ preset, from, to }`. Update it to also validate and return `service`:

```ts
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
    const service = typeof search.service === "string" && search.service.trim()
      ? search.service.trim()
      : undefined;
    return { preset, from, to, service };
  },
```

- [ ] **Step 3: Build to confirm no type errors**

```bash
cd apps/frontend && npm run build 2>&1 | tail -5
```

Expected: clean build or only pre-existing warnings. No new TypeScript errors about `service`.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/router.ts
git commit -m "feat(frontend): add service to root route search schema"
```

---

## Task 2: Fix `useGlobalDateRange` to preserve `service` on navigation

**Files:**
- Modify: `apps/frontend/src/hooks/useGlobalDateRange.ts`

Currently `updateSearch` calls `navigate({ search: nextSearch })` which replaces the entire search object. After adding `service` to the schema, changing the date range would silently drop `?service=...` from the URL. The fix: use TanStack Router's functional navigate form `navigate({ search: (prev) => ({ ...prev, ...nextSearch }) })` which merges with existing params.

- [ ] **Step 1: Update `updateSearch` to use functional navigate**

In `apps/frontend/src/hooks/useGlobalDateRange.ts`, find the `updateSearch` function (around line 52) and replace it:

```ts
  const updateSearch = (nextSearch: Partial<RootSearch>) => {
    navigate({
      search: (prev: RootSearch) => ({ ...prev, ...nextSearch }),
    } as unknown as Parameters<typeof navigate>[0]);
  };
```

The change: `nextSearch` type changes from `RootSearch` to `Partial<RootSearch>`, and `search` becomes a function that spreads `prev` before applying the update. This preserves `service` (and any future root params) when the date range changes.

- [ ] **Step 2: Verify the existing date range tests still pass**

```bash
cd apps/frontend && npm run test -- --reporter=verbose hooks/useGlobalDateRange 2>&1 | tail -10
```

Expected: all existing tests pass (`presetToMs`, `deriveRange`, `PRESET_OPTIONS` tests — these test pure functions and are unaffected by the navigate change).

- [ ] **Step 3: Build to confirm no type errors**

```bash
cd apps/frontend && npm run build 2>&1 | tail -5
```

Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add apps/frontend/src/hooks/useGlobalDateRange.ts
git commit -m "fix(frontend): preserve service param when updating date range"
```

---

## Task 3: Create `useGlobalServiceFilter` hook (TDD)

**Files:**
- Create: `apps/frontend/src/hooks/useGlobalServiceFilter.ts`
- Create: `apps/frontend/src/hooks/useGlobalServiceFilter.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `apps/frontend/src/hooks/useGlobalServiceFilter.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { normalizeService } from "./useGlobalServiceFilter";

describe("normalizeService", () => {
  it("returns the string for a non-empty string", () => {
    expect(normalizeService("checkout")).toBe("checkout");
  });

  it("trims whitespace from both ends", () => {
    expect(normalizeService("  checkout  ")).toBe("checkout");
  });

  it("returns undefined for an empty string", () => {
    expect(normalizeService("")).toBeUndefined();
  });

  it("returns undefined for a whitespace-only string", () => {
    expect(normalizeService("   ")).toBeUndefined();
  });

  it("returns undefined for undefined", () => {
    expect(normalizeService(undefined)).toBeUndefined();
  });

  it("returns undefined for null", () => {
    expect(normalizeService(null)).toBeUndefined();
  });

  it("returns undefined for a number", () => {
    expect(normalizeService(42)).toBeUndefined();
  });
});
```

- [ ] **Step 2: Run tests — verify RED**

```bash
cd apps/frontend && npm run test -- --reporter=verbose hooks/useGlobalServiceFilter 2>&1 | tail -10
```

Expected: all 7 tests fail because `normalizeService` is not defined yet.

- [ ] **Step 3: Implement `useGlobalServiceFilter.ts`**

Create `apps/frontend/src/hooks/useGlobalServiceFilter.ts`:

```ts
import { useNavigate, useSearch } from "@tanstack/react-router";
import type { RootSearch } from "../router";

/**
 * Validates and normalises a raw search param value to a trimmed string
 * or undefined. Used by validateSearch in router.ts and by this hook.
 */
export function normalizeService(raw: unknown): string | undefined {
  return typeof raw === "string" && raw.trim() ? raw.trim() : undefined;
}

export interface GlobalServiceFilter {
  service: string | undefined;
  setService: (next: string | undefined) => void;
}

/**
 * Read and write the global service filter from the root route's URL search
 * params. Mirrors useGlobalDateRange — setting the service preserves the
 * existing date range params, and vice versa.
 */
export function useGlobalServiceFilter(): GlobalServiceFilter {
  const search = useSearch({ strict: false }) as RootSearch;
  const navigate = useNavigate();

  const setService = (next: string | undefined) => {
    navigate({
      search: (prev: RootSearch) => ({
        ...prev,
        service: normalizeService(next),
      }),
    } as unknown as Parameters<typeof navigate>[0]);
  };

  return { service: search.service, setService };
}
```

- [ ] **Step 4: Run tests — verify GREEN**

```bash
cd apps/frontend && npm run test -- --reporter=verbose hooks/useGlobalServiceFilter 2>&1 | tail -10
```

Expected: all 7 tests pass.

- [ ] **Step 5: Build to confirm no type errors**

```bash
cd apps/frontend && npm run build 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/hooks/useGlobalServiceFilter.ts \
        apps/frontend/src/hooks/useGlobalServiceFilter.test.ts
git commit -m "feat(frontend): add useGlobalServiceFilter hook for URL-persisted service param"
```

---

## Task 4: Update the three page call sites

**Files:**
- Modify: `apps/frontend/src/pages/LogSearch.tsx`
- Modify: `apps/frontend/src/pages/TraceSearch.tsx`
- Modify: `apps/frontend/src/pages/MetricsSearch.tsx`

Each page currently reads the service filter with `new URLSearchParams(window.location.search).get("service") ?? ""`. Replace each with `useGlobalServiceFilter()`.

- [ ] **Step 1: Update `LogSearch.tsx`**

In `apps/frontend/src/pages/LogSearch.tsx`, add the import at the top:

```ts
import { useGlobalServiceFilter } from "../hooks/useGlobalServiceFilter";
```

Replace the `LogSearch` component function (around line 76):

```ts
export default function LogSearch() {
  const { service } = useGlobalServiceFilter();
  return (
    <LogExplorer
      initialService={service ?? ""}
    />
  );
}
```

- [ ] **Step 2: Update `TraceSearch.tsx`**

In `apps/frontend/src/pages/TraceSearch.tsx`, add the import near the top with the other hook imports:

```ts
import { useGlobalServiceFilter } from "../hooks/useGlobalServiceFilter";
```

Replace the `TraceSearch` default export function (at line ~106):

```ts
// Before:
export default function TraceSearch() {
  return (
    <TraceExplorer
      initialService={new URLSearchParams(window.location.search).get("service") ?? ""}
    />
  );
}

// After:
export default function TraceSearch() {
  const { service } = useGlobalServiceFilter();
  return (
    <TraceExplorer
      initialService={service ?? ""}
    />
  );
}
```

- [ ] **Step 3: Update `MetricsSearch.tsx`**

In `apps/frontend/src/pages/MetricsSearch.tsx`, replace the entire file with:

```ts
import { useGlobalServiceFilter } from "../hooks/useGlobalServiceFilter";
import { ServiceMetricsWorkspace } from "../features/metrics/ServiceMetricsWorkspace";

export default function MetricsSearch() {
  const { service } = useGlobalServiceFilter();
  return (
    <ServiceMetricsWorkspace
      initialService={service ?? ""}
      lockedService={false}
    />
  );
}
```

- [ ] **Step 4: Build to confirm no errors**

```bash
cd apps/frontend && npm run build 2>&1 | tail -5
```

Expected: clean build. No `window.location.search` references remain in the three pages.

- [ ] **Step 5: Verify no remaining `window.location.search` in the page files**

```bash
grep -n "window.location.search" apps/frontend/src/pages/LogSearch.tsx apps/frontend/src/pages/TraceSearch.tsx apps/frontend/src/pages/MetricsSearch.tsx
```

Expected: no output (all occurrences replaced).

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/pages/LogSearch.tsx \
        apps/frontend/src/pages/TraceSearch.tsx \
        apps/frontend/src/pages/MetricsSearch.tsx
git commit -m "feat(frontend): use useGlobalServiceFilter in LogSearch, TraceSearch, MetricsSearch"
```

---

## Task 5: Smoke test + roadmap update

**Files:**
- Modify: `tests/e2e/smoke_test_unit.sh`
- Modify: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`
- Modify: `docs/agent-context.md`

- [ ] **Step 1: Add hook existence assertion to `smoke_test_unit.sh`**

In `tests/e2e/smoke_test_unit.sh`, after the existing `test_storage_writer_has_write_buffer` function and its `run_test` registration, add:

```bash
test_frontend_has_service_filter_hook() {
  local hook="$SCRIPT_DIR/../../apps/frontend/src/hooks/useGlobalServiceFilter.ts"

  if [ ! -f "$hook" ]; then
    echo "FAIL: apps/frontend/src/hooks/useGlobalServiceFilter.ts must exist (URL-persisted service filter hook)"
    exit 1
  fi
}
```

Add the `run_test` registration:

```bash
run_test "frontend has service filter hook" test_frontend_has_service_filter_hook
```

- [ ] **Step 2: Run `smoke_test_unit.sh` to verify all tests pass**

```bash
bash tests/e2e/smoke_test_unit.sh
```

Expected: all tests pass including the new one.

- [ ] **Step 3: Mark Context Preservation complete in the roadmap**

In `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`, find:

```markdown
- [ ] **Context Preservation**: Ensure all tabs (Logs, Metrics, Traces, etc.) consistently apply the service filter and global date range from the URL.
```

Replace with:

```markdown
- [x] **Context Preservation**: Ensure all tabs (Logs, Metrics, Traces, etc.) consistently apply the service filter and global date range from the URL. (COMPLETED 2026-05-31) `service` added to `RootSearch` in `router.ts`; `useGlobalServiceFilter` hook created (mirrors `useGlobalDateRange`); `LogSearch`, `TraceSearch`, `MetricsSearch` updated to use the hook. `useGlobalDateRange` fixed to use functional navigate so `service` is preserved when the date range changes.
```

- [ ] **Step 4: Update `docs/agent-context.md`**

Find:

```
- Active detailed implementation plan: none — RF-2, RF-3, RF-6, P4-S9, stream-processor batching, Telemetry Loop Prevention, P4-S4 dashboard ReBAC, and ClickHouse insert efficiency complete. Next: P4-S3b SCIM/SSO (if required by v1 customers), Context Preservation (frontend), or Live Tail.
```

Replace with:

```
- Active detailed implementation plan: none — RF-2, RF-3, RF-6, P4-S9, stream-processor batching, Telemetry Loop Prevention, P4-S4 dashboard ReBAC, ClickHouse insert efficiency, and Context Preservation complete. Next: Live Tail or P4-S3b SCIM/SSO (if required by v1 customers).
```

- [ ] **Step 5: Format and commit**

```bash
git add tests/e2e/smoke_test_unit.sh \
        docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md \
        docs/agent-context.md
git commit -m "chore(docs): mark Context Preservation complete, update agent-context"
```

---

## Verification Checklist (run before pushing)

- [ ] `cd apps/frontend && npm run test -- hooks/useGlobalServiceFilter hooks/useGlobalDateRange` — all tests pass
- [ ] `cd apps/frontend && npm run build` — clean build, no TypeScript errors
- [ ] `grep -r "window.location.search" apps/frontend/src/pages/` — no results in LogSearch, TraceSearch, MetricsSearch
- [ ] `bash tests/e2e/smoke_test_unit.sh` — all 20 tests pass
