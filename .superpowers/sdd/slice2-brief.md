# Task Brief — Slice 2: Discoverable Brush-to-Zoom

## Context
This is Slice 2 (P1) of the UI Usability Remediation plan, building on Slice 1 (histogram axes). The histogram component already supports drag-to-zoom via `onRangeSelect` but the affordance is hidden behind an `sr-only` paragraph. Sighted users get only a crosshair cursor with no label — the main time-narrowing interaction is undiscoverable.

## Task
Make the histogram's brush-to-zoom interaction visible and discoverable in `apps/frontend/src/components/ui/histogram.tsx`.

## Requirements

### 1. Visible drag-to-zoom hint in header (when `onRangeSelect` is provided)
- Add a "Drag to zoom" text hint in the histogram header area — visible to sighted users, not just screen readers
- Position: small text alongside the existing category legend (right side of the header bar), or below the header bar above the SVG
- Style: `text-xs text-[var(--muted)]` (subtle, not distracting)
- Only render when `onRangeSelect` is defined (same condition as the crosshair cursor)
- The existing `<p className="sr-only">Drag over bars to zoom into a time range.</p>` should be kept for screen reader users; the new hint is additive for sighted users

### 2. Reset-zoom control when a custom range is active
- Add a `selectedRange` optional prop to `HistogramProps`: `selectedRange?: { fromMs: number; toMs: number }`
- When `selectedRange` is provided, show a small "Reset zoom" button/link next to the "Drag to zoom" hint
- Clicking it calls `onRangeSelect(fullStart, fullEnd)` where `fullStart = buckets[0].startMs` and `fullEnd = buckets[buckets.length - 1].endMs` (the full range of available buckets)
- If `buckets` is empty, the reset button should not render even if `selectedRange` is provided
- Style: `text-xs text-[var(--muted)] hover:text-[var(--text)] underline cursor-pointer` (or a small button variant that matches existing patterns)
- The `selectedRange` prop is display-only — it tells the histogram that a zoom is active; the histogram itself does not manage zoom state (callers do)

### 3. Hover tooltip on bars
- Show a tooltip on each bar showing bucket time + count when hovering
- The bar segments already have a `title` attribute (e.g., `"10:00 ok: 5"`) — promote this to a real visible tooltip
- Implementation: on `mouseenter`/`mouseleave` on each `<g>` element (or the SVG), show a small floating `<div>` or SVG `<text>` with the bucket's time range + total count
- Simplest viable approach: a `<title>` element already exists; add a visible tooltip element that shows on hover
- The tooltip must show: time label (using `format(bucket.startMs)`) and total count
- Position the tooltip near the hovered bar (e.g., above or below it, clamped to SVG bounds)
- On mobile/touch (no hover), this can simply not show — no fallback needed
- **If implementing an HTML floating tooltip (preferred): use a React state `hoverTooltip: { bucketIdx: number; x: number } | null` and render an absolutely positioned `<div>` relative to the section container**

### No new dependencies
Pure React + SVG + Tailwind. No tooltip library.

## Key File
`apps/frontend/src/components/ui/histogram.tsx`

Current structure after Slice 1:
- `PLOT_HEIGHT = 96`, `X_AXIS_HEIGHT = 18`, `GAP_PX = 2`
- SVG: `height={PLOT_HEIGHT + X_AXIS_HEIGHT}`, `viewBox={0 0 ${width} ${PLOT_HEIGHT + X_AXIS_HEIGHT}}`
- Drag state: `dragRef`, `dragDisplay`, `handlePointerDown/Move/Up`
- Header: rendered when `(title || subtitle || categoryOrder.length > 0)`
- `onRangeSelect?: (fromMs: number, toMs: number) => void`

## Test File
`apps/frontend/src/components/ui/histogram.test.tsx` (already exists)

Add tests covering:
1. "Drag to zoom" hint renders when `onRangeSelect` is provided
2. "Drag to zoom" hint does NOT render when `onRangeSelect` is not provided
3. "Reset zoom" button renders when `selectedRange` is provided + `onRangeSelect` is provided
4. "Reset zoom" button calls `onRangeSelect` with full range when clicked (use `userEvent.click` or `fireEvent.click`)
5. Tooltip appears on bar hover (mouseenter on a `<g>` element)

Use existing test patterns (`render`, `screen`, `fireEvent` or `userEvent` from `@testing-library/react`).

## Verification
1. `npm run typecheck` from `apps/frontend/` — must pass
2. `npm test -- --testPathPattern=histogram` from `apps/frontend/` — all tests must pass

## Commit
```
feat(ui): add discoverable brush-to-zoom hint and hover tooltip to histogram
```

## Report Contract
Write full report to: `.superpowers/sdd/slice2-report.md`
Return only: status (DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED), commit hash, one-line test summary, and any concerns.
