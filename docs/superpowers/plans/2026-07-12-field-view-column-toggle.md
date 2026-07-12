# Field View Column Toggle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore add-as-column behavior for every displayed trace and log field, and let users remove an added column from the same field-row action.

**Architecture:** Keep visible-column state in each explorer and pass one toggle callback into its context panel. Signal-specific resolver utilities translate stable field keys into formatted per-record values, while the shared `DlRow` renders accessible add/remove actions. Component tests prove state synchronization and Playwright tests protect the complete browser interaction.

**Tech Stack:** React 19, TypeScript, Vitest, Testing Library, Playwright, lucide-react, npm.

## Global Constraints

- Work only on `fix/issue-536-field-add-column`; do not edit or remove unrelated untracked Python cache directories.
- Write and commit the failing bug tests before changing production code.
- Reuse `DlRow`, `ColumnPickerControl`, `LogResultsTable`, and `TraceResultsTable`; do not create a parallel field-view component.
- Every displayed field whose value can be resolved from a record is toggleable, including time, message, trace ID, other built-in fields, attributes, and resource attributes; there are no permanently visible field columns.
- The field action displays `+` when absent and `-` when present, with matching accessible names.
- Run `cd apps/frontend && npm run test:visual` before and after component/layout changes and inspect generated screenshots.
- Run `bash scripts/local-ci.sh` successfully before pushing code.
- Do not edit generated files, dependencies, ADRs, specs, or `docs/agent-context.md`.

---

### Task 1: Capture the regression at component and browser levels

**Files:**
- Modify: `apps/frontend/src/pages/LogSearch.test.tsx`
- Modify: `apps/frontend/src/pages/TraceSearch.test.tsx`
- Modify: `apps/frontend/e2e/navigation.spec.ts`

**Interfaces:**
- Consumes: existing `LogExplorer`, `TraceExplorer`, context panels, and results-table accessible labels.
- Produces: failing tests for `Add <field> as a column`, `Remove <field> column`, and synchronized table headers.

- [ ] **Step 1: Record the pre-change visual baseline**

Run:

```bash
cd apps/frontend
npm run test:visual
```

Expected: both Playwright files pass. Inspect `e2e/screenshots/` and record any pre-existing visual anomaly before editing.

- [ ] **Step 2: Replace the one-way log promotion assertion with a toggle regression**

In `LogSearch.test.tsx`, extend the existing promotion test so it first toggles built-in `service.name`, then arbitrary `log.http.route`, using assertions equivalent to:

```tsx
fireEvent.click(within(sidebar).getByRole("button", { name: "Add service.name as a column" }));
expect(within(table).getByRole("columnheader", { name: "service.name" })).toBeInTheDocument();
expect(within(sidebar).getByRole("button", { name: "Remove service.name column" })).toBeEnabled();
fireEvent.click(within(sidebar).getByRole("button", { name: "Remove service.name column" }));
expect(within(table).queryByRole("columnheader", { name: "service.name" })).not.toBeInTheDocument();

fireEvent.click(within(sidebar).getByRole("button", { name: "Add log.http.route as a column" }));
expect(within(table).getByRole("columnheader", { name: "log.http.route" })).toBeInTheDocument();
expect(within(table).getByText("/checkout")).toBeInTheDocument();
fireEvent.click(within(sidebar).getByRole("button", { name: "Remove log.http.route column" }));
expect(within(table).queryByRole("columnheader", { name: "log.http.route" })).not.toBeInTheDocument();
```

- [ ] **Step 3: Add the trace toggle component regression**

Extend `nlqTraceRows[0]` with representative `attributes` and `resource_attributes` data that `nlqRowToTraceResponse` preserves, then add a test that opens the trace sidebar and checks built-in `service.name` plus an arbitrary field:

