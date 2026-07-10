# Log Quick Filter (Regex) and Span Error Visibility — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the logs explorer's existing message quick-filter regex-capable and clearly labeled, and make span error status visible per-row in the trace waterfall through a channel independent of fill color.

**Architecture:** Two independent, small UI-only changes: (1) `LogExplorer` in `apps/frontend/src/pages/LogSearch.tsx` gains a regex-mode toggle next to its existing message-search input, changing only the client-side filter predicate already applied to already-fetched rows; (2) `TraceDetail` in `apps/frontend/src/pages/TraceDetail.tsx`'s waterfall row rendering gains a border accent and a text badge for spans with `status_code === "ERROR"`, alongside the existing fill-color override.

**Tech Stack:** React, TypeScript, Vitest, Testing Library.

## Global Constraints

- No backend changes — both parts are frontend-only, client-side behavior over data already being fetched today.
- Do not modify `QueryFilterInput.tsx`, `queryFilters.ts`, `ShorthandHint.tsx`, or any NLQ/shorthand backend code (`llm_adapter.rs`) — the main query box and its `/`-prefix convention are out of scope.
- Do not change `TraceResultsTable.tsx`'s root-span-only Status column semantics — out of scope.
- Reuse existing UI primitives: `Badge` component (`apps/frontend/src/components/ui/badge.tsx`, `tone="bad"` for error) and the existing `border-l-2 border-l-[var(--bad)]` accent class already used in `LogResultsTable.tsx`'s `LogResultsRow` for severity accents.
- Run `cd apps/frontend && npx vitest run <path>` after each test-writing step; run `npm run typecheck` before the final commit of each task.

---

## File Map

| File | Change |
|------|--------|
| `apps/frontend/src/pages/LogSearch.tsx` | Add `isRegexMode` state, regex-aware filtering, `.*` toggle button, invalid-regex inline notice |
| `apps/frontend/src/pages/LogSearch.test.tsx` | Tests for regex-mode matching, plain-mode unaffected, invalid-regex fallback |
| `apps/frontend/src/pages/TraceDetail.tsx` | Add error border accent + "ERROR" badge to waterfall rows |
| `apps/frontend/src/pages/TraceDetail.test.tsx` | Tests for the error accent/badge appearing only on error spans |
| `spec/05-frontend.md` | Note both shipped behaviors alongside existing logs/traces UX requirements |

---

### Task 1: Log quick filter regex support

**Files:**
- Modify: `apps/frontend/src/pages/LogSearch.tsx`
- Modify: `apps/frontend/src/pages/LogSearch.test.tsx`

**Interfaces:**
- Consumes: nothing from outside this task; operates on existing `messageSearch: string` state and `formatLogMessage` (already imported in `LogSearch.tsx`).
- Produces: nothing consumed by later tasks — this task is self-contained.

- [ ] **Step 1: Write the failing tests**

Read `apps/frontend/src/pages/LogSearch.test.tsx` top-of-file fixtures first (the `logs` array defines two `LogRecord`s: one with `body: { message: "checkout completed", ... }` service `checkout`, and one with `body: "payment failed"` service `payments` — both `severity_number` differ, 9 and 17 respectively). Add these three tests after the existing `test("loading a saved view applies its severity filter and message search", ...)` test (same file, same `renderLogSearch()` helper, same `fireEvent`/`waitFor`/`screen` imports already used elsewhere in the file):

```typescript
test("plain-mode quick filter matches substrings as before", async () => {
  renderLogSearch();

  const input = await screen.findByLabelText("Search log messages");
  fireEvent.change(input, { target: { value: "failed" } });

  await waitFor(() => {
    expect(screen.getByText("payment failed")).toBeInTheDocument();
    expect(screen.queryByText("checkout completed")).not.toBeInTheDocument();
  });
});

test("regex-mode quick filter matches a pattern against log messages", async () => {
  renderLogSearch();

  fireEvent.click(await screen.findByRole("button", { name: /enable regex quick filter/i }));
  const input = screen.getByLabelText("Search log messages");
  fireEvent.change(input, { target: { value: "^payment" } });

  await waitFor(() => {
    expect(screen.getByText("payment failed")).toBeInTheDocument();
    expect(screen.queryByText("checkout completed")).not.toBeInTheDocument();
  });
});

test("invalid regex in regex mode shows all rows with an inline notice", async () => {
  renderLogSearch();

  fireEvent.click(await screen.findByRole("button", { name: /enable regex quick filter/i }));
  const input = screen.getByLabelText("Search log messages");
  fireEvent.change(input, { target: { value: "(unterminated" } });

  await waitFor(() => {
    expect(screen.getByText("payment failed")).toBeInTheDocument();
    expect(screen.getByText("checkout completed")).toBeInTheDocument();
    expect(screen.getByText("Invalid regex — showing all results.")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/frontend && npx vitest run src/pages/LogSearch.test.tsx -t "quick filter" 2>&1 | tail -40`
