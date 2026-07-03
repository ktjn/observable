# Log Quick Filter (Regex) and Span Error Visibility — Design

**Status:** Draft

## Background

Investigation (screenshots + code) of the current UI surfaced two related UX gaps:

1. **Log explorer search is split across two inputs with unclear roles.** `LogSearch.tsx` has a main "Filter logs" box that accepts NLQ text or, with a `/` prefix, a shorthand grammar (`field:value`, `"quoted exact"`, unquoted free-text terms — dispatched server-side in `llm_adapter.rs`'s `tokenize_shorthand`). A hover-only tooltip (`ShorthandHint.tsx`) documents this, but it's easy to miss since nothing about the input signals there's a hidden mode. Separately, a "Search log messages" box next to the severity pills already does a live, client-side substring filter over the currently-loaded rows — but it has no visible relationship to the main box, and supports no regex.
2. **Span error status is not reliably visible in the trace waterfall.** `TraceDetail.tsx` overrides a span bar's fill color to the "bad" color when `status_code === "ERROR"`, but the waterfall's only legend entries are per-service colors — there's no "Error" swatch, and no icon or text per row. A red bar is visually indistinguishable from "this service's color happens to be reddish" unless the viewer already knows the convention. Only the single selected span's side detail panel shows an explicit "ERROR" text badge.

## Goal

1. Make the logs explorer's existing quick-filter box explicitly regex-capable and clearly labeled, without touching the separate NLQ/shorthand query box.
2. Make span error status visible per-row in the trace waterfall through a channel independent of fill color (border accent + text), matching the pattern already used elsewhere in the codebase (`LogResultsTable`'s severity accent, `TraceResultsTable`'s trace-level error accent).

## Scope

**In scope:**
- `apps/frontend/src/pages/LogSearch.tsx`: relabel the existing message-search box, add a regex-mode toggle, wire matching to support both substring and regex.
- `apps/frontend/src/pages/TraceDetail.tsx`: add a red left-border accent and an "ERROR" text marker to each waterfall row whose span has `status_code === "ERROR"`, in addition to the existing fill-color override.

**Out of scope (explicit non-goals):**
- Changing the main NLQ/shorthand query box (`QueryFilterInput.tsx`) or its backend dispatch (`llm_adapter.rs`) in any way. No new prefix convention is added there.
- Making the quick filter a server-side query — it stays a client-side filter over already-loaded rows, exactly as today.
- Changing `TraceResultsTable`'s trace-list Status column semantics (root-span-only status). That's a separate, larger question about aggregate-vs-root-span error semantics and is not bundled into this visibility fix.
- Adding error visibility to `TraceSearch.tsx`'s list view beyond what already exists (status pills, error-rate card, root-span badge) — this design only touches the waterfall in `TraceDetail.tsx`.

## Part 1: Log Quick Filter

**Current state:** `LogSearch.tsx` holds `messageSearch: string` state, filtered via `formatLogMessage(l.body).toLowerCase().includes(needle)` against the already-fetched/tailed log rows (`messageFilteredLogs` in `LogExplorer`). The input's placeholder is "Search messages…" with `aria-label="Search log messages"`.

**Change:**
- Placeholder becomes `"Quick filter — plain text or regex"`.
- A new toggle button, `.*`, sits immediately to the right of the input (same toolbar row as the severity pills and Live button). Toggling it on switches matching from substring to regex; the button's pressed state (`aria-pressed`) reflects the mode, matching the existing Live-tail button's own `aria-pressed` pattern in the same row.
- Matching logic: in regex mode, compile `messageSearch` as a case-insensitive `RegExp` and test it against the formatted message; an invalid pattern (e.g. unbalanced parens) falls back to "no filter applied" rather than throwing, with a small inline note ("Invalid regex — showing all results") reusing the existing warning-text styling already used for the live-tail NLQ notice in the same component.
- No change to `userQuery`/NLQ state, no change to `LOG_BASE_IR`, no change to the query-input's own tooltip or backend calls.

## Part 2: Span Error Visibility in the Waterfall

**Current state:** In `TraceDetail.tsx`'s waterfall rendering, each span row's bar gets `fill={span.status_code === "ERROR" ? "var(--bad)" : serviceColor}` — the only signal of an error is this color swap, and the waterfall's legend (top of the page) lists only service-name/color pairs, no error entry.

**Change:**
- Each waterfall row gets a `border-left` accent (`border-l-2 border-l-[var(--bad)]`) applied to the row container when `span.status_code === "ERROR"` — mirroring the exact class already used in `LogResultsTable`'s `LogResultsRow` for severity accents (`border-l-2 border-l-[var(--bad)]` for `tone === "bad"`) and in `TraceResultsTable`'s row-level accent for an errored root span. This is a second, independent visual channel from the bar's fill color, so color-blind users or anyone scanning quickly still get the signal.
- The span's name label in that row gets a small trailing "ERROR" text tag (reusing the existing `Badge` component with `tone="bad"`, the same component/tone already used in the selected-span detail panel and in `TraceResultsTable`), so the status is legible as text, not just color.
- The bar's fill-color override for errored spans stays as-is (this design adds signals, it doesn't remove the existing one).
- No change to the legend's existing per-service color entries — an "Error" entry is not added to the legend, since the border+badge treatment is now self-describing at the row level and doesn't need a legend key to interpret (a color swatch in a legend interpreted against a bar's fill would still have the "is this the service color or the error override" ambiguity; border+text does not).

## Testing

- **Log quick filter:** component test on `LogSearch.tsx` (or the smallest testable unit around `messageFilteredLogs`) covering: substring mode matches as today; regex mode matches a pattern like `error|fail`; an invalid regex in regex mode falls back to showing all rows with the inline notice; toggling the `.*` button flips `aria-pressed` and the matching mode.
- **Span error visibility:** component test on `TraceDetail.tsx`'s waterfall rendering covering: a span with `status_code === "ERROR"` renders both the left-border accent class and the "ERROR" badge; a non-error span renders neither.

## Spec Sync

- `spec/05-frontend.md`: note the quick-filter regex capability alongside the existing logs-explorer UX requirements, and note the waterfall's per-row error indicator (border + badge) alongside the existing trace-detail description.