```tsx
test("toggles trace fields as table columns from the context panel", async () => {
  renderTraceSearch();
  const table = await screen.findByRole("table", { name: "Trace results" });
  fireEvent.click(screen.getByText("GET /checkout"));
  const sidebar = screen.getByRole("complementary", { name: "Selected trace context" });

  fireEvent.click(within(sidebar).getByRole("button", { name: "Add service.name as a column" }));
  expect(within(table).getByRole("columnheader", { name: "service.name" })).toBeInTheDocument();
  fireEvent.click(within(sidebar).getByRole("button", { name: "Remove service.name column" }));
  expect(within(table).queryByRole("columnheader", { name: "service.name" })).not.toBeInTheDocument();

  fireEvent.click(within(sidebar).getByRole("button", { name: "Add deployment.environment as a column" }));
  expect(within(table).getByRole("columnheader", { name: "deployment.environment" })).toBeInTheDocument();
  expect(within(table).getByText("production")).toBeInTheDocument();
});
```

If NLQ list rows do not carry arbitrary span attributes, adjust the fixture and expectation to a displayed resolvable built-in field such as `operation`; do not invent data unavailable to the explorer response.

- [ ] **Step 4: Add real-browser regressions for logs and traces**

In `navigation.spec.ts`, add focused tests using the existing `mockAuth`, `/v1/nlq`, and histogram route patterns. Each test must navigate to the explorer, select the first row, click the field action, verify a table header and value, click the remove action, and verify the header disappears:

```ts
await page.getByRole("button", { name: "Add service.name as a column" }).click();
await expect(page.getByRole("columnheader", { name: "service.name" })).toBeVisible();
await page.getByRole("button", { name: "Remove service.name column" }).click();
await expect(page.getByRole("columnheader", { name: "service.name" })).toHaveCount(0);
```

Use unique mock values for each signal so the row-value assertion cannot pass against unrelated panel text.

- [ ] **Step 5: Run the focused tests and verify RED**

Run:

```bash
cd apps/frontend
npm test -- --run src/pages/LogSearch.test.tsx src/pages/TraceSearch.test.tsx
npx playwright test e2e/navigation.spec.ts --grep "field.*column"
```

Expected: component and browser tests fail because built-in log fields and all trace fields lack add buttons, and the existing action never becomes a remove action. Failures must be assertion failures about missing accessible actions/headers, not fixture or route errors.

- [ ] **Step 6: Commit the failing tests alone**

```bash
git add apps/frontend/src/pages/LogSearch.test.tsx apps/frontend/src/pages/TraceSearch.test.tsx apps/frontend/e2e/navigation.spec.ts
git commit -m "test(issue-536): reproduce broken field column toggles"
```

### Task 2: Implement shared add/remove semantics

**Files:**
- Modify: `apps/frontend/src/components/ui/dl-row.tsx`
- Modify: `apps/frontend/src/components/ui/dl-row.test.tsx`

**Interfaces:**
- Consumes: `label: string`, `onPromote?: () => void`, and `promoted?: boolean` from current callers.
- Produces: `onToggleColumn?: () => void` and `columnVisible?: boolean`, rendering `Add ${label} as a column` or `Remove ${label} column`.

- [ ] **Step 1: Update the shared component unit test for both states**

Change the test to render `DlRow` once with `columnVisible={false}` and once with `columnVisible={true}`. Assert the add button contains a lucide plus, the remove button has accessible name `Remove log.error.type column`, and each invokes `onToggleColumn` once.

- [ ] **Step 2: Implement the toggle affordance**

Replace one-way promotion props and disabled behavior with:

```tsx
onToggleColumn?: () => void;
columnVisible?: boolean;
```

Render `Plus` when false and `Minus` when true. The click handler must call `e.stopPropagation()`, invoke `onToggleColumn`, and blur the button. Keep existing hover, keyboard-focus, and focus-ring classes; the remove state stays enabled.

- [ ] **Step 3: Run the shared test**

```bash
cd apps/frontend
npm test -- --run src/components/ui/dl-row.test.tsx
```

Expected: PASS with no warnings.

### Task 3: Unify log field columns and toggling

**Files:**
- Modify: `apps/frontend/src/utils/logContext.ts`
- Modify: `apps/frontend/src/utils/logContext.test.ts`
- Modify: `apps/frontend/src/features/signals/components/LogResultsTable.tsx`
- Modify: `apps/frontend/src/features/signals/components/LogResultsTable.test.tsx`
- Modify: `apps/frontend/src/pages/LogSearch.tsx`