Expected: FAIL — no "Enable regex quick filter" button exists yet, and the plain-mode test may pass already (that behavior exists) but the other two fail on the missing toggle button.

- [ ] **Step 3: Add regex-mode state and filtering logic**

In `apps/frontend/src/pages/LogSearch.tsx`, find:

```typescript
  const [visibleColumns, setVisibleColumns] = useState<("level" | "service")[]>(["level", "service"]);
```

Add directly after it:

```typescript
  const [isRegexMode, setIsRegexMode] = useState(false);
```

Find:

```typescript
  // Apply message search first, then compute severity counts, then apply severity filter.
  const messageFilteredLogs = useMemo(() => {
    if (!messageSearch.trim()) return logs;
    const needle = messageSearch.toLowerCase();
    return logs.filter((l) => formatLogMessage(l.body).toLowerCase().includes(needle));
  }, [logs, messageSearch]);
```

Replace with:

```typescript
  const regexPattern = useMemo(() => {
    if (!isRegexMode || !messageSearch.trim()) return null;
    try {
      return new RegExp(messageSearch, "i");
    } catch {
      return undefined; // undefined marks an invalid pattern, distinct from null (no pattern requested)
    }
  }, [isRegexMode, messageSearch]);

  const isRegexInvalid = isRegexMode && messageSearch.trim() !== "" && regexPattern === undefined;

  // Apply message search first, then compute severity counts, then apply severity filter.
  const messageFilteredLogs = useMemo(() => {
    if (!messageSearch.trim()) return logs;
    if (isRegexMode) {
      if (!regexPattern) return logs; // invalid pattern: show everything rather than nothing
      return logs.filter((l) => regexPattern.test(formatLogMessage(l.body)));
    }
    const needle = messageSearch.toLowerCase();
    return logs.filter((l) => formatLogMessage(l.body).toLowerCase().includes(needle));
  }, [logs, messageSearch, isRegexMode, regexPattern]);
```

- [ ] **Step 4: Add the toggle button and invalid-regex notice**

In the same file, find the message-search `<input>` block:

```typescript
            <input
              type="search"
              value={messageSearch}
              onChange={(e) => setMessageSearch(e.target.value)}
              placeholder="Search messages…"
              aria-label="Search log messages"
              className="min-w-[180px] flex-1 border border-[var(--border)] bg-[var(--surface)] px-2.5 py-1 text-xs text-[var(--text)] placeholder:text-[var(--muted)] focus:border-[var(--brand)] focus:outline-none"
            />
```

Replace with:

```typescript
            <input
              type="search"
              value={messageSearch}
              onChange={(e) => setMessageSearch(e.target.value)}
              placeholder="Quick filter — plain text or regex"
              aria-label="Search log messages"
              className="min-w-[180px] flex-1 border border-[var(--border)] bg-[var(--surface)] px-2.5 py-1 text-xs text-[var(--text)] placeholder:text-[var(--muted)] focus:border-[var(--brand)] focus:outline-none"
            />
            <button
              type="button"
              onClick={() => setIsRegexMode((v) => !v)}
              aria-pressed={isRegexMode}
              aria-label={isRegexMode ? "Disable regex quick filter" : "Enable regex quick filter"}
              title="Toggle regex matching for the quick filter"
              className={[
                "px-2 py-1 text-xs font-mono font-bold border transition-colors",
                isRegexMode
                  ? "border-[var(--brand)] text-[var(--brand)]"
                  : "border-[var(--border)] text-[var(--muted)] hover:border-[var(--brand)] hover:text-[var(--text)]",
              ].join(" ")}
            >
              .*
            </button>
```

Then find:

```typescript
          {isLive && userQuery?.trim() && (
            <p className="text-[10px] text-[var(--warn)] px-1">
              NLQ query not applied in tail mode — service and severity filters are active.
            </p>
          )}
```

Add directly after it:

