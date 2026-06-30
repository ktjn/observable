# Slice 2 Report — Discoverable Brush-to-Zoom

## Status
DONE

## Commit
e7c24d0

## Test Summary
17 tests passed (1 test file) — 8 new tests covering all 5 brief requirements: drag-to-zoom hint visibility, reset-zoom button, reset-zoom click behavior, empty-buckets guard, tooltip on mouseenter, tooltip dismissal on mouseleave.

## What Was Implemented

### 1. "Drag to zoom" hint (Req 1)
- Added `<span>Drag to zoom</span>` in the header's right-side legend area
- Only renders when `onRangeSelect` is provided
- Styled `text-xs text-[var(--muted)]`
- Header condition expanded from `(title || subtitle || categoryOrder.length > 0)` to also include `|| onRangeSelect` so the hint appears even when no title/categories are set
- Existing `<p className="sr-only">` preserved

### 2. Reset zoom control (Req 2)
- Added `selectedRange?: { fromMs: number; toMs: number }` to `HistogramProps`
- "Reset zoom" `<button>` renders only when `onRangeSelect && selectedRange && buckets.length > 0`
- Clicking calls `onRangeSelect(buckets[0].startMs, buckets[buckets.length - 1].endMs)`
- Styled `text-xs text-[var(--muted)] underline hover:text-[var(--text)] cursor-pointer`

### 3. Hover tooltip (Req 3)
- Added `hoverTooltip: { bucketIdx: number } | null` state
- Each `<g>` bar element has `onMouseEnter`/`onMouseLeave` handlers
- Absolutely positioned `<div data-testid="histogram-tooltip">` rendered below the SVG, inside the `relative`-positioned section container
- Shows `format(bucket.startMs)` and `bucket.total`
- Positioned via CSS: `left` as percentage of container width, `bottom` above the x-axis

## Concerns
None.
