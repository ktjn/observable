# Live Tail Design

**Date:** 2026-05-31
**Status:** Approved for implementation

---

## Goal

Add a "Live" toggle to `LogExplorer` that switches from historical search to a cursor-based polling session: new log rows are fetched every 5 seconds via the existing `tailLogs` API, appended to a sliding window of 500 rows, and the table auto-scrolls to the bottom as rows arrive.

---

## Context

The `tailLogs` function and the `/v1/logs/tail` backend endpoint already exist in `apps/frontend/src/api/logs.ts`. The spec (§9.18) requires cursor-based polling with the same filter semantics as historical search. The `LIVE_VIEW_REFRESH_INTERVAL_MS = 5_000` constant in `useLiveRefresh.ts` is reused as the poll interval.

---

## Architecture

### New file: `apps/frontend/src/hooks/useLiveTail.ts`

Owns all live tail state. Takes options and returns the accumulated log rows and any fetch error.

**Interface:**
```ts
interface UseLiveTailOptions {
  tenantId: string
  service?: string
  severityMin?: number
  enabled: boolean
}

interface UseLiveTailResult {
  logs: LogRecord[]
  error: Error | null
}

function useLiveTail(opts: UseLiveTailOptions): UseLiveTailResult
```

**Behaviour:**
- When `enabled` transitions to `true`: cursor `useRef` is set to `String(Date.now() * 1_000_000)` (nanosecond timestamp as string, matching the `since_unix_nano` API param type) and a `setInterval` of `LIVE_VIEW_REFRESH_INTERVAL_MS` (5 000 ms) is started.
- On each tick: calls `tailLogs({ service, severity: severityMin, since_unix_nano: cursor.current, limit: 100 })`, appends new rows to the accumulator, advances the cursor to the `timestamp_unix_nano` of the newest row in the response (or leaves the cursor unchanged if no rows returned), trims the accumulator to the last 500 entries using `appendAndTrim`.
- When `enabled` transitions to `false`: interval cleared, accumulator reset to `[]`, error reset to `null`.
- On fetch error: error state is set; accumulator and cursor are unchanged (next tick retries).

**Pure helper (exported for tests):**
```ts
export function appendAndTrim<T>(
  prev: T[],
  next: T[],
  maxRows: number
): T[]
```
Concatenates `prev` and `next`, then returns the last `maxRows` entries.

### Modified: `apps/frontend/src/pages/LogSearch.tsx` (`LogExplorer` component)

**State added:**
```ts
const [isLive, setIsLive] = useState(false)
```

**Hook wired:**
```ts
const SEVERITY_MIN: Partial<Record<SeverityFilter, number>> = {
  error: 17, warn: 13, info: 9,
}

const liveTail = useLiveTail({
  tenantId: ctx.tenantId,
  service: service || undefined,
  severityMin: SEVERITY_MIN[severityFilter],
  enabled: isLive,
})
```

**"Live" button:** Added to the header row alongside the existing controls. Renders with a pulsing green dot indicator when active:
```tsx
<button onClick={() => setIsLive(v => !v)}>
  {isLive ? '⏹ Stop' : '▶ Live'}
</button>
```

**Conditional rendering when `isLive`:**
- Table shows `liveTail.logs` instead of the historical query result
- Histogram is hidden
- Date range picker is disabled (live mode tails from now)
- Error banner shows `liveTail.error` if present

**Auto-scroll:**
A `useRef` on the table scroll container tracks whether the user has manually scrolled up (`userScrolledUp`). A scroll event listener sets this ref to `true` when the scroll position is not at the bottom, and back to `false` when it returns to the bottom. A `useEffect` watching `liveTail.logs.length` calls `scrollContainer.current?.scrollTop = scrollContainer.current?.scrollHeight` when `!userScrolledUp.current`.

---

## Data Flow

```
User clicks "Live"
  → isLive = true
  → useLiveTail: cursor = now_ns, interval starts

Every 5 s:
  → tailLogs({ since_unix_nano: cursor, limit: 100 })
  → new rows: appendAndTrim(accumulator, newRows, 500)
  → cursor = newest timestamp_unix_nano in response
  → LogExplorer renders live rows
  → useEffect: scroll container scrolled to bottom (if !userScrolledUp)

User scrolls up:
  → userScrolledUp = true → auto-scroll paused

User scrolls to bottom:
  → userScrolledUp = false → auto-scroll resumes

User clicks "Stop":
  → isLive = false
  → useLiveTail: interval cleared, accumulator reset
  → historical query resumes
```

---

## Error Handling

- If `tailLogs` throws, `error` is set and shown in a banner. The interval keeps running; the next tick retries automatically.
- If the tenant session expires (401 response), the error banner prompts re-authentication. The interval is not cleared — the user must manually stop live tail or re-authenticate.

---

## Testing

### Unit tests: `useLiveTail.test.ts`

Uses `vi.useFakeTimers()` and `vi.spyOn` on `tailLogs`.

1. **Rows accumulate** — advance timer by 5 s, assert accumulator grows by the mocked response rows
2. **Cursor advances** — second tick passes `since_unix_nano` equal to the newest `timestamp_unix_nano` from the first response
3. **Sliding window cap** — mock returns 300 rows per tick; after 2 ticks (600 total), assert accumulator length === 500
4. **Disabling clears state** — toggle `enabled` false, assert accumulator reset to `[]`
5. **Error is surfaced** — mock throws, assert `error` is set; accumulator unchanged

### Unit tests: `appendAndTrim.test.ts` (inline in `useLiveTail.test.ts`)

- `appendAndTrim([], [a,b,c], 2)` → `[b,c]`
- `appendAndTrim([a,b], [c], 3)` → `[a,b,c]`
- `appendAndTrim([a,b,c], [d,e], 3)` → `[c,d,e]`

### Smoke test

Assert `apps/frontend/src/hooks/useLiveTail.ts` exists in `tests/e2e/smoke_test_unit.sh`.

---

## Backward Compatibility

- When `isLive` is false, `LogExplorer` behaves identically to before
- `LogSearch` default export is unchanged
- Existing `liveViewQueryOptions` usage on the historical queries is unaffected