```typescript
          {isRegexInvalid && (
            <p className="text-[10px] text-[var(--warn)] px-1">
              Invalid regex — showing all results.
            </p>
          )}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd apps/frontend && npx vitest run src/pages/LogSearch.test.tsx 2>&1 | tail -60`
Expected: all tests in the file pass, including the 3 new ones.

- [ ] **Step 6: Typecheck and commit**

```bash
cd apps/frontend && npm run typecheck 2>&1 | tail -30
git add apps/frontend/src/pages/LogSearch.tsx apps/frontend/src/pages/LogSearch.test.tsx
git commit -m "feat(frontend): add regex mode to logs quick filter"
```

---

### Task 2: Span error visibility in the trace waterfall

**Files:**
- Modify: `apps/frontend/src/pages/TraceDetail.tsx`
- Modify: `apps/frontend/src/pages/TraceDetail.test.tsx`

**Interfaces:**
- Consumes: `Badge` from `apps/frontend/src/components/ui/badge.tsx` (already imported in `TraceDetail.tsx` — verify the import exists; if not, add `import { Badge } from "../components/ui/badge";`).
- Produces: nothing consumed by later tasks — this task is self-contained.

- [ ] **Step 1: Write the failing tests**

Add to `apps/frontend/src/pages/TraceDetail.test.tsx`, after the existing `test("Errors MetricCard is present when there are error spans", ...)` test (same file, same `baseSpan` fixture, same `wrapper` helper):

```typescript
test("waterfall row for an error span has the error border accent and an ERROR badge", () => {
  const errorSpan: Span = { ...baseSpan, span_id: "222", status_code: "ERROR" };
  render(<TraceDetail traceId="abc" spans={[baseSpan, errorSpan]} />, { wrapper });

  // baseSpan and errorSpan share the same service/operation text, so scope the
  // assertion to the row containing the ERROR badge rather than by row text.
  const errorBadges = screen.getAllByText("ERROR");
  expect(errorBadges.length).toBeGreaterThanOrEqual(1);
  const errorRow = errorBadges[0].closest('[role="button"]');
  expect(errorRow).not.toBeNull();
  expect(errorRow).toHaveClass("border-l-[var(--bad)]");
});

test("waterfall row for a non-error span has no error border accent or ERROR badge", () => {
  render(<TraceDetail traceId="abc" spans={[baseSpan]} />, { wrapper });

  const row = screen.getByRole("button", { name: /POST \/order/ });
  expect(row).not.toHaveClass("border-l-[var(--bad)]");
  expect(screen.queryByText("ERROR")).not.toBeInTheDocument();
});
```

Note: the first test references `errorLabel` but does not use it beyond computing it for clarity during debugging — remove that unused line if your linter flags it; the actual assertions are on `errorBadges`/`errorRow`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/frontend && npx vitest run src/pages/TraceDetail.test.tsx -t "waterfall row" 2>&1 | tail -40`
Expected: FAIL — no "ERROR" badge text and no `border-l-[var(--bad)]` class exist on waterfall rows yet.

- [ ] **Step 3: Add the border accent and badge**

In `apps/frontend/src/pages/TraceDetail.tsx`, find the waterfall row rendering (inside the `spans.map((span) => { ... })` block):

```typescript
                return (
                  <div
                    key={span.span_id}
                    role="button"
                    tabIndex={0}
                    onClick={() =>
                      setSelectedSpanId(isSelected ? undefined : span.span_id)
                    }
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        setSelectedSpanId(isSelected ? undefined : span.span_id);
                      }
                    }}
                    className={`flex items-center mb-1 cursor-pointer px-0 py-0.5 ${
                      isSelected ? "bg-[var(--surface-subtle)]" : "bg-transparent"
                    }`}
                  >
                    <span
                      className="w-[200px] overflow-hidden text-ellipsis whitespace-nowrap text-xs shrink-0"
                      style={{ paddingLeft: `${depth * 12}px` }}
                    >
                      {span.service_name}: {span.operation_name}
                      <span className="ml-1 text-[10px] text-[var(--muted)] font-mono">
                        [{span.span_kind}]
                      </span>
                    </span>