**Interfaces:**
- Consumes: `getLogFieldValue(log, key, format): string` and `logContextEntries(log, format): [string, string][]`.
- Produces: one ordered `visibleColumns: string[]` state and `toggleLogColumn(key: string): void`; every context-entry key is toggleable.

- [ ] **Step 1: Define stable log keys and labels**

Keep `getLogFieldValue` as the single resolver. Remove `isPromotableLogKey`; instead expose each displayed `logContextEntries` key to `DlRow`. Avoid duplicate physical columns by replacing legacy `level`/`service` identities with the context keys `severity_number`/`service.name`; represent `time` and `message` in the same state and picker logic as every other field.

- [ ] **Step 2: Make the log table render the ordered selected keys**

Replace the split `visibleColumns`/`promotedColumns` rendering path with an ordered string-key list. For each selected key render one header and one cell through `getLogFieldValue`; `time` and `message` use the same optional path. Ensure the built-in key label shown in the field panel is the header label.

- [ ] **Step 3: Wire log context toggles and picker synchronization**

Implement the state update once:

```ts
const toggleColumn = (key: string) =>
  setVisibleColumns((current) =>
    current.includes(key) ? current.filter((column) => column !== key) : [...current, key],
  );
```

Pass `onToggleColumn={() => toggleColumn(key)}` and `columnVisible={visibleColumns.includes(key)}` to every displayed `DlRow`. Generate picker entries from built-ins plus selected arbitrary fields and continue serializing the same string keys in saved log views.

- [ ] **Step 4: Run focused log tests**

```bash
cd apps/frontend
npm test -- --run src/utils/logContext.test.ts src/features/signals/components/LogResultsTable.test.tsx src/pages/LogSearch.test.tsx
```

Expected: PASS, including add/remove behavior for built-in and arbitrary fields.

### Task 4: Add trace field resolution and toggling

**Files:**
- Create: `apps/frontend/src/utils/traceContext.ts`
- Create: `apps/frontend/src/utils/traceContext.test.ts`
- Modify: `apps/frontend/src/features/signals/components/TraceResultsTable.tsx`
- Modify: `apps/frontend/src/features/signals/components/TraceResultsTable.test.tsx`
- Modify: `apps/frontend/src/pages/TraceSearch.tsx`

**Interfaces:**
- Produces: `traceContextEntries(trace: TraceResponse, format: TimeFormat): [string, string][]` and `getTraceFieldValue(trace: TraceResponse, key: string, format: TimeFormat): string`.
- Produces: `TraceTableColumn = string`, ordered `visibleColumns`, and `toggleTraceColumn(key: string): void`.

- [ ] **Step 1: Implement and test the trace resolver**

Cover the displayed keys `trace_id`, `start_time`, `service.name`, `operation`, and `duration`, plus root span attributes and resource attributes. Use `formatTimestamp` and `formatContextValue`; return an empty string when a key is missing on another trace. Add tests for every built-in mapping, one span attribute, one resource attribute, and a missing key.

- [ ] **Step 2: Render dynamic trace columns**

Refactor `TraceResultsTable` to iterate the ordered selected keys and resolve each row through `getTraceFieldValue`. When `trace_id` is selected, its cell retains the existing detail link and copy action; when removed, row selection still opens the context panel and its full-trace link remains available. Preserve selection semantics and do not introduce nested interactive controls.

- [ ] **Step 3: Wire every trace context row to column state**

Pass `visibleColumns` and `onToggleColumn` into `TraceContextSidebar`. Generate its rows from `traceContextEntries`, preserving copy buttons and the existing full-trace link. Use the same de-duplicating toggle update as logs and generate picker options from the stable built-ins plus selected arbitrary keys.

- [ ] **Step 4: Run focused trace tests**

```bash
cd apps/frontend
npm test -- --run src/utils/traceContext.test.ts src/features/signals/components/TraceResultsTable.test.tsx src/pages/TraceSearch.test.tsx
```

Expected: PASS, including add/remove behavior and per-row values.

- [ ] **Step 5: Commit the production fix**

