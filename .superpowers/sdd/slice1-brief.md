# Task Brief — Slice 1: Histogram Axes & Readable Scale

## Context
This is Slice 1 (P0) of the UI Usability Remediation plan. The histogram component (`apps/frontend/src/components/ui/histogram.tsx`) renders stacked bar charts for Traces, Logs, and Service Detail pages. Currently it shows bars and 3 unlabeled gridlines only — no time axis, no count axis. Operators cannot read *when* a spike happened or *how many* events it represents.

## Task
Add an x-axis with 3–5 time ticks and a y-axis max label to `apps/frontend/src/components/ui/histogram.tsx`.

## Requirements

### X-axis
- Show 3–5 evenly-spaced time labels below the bar area
- Use the existing `format` prop (signature: `(ms: number) => string`) to format each label
- Pick tick positions from `buckets` (e.g., first, last, and 1–3 evenly-spaced middle buckets)
- Do not show x-axis labels when `buckets` is empty

### Y-axis max label
- Show the max count (the `max` local variable = `Math.max(1, ...buckets.map(b => b.total))`) as a single label at the top of the y-axis
- Render as a small text element (e.g., top-left of the plot area, y≈10)
- Format: plain number (e.g., `"1 204"` with space as thousands separator, or just raw number — keep it simple)
- Do not show when `buckets` is empty or max is 1 (placeholder)

### SVG layout
- Reserve bottom gutter for x-axis: add `X_AXIS_HEIGHT = 18` constant; update SVG `height` and `viewBox` from `PLOT_HEIGHT` to `PLOT_HEIGHT + X_AXIS_HEIGHT`
- Keep all existing bar/gridline rendering unchanged (they still use `PLOT_HEIGHT` for their coordinate space)
- Render x-axis `<text>` elements with `y={PLOT_HEIGHT + 13}` (within the new gutter)
- Render y-axis max `<text>` element inside the plot area (e.g., `x={4}`, `y={10}`)
- Text style: `fontSize={10}`, `fill="var(--muted)"` (matching existing `text-[var(--muted)]` patterns)
- Anchor x-axis ticks: first tick `textAnchor="start"`, last tick `textAnchor="end"`, middle ticks `textAnchor="middle"`
- Anchor y-axis max label: `textAnchor="start"`

### No new dependencies
The component is hand-rolled SVG. Do not add any chart library.

## Key File
`apps/frontend/src/components/ui/histogram.tsx`

Current structure summary:
- `PLOT_HEIGHT = 96`, `GAP_PX = 2`
- SVG: `<svg width="100%" height={PLOT_HEIGHT} viewBox={0 0 ${width} ${PLOT_HEIGHT}}>`  
- `max = Math.max(1, ...buckets.map(b => b.total))`
- `format` prop: `(ms: number) => string`
- `buckets` prop: `HistogramBucket<T>[]` where each has `startMs`, `endMs`, `total`, `categories`

## Test File
Write unit tests in `apps/frontend/src/components/ui/histogram.test.tsx` (create if it doesn't exist — check first with glob).

Tests must cover:
1. With 10 buckets: `<text>` elements for time ticks are rendered (at least 3)
2. With 10 buckets: the max label text is present (e.g., the rendered SVG contains the max count string)
3. With 0 buckets: no `<text>` elements rendered (or specifically no tick/max labels)
4. Snapshot or tick-count assertions as appropriate

Use existing test patterns from the frontend (check `apps/frontend/src/components/ui/` for sibling test files first).

## Verification
After implementation:
1. Run `npm run typecheck` from `apps/frontend/`
2. Run `npm test -- --testPathPattern=histogram` from `apps/frontend/`
3. The test must pass. Typecheck must pass.

## Commit
Commit to the current branch (`worktree-feat+ui-usability-remediation`):
```
fix(ui): add time and count axes to histogram component
```

## Report Contract
Write your full report to: `.superpowers/sdd/slice1-report.md`
Return only: status (DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED), the commit hash, a one-line test summary, and any concerns.