```

Replace with:

```typescript
                const isErrorSpan = span.status_code === "ERROR";
                return (
                  <div
                    key={span.span_id}
                    role="button"
                    tabIndex={0}
                    onClick={() =>
                      setSelectedSpanId(isSelected ? undefined : span.span_id)
                    }
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        setSelectedSpanId(isSelected ? undefined : span.span_id);
                      }
                    }}
                    className={`flex items-center mb-1 cursor-pointer px-0 py-0.5 ${
                      isSelected ? "bg-[var(--surface-subtle)]" : "bg-transparent"
                    } ${isErrorSpan ? "border-l-2 border-l-[var(--bad)]" : ""}`}
                  >
                    <span
                      className="w-[200px] overflow-hidden text-ellipsis whitespace-nowrap text-xs shrink-0"
                      style={{ paddingLeft: `${depth * 12}px` }}
                    >
                      {span.service_name}: {span.operation_name}
                      <span className="ml-1 text-[10px] text-[var(--muted)] font-mono">
                        [{span.span_kind}]
                      </span>
                      {isErrorSpan && (
                        <Badge tone="bad" className="ml-1.5">
                          ERROR
                        </Badge>
                      )}
                    </span>
```

Note: `const isErrorSpan = span.status_code === "ERROR";` replaces the need to declare `color` separately for this purpose, but the existing `color` variable (used for the bar's `background`) stays as-is — do not remove it, it's still needed a few lines below for the bar's fill style. Just add the new `isErrorSpan` line alongside it (both can reference `span.status_code === "ERROR"` independently; do not try to merge them into one variable to avoid touching the existing `color` logic).

- [ ] **Step 4: Verify the `Badge` import exists**

Check the top of `apps/frontend/src/pages/TraceDetail.tsx` for `import { Badge } from "../components/ui/badge";`. If it's missing, add it near the other UI component imports.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd apps/frontend && npx vitest run src/pages/TraceDetail.test.tsx 2>&1 | tail -60`
Expected: all tests pass, including the 2 new ones.

- [ ] **Step 6: Typecheck and commit**

```bash
cd apps/frontend && npm run typecheck 2>&1 | tail -30
git add apps/frontend/src/pages/TraceDetail.tsx apps/frontend/src/pages/TraceDetail.test.tsx
git commit -m "feat(frontend): show per-row error indicator in trace waterfall"
```

---

### Task 3: Spec sync

**Files:**
- Modify: `spec/05-frontend.md`

**Interfaces:**
- Consumes: nothing (documentation only)
- Produces: nothing (documentation only)

- [ ] **Step 1: Update the logs explorer UX section**

In `spec/05-frontend.md`, find the existing description of the logs explorer's message search (the same area referenced by the Saved Views design work, near where "Saved views" and quick-filter-adjacent behavior is documented). Add a note documenting the regex mode:

Find the sentence describing the log message search box (search for "Search log messages" or the surrounding paragraph describing severity pills and message filtering in the Logs Explorer section). Add, directly after that description:

```markdown
The quick filter supports a regex mode (toggled via the `.*` button next to the input) for pattern matching against log messages, in addition to the default plain-substring mode. This is a client-side filter over already-loaded rows, distinct from the NLQ/shorthand query box above it.
```

- [ ] **Step 2: Update the trace waterfall UX section**

Find the existing description of the trace detail waterfall (search for "Waterfall" or the section describing span rendering in the Trace Detail page). Add, directly after that description:

```markdown
Each waterfall row shows an explicit "ERROR" badge and a red left-border accent when the span's `status_code` is `ERROR`, independent of the row's service-color-coded bar fill — so error status is legible without relying on color alone.
```

- [ ] **Step 3: Commit**

```bash
git add spec/05-frontend.md
git commit -m "docs(spec): document logs quick-filter regex mode and waterfall error indicator"
```

---

## Self-Review

**Spec coverage** (against `docs/superpowers/specs/2026-07-03-log-quick-filter-and-span-error-visibility-design.md`):
- Quick filter relabeled + regex toggle + invalid-pattern fallback → Task 1 ✓
- No change to main NLQ/shorthand query box → correctly excluded from Task 1 ✓
- Waterfall border accent + text badge, fill-color override retained → Task 2 ✓
- No change to `TraceResultsTable`'s Status column semantics → correctly excluded from Task 2 ✓
- Spec/doc sync → Task 3 ✓

**Placeholder scan:** No TBD/TODO markers. All code blocks are complete, including exact class names and test assertions.

**Type consistency:** `isRegexMode`/`regexPattern`/`isRegexInvalid` in Task 1 are all local to `LogExplorer` and don't cross into Task 2. `isErrorSpan` in Task 2 is local to the waterfall's `.map()` callback and doesn't conflict with the pre-existing `color` variable it sits alongside. Both tasks are independent — no shared interfaces, so no cross-task type-consistency risk.