```bash
git add apps/frontend/src/components/ui/dl-row.tsx apps/frontend/src/components/ui/dl-row.test.tsx apps/frontend/src/utils/logContext.ts apps/frontend/src/utils/logContext.test.ts apps/frontend/src/utils/traceContext.ts apps/frontend/src/utils/traceContext.test.ts apps/frontend/src/features/signals/components/LogResultsTable.tsx apps/frontend/src/features/signals/components/LogResultsTable.test.tsx apps/frontend/src/features/signals/components/TraceResultsTable.tsx apps/frontend/src/features/signals/components/TraceResultsTable.test.tsx apps/frontend/src/pages/LogSearch.tsx apps/frontend/src/pages/TraceSearch.tsx
git commit -m "fix(issue-536): restore field column toggles"
```

### Task 5: Verify browser behavior, visual output, and repository gates

**Files:**
- Modify after completion: move `docs/superpowers/plans/2026-07-12-field-view-column-toggle.md` to `archived/plans/2026-07-12-field-view-column-toggle.md`
- Modify only if linked: `docs/agent-context.md`

**Interfaces:**
- Consumes: all Task 1 regressions and Task 2-4 implementation.
- Produces: verified screenshots, passing frontend/full CI, archived completed plan, and PR-ready commits.

- [ ] **Step 1: Run the browser regressions**

```bash
cd apps/frontend
npx playwright test e2e/navigation.spec.ts --grep "field.*column"
```

Expected: both log and trace tests PASS.

- [ ] **Step 2: Run frontend verification**

```bash
cd apps/frontend
npm run typecheck
npm run lint
npm run build
npm test -- --run
```

Expected: every command exits 0 without new warnings.

- [ ] **Step 3: Run and inspect the post-change visual suite**

```bash
cd apps/frontend
npm run test:visual
```

Expected: PASS. Inspect the trace/log panel screenshots and confirm the `+`/`-` actions do not overlap values, force horizontal overflow, or disappear at keyboard focus.

- [ ] **Step 4: Run the mandatory repository gate**

From the repository root:

```bash
bash scripts/local-ci.sh
```

Expected: frontend typecheck/lint/build/test, Helm lint, Docker build, and smoke test all PASS.

- [ ] **Step 5: Archive the completed plan**

Move this file to `archived/plans/2026-07-12-field-view-column-toggle.md`. Search for active links before moving:

```bash
rg -n "2026-07-12-field-view-column-toggle" docs spec AGENTS.md
git mv docs/superpowers/plans/2026-07-12-field-view-column-toggle.md archived/plans/2026-07-12-field-view-column-toggle.md
```

Update `docs/agent-context.md` only if the search finds a link to the active plan; otherwise leave it unchanged and state in the PR that no future-agent guidance changed.

- [ ] **Step 6: Commit verification bookkeeping**

```bash
git add archived/plans/2026-07-12-field-view-column-toggle.md docs/agent-context.md
git commit -m "docs(issue-536): archive completed column toggle plan"
```

If `docs/agent-context.md` is unchanged, omit it from `git add`.

### Task 6: Publish for human review

**Files:**
- No additional source files.

**Interfaces:**
- Produces: pushed issue branch, PR closing issue #536, and review-ready issue labels.

- [ ] **Step 1: Confirm branch scope and commits**

```bash
git status --short --branch
git diff --check origin/main...HEAD
git log --oneline origin/main..HEAD
```

Expected: only the two unrelated untracked cache directories remain; commits include design, failing tests, fix, and archived plan.

- [ ] **Step 2: Push the verified branch**

```bash
git push
```

- [ ] **Step 3: Open the pull request**

Create a PR titled `fix(issue-536): restore field column toggles` with `Closes #536`, the red/green test evidence, visual-suite result, `local-ci.sh` result, and these governance statements:

- ADR/spec sync not required because this restores existing frontend behavior without changing architecture or contracts.
- Testcontainers not applicable because no backend or real dependency boundary changed; component and Playwright tests are the replacement signals.
- `docs/agent-context.md` unchanged because layout, ownership, required verification, and future-agent assumptions did not change.
- Regression gates were extended, not weakened.

- [ ] **Step 4: Mark the issue ready for review**

```bash
gh issue edit 536 --remove-label "in-progress" --add-label "ready-for-review"
```

Do not merge or approve the PR.
