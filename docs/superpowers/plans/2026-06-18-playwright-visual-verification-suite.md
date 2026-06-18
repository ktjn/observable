# Playwright Visual Verification Suite — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a runnable Playwright suite that screenshots every major page and navigation flow with mocked data, fix the panel overflow when a span or log is selected, and document when to run the suite.

**Architecture:** Three concerns — bug fix (panel overflow), test coverage (visual snapshots + navigation), and process (AGENTS.md runbook). The tests live in `apps/frontend/e2e/` alongside the existing functional specs, use the same mock patterns, and are runnable with a single `npm run test:visual` command.

**Tech Stack:** Playwright (already installed), Vite dev server (auto-started by Playwright config), Tailwind CSS (for the overflow fix), TypeScript.

## Global Constraints

- Tests use `page.route()` mocks only — no live backend required.
- Mock data must be structurally complete (all required fields present) — the existing specs in `e2e/` are the reference.
- Screenshots written to `apps/frontend/e2e/screenshots/` (already gitignored — confirm or add to `.gitignore`).
- Overflow fix must not change the visual appearance of the panels on non-overflow content.
- All changes inside `apps/frontend/`.
- Run `npm run typecheck` after every TypeScript edit.

---

## Layout Context (read before touching CSS)

The page layout chain is:

```
.app-shell { overflow: hidden }
  .workspace { overflow: hidden; display: grid; grid-template-rows: auto 1fr }
    .topbar (auto row, ~44px tall)
    .content-shell (1fr row) { overflow-y: auto; padding: 16px }
      <Outlet /> → page content rendered in .page-stack { display: grid; gap: 12px }
```

Key implication: `.content-shell` is the one scrolling element. Any `h-full` inside a page resolves to `auto` because `.page-stack` has no fixed height. This means panel components must use `max-h-[calc(100vh-NNpx)]` (viewport-relative cap) rather than `h-full` to constrain their scroll region. The correct offset is `topbar(44px) + content-shell-padding(32px) + panel-card-header(~44px) ≈ 120px`.

---

## File Map

| File | Role |
|---|---|
| `src/pages/LogSearch.tsx:394–460` | `LogContextSidebar` — fix `h-full` → `max-h` |
| `src/pages/TraceDetail.tsx:159–163` | `SpanContextPanel` aside — fix `max-h` offset |
| `src/components/shared/SignalExplorer.tsx:126` | Log panel host container — fix `h-full` |
| `e2e/visual.spec.ts` | Full-page screenshots, all main routes |
| `e2e/navigation.spec.ts` | Button-click navigation flows with screenshots |
| `e2e/screenshots/` | Output directory (already exists from prior work) |
| `package.json` (frontend) | Add `test:visual` npm script |
| `AGENTS.md` (repo root) | Add runbook section |

---

## Task 1: Fix log panel overflow (`LogContextSidebar`)

**Files:**
- Modify: `apps/frontend/src/pages/LogSearch.tsx:407–411`
- Modify: `apps/frontend/src/components/shared/SignalExplorer.tsx:125–128`

**Root cause:** `LogContextSidebar` uses `h-full` on the `<aside>` for wide screens. Because `.content-shell` (not the page) is the scroll root, `h-full` resolves to `auto` inside `.page-stack`, so `overflow-y-auto` never activates — the sidebar just grows and pushes the page taller.

Secondary: The host container in `SignalExplorer` also has `h-full` which compounds the problem.

- [ ] **Step 1: Write a failing Playwright test to document the overflow**

In `apps/frontend/e2e/navigation.spec.ts`, add this test (inside a new `describe("panel overflow")`):

```typescript
import { test, expect } from "@playwright/test";

// shared helpers already defined at the top of the file — the MOCK_USER,
// mockAuth, and T_NS constants are already declared there.

test.describe("panel overflow (regression)", () => {
  test("log context panel stays within viewport when tall", async ({ page }) => {
    await mockAuth(page);
    // Mock a log with many resource attributes to produce a tall panel
    const manyAttrs: Record<string, string> = {};
    for (let i = 0; i < 30; i++) manyAttrs[`resource.key.${i}`] = `value-${i}`;
    await page.route("**/v1/nlq", (route) =>
      route.fulfill({
        json: {
          type: "frame",
          frame: {
            data: [
              {
                log_id: "log-overflow",
                timestamp_unix_nano: T_NS,
                observed_timestamp_unix_nano: T_NS,
                severity_number: 9,
                body: "test log",
                service_name: "checkout",
                environment: "prod",
                host_id: "h1",
                trace_id: null,
                span_id: null,
                fingerprint: null,
                attributes: {},
                resource_attributes: manyAttrs,
              },
            ],
          },
        },
      })
    );
    await page.route("**/v1/tenants/**/logs/histogram**", (route) =>
      route.fulfill({ json: { buckets: [] } })
    );
    await page.goto("/logs");
    await page.waitForSelector('[aria-label="Log results"]');
    await page.locator('[aria-label="Log results"] tbody tr').first().click();
    await page.waitForSelector('[aria-label="Selected log context"]');
    // The aside must not extend beyond the viewport — scrollHeight of the panel
    // should be <= the viewport height (any overflow must be inside the panel's
    // own scroll, not the page scroll).
    const pageScrollHeight = await page.evaluate(() => document.documentElement.scrollHeight);
    const viewportHeight = await page.evaluate(() => window.innerHeight);
    expect(pageScrollHeight).toBeLessThanOrEqual(viewportHeight + 20); // 20px tolerance
    await page.screenshot({ path: "e2e/screenshots/panel-log-overflow-BEFORE.png", fullPage: true });
  });
});
```

- [ ] **Step 2: Run the test to confirm it fails (documents the bug)**

```bash
cd apps/frontend
npx playwright test e2e/navigation.spec.ts --grep "log context panel" --reporter=list
```

Expected: FAIL — `pageScrollHeight` will be much greater than `viewportHeight` because the sidebar pushes the page down.

- [ ] **Step 3: Fix `SignalExplorer.tsx` — remove `h-full` from the panel host container**

In `apps/frontend/src/components/shared/SignalExplorer.tsx`, change line 126:

```tsx
// Before
<div className="w-1/4 shrink-0 h-full max-[900px]:h-auto">

// After
<div className="w-1/4 shrink-0 min-h-0 max-[900px]:w-full">
```

The panel now takes its height from its own content; the `aside` inside controls scroll.

- [ ] **Step 4: Fix `LogSearch.tsx` — swap `h-full` for a viewport-capped `max-h`**

In `apps/frontend/src/pages/LogSearch.tsx`, change the `<aside>` class in `LogContextSidebar` (line ~410):

```tsx
// Before
className="w-full h-full max-[900px]:max-h-[calc(100vh-200px)] overflow-y-auto border border-[var(--border)] bg-[var(--surface)] p-4"

// After
className="w-full max-h-[calc(100vh-120px)] overflow-y-auto border border-[var(--border)] bg-[var(--surface)] p-4"
```

The `120px` offset accounts for: topbar (44px) + content-shell top-padding (16px) + bottom-padding (16px) + Panel card header (44px). Use `max-h` (not `h`) so short logs don't have dead space. The single breakpoint value now applies to both narrow and wide screens.

- [ ] **Step 5: Typecheck**

```bash
cd apps/frontend
npm run typecheck
```

Expected: zero errors.

- [ ] **Step 6: Run the regression test — confirm it passes**

```bash
cd apps/frontend
npx playwright test e2e/navigation.spec.ts --grep "log context panel" --reporter=list
```

Expected: PASS. Check `e2e/screenshots/panel-log-overflow-BEFORE.png` is gone; re-run produces a correctly-constrained screenshot named `panel-log-overflow-fixed.png` (rename the path in the test after confirming).

- [ ] **Step 7: Commit**

```bash
git add apps/frontend/src/components/shared/SignalExplorer.tsx \
        apps/frontend/src/pages/LogSearch.tsx \
        apps/frontend/e2e/navigation.spec.ts
git commit -m "fix(ui): constrain log context sidebar height to prevent page overflow"
```

---

## Task 2: Fix span panel overflow (`SpanContextPanel`)

**Files:**
- Modify: `apps/frontend/src/pages/TraceDetail.tsx:159–163`

**Root cause:** `SpanContextPanel` has `max-h-[calc(100vh-80px)]` but the offset only accounts for ~80px when the real consumed height above the panel is ~120px (topbar 44 + padding 32 + trace summary cards ~44). This causes the panel to still push the page taller than the viewport before scroll kicks in. Additionally, the panel container uses `flex items-start` — if the span panel exceeds the waterfall's height, the Panel card grows with it.

- [ ] **Step 1: Write the failing test**

Add to the `"panel overflow (regression)"` describe block in `e2e/navigation.spec.ts`:

```typescript
test("span context panel stays within viewport when tall", async ({ page }) => {
  await mockAuth(page);
  const manyAttrs: Record<string, string> = {};
  for (let i = 0; i < 30; i++) manyAttrs[`span.attr.${i}`] = `value-${i}`;
  const resourceAttrs: Record<string, string> = {};
  for (let i = 0; i < 10; i++) resourceAttrs[`resource.${i}`] = `val-${i}`;

  await page.route(`**/v1/traces/${TRACE_ID}`, (route) =>
    route.fulfill({
      json: {
        trace_id: TRACE_ID,
        spans: [
          {
            tenant_id: "00000000-0000-0000-0000-000000000001",
            trace_id: TRACE_ID,
            span_id: "span001",
            parent_span_id: null,
            service_name: "checkout",
            operation_name: "GET /order",
            start_time_unix_nano: 1_000_000_000,
            end_time_unix_nano: 6_000_000_000,
            duration_ns: 5_000_000_000,
            status_code: "OK",
            span_kind: "SERVER",
            service_version: "v1.0",
            attributes: manyAttrs,
            resource_attributes: resourceAttrs,
          },
        ],
      },
    })
  );
  await page.route("**/v1/logs**", (route) =>
    route.fulfill({ json: { logs: [], total: 0, facets: {} } })
  );

  await page.goto(`/traces/${TRACE_ID}`);
  await page.waitForSelector("text=GET /order");
  await page.locator('[role="button"]', { hasText: /checkout.*GET/ }).click();
  await page.waitForSelector('[aria-label="Selected span context"]');

  const pageScrollHeight = await page.evaluate(() => document.documentElement.scrollHeight);
  const viewportHeight = await page.evaluate(() => window.innerHeight);
  expect(pageScrollHeight).toBeLessThanOrEqual(viewportHeight + 20);
  await page.screenshot({ path: "e2e/screenshots/panel-span-overflow-BEFORE.png", fullPage: true });
});
```

- [ ] **Step 2: Run the test to confirm it fails**

```bash
cd apps/frontend
npx playwright test e2e/navigation.spec.ts --grep "span context panel" --reporter=list
```

Expected: FAIL — page scroll height will exceed viewport height.

- [ ] **Step 3: Fix `TraceDetail.tsx` — correct the `max-h` offset and pin with `self-start`**

In `apps/frontend/src/pages/TraceDetail.tsx`, change the `<aside>` class in `SpanContextPanel` (line ~162):

```tsx
// Before
className="w-[320px] shrink-0 border border-[var(--border)] bg-[var(--surface)] p-4 max-[900px]:w-full max-h-[calc(100vh-80px)] overflow-y-auto"

// After
className="w-[320px] shrink-0 self-start border border-[var(--border)] bg-[var(--surface)] p-4 max-[900px]:w-full max-h-[calc(100vh-200px)] overflow-y-auto"
```

Two changes:
- `self-start` prevents the aside from stretching to match the waterfall column height in `items-stretch` context — it stays as tall as its content (up to the cap).
- `max-h-[calc(100vh-200px)]` uses a larger offset (`200px` = topbar 44 + padding 32 + trace header block 80 + card header 44) to cap the panel well within the viewport.

- [ ] **Step 4: Typecheck**

```bash
cd apps/frontend
npm run typecheck
```

Expected: zero errors.

- [ ] **Step 5: Run the span overflow test**

```bash
cd apps/frontend
npx playwright test e2e/navigation.spec.ts --grep "span context panel" --reporter=list
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/pages/TraceDetail.tsx \
        apps/frontend/e2e/navigation.spec.ts
git commit -m "fix(ui): constrain span context panel height to prevent page overflow"
```

---

## Task 3: Add panel-open visual tests to `navigation.spec.ts`

**Files:**
- Modify: `apps/frontend/e2e/navigation.spec.ts`

These are the "golden path" screenshots with a panel open — confirms the layout looks correct after the overflow fix, and will catch regressions on future changes.

- [ ] **Step 1: Add log-panel-open screenshot test**

Add a new describe block in `navigation.spec.ts` after the overflow tests:

```typescript
test.describe("panel screenshots (post-fix baselines)", () => {
  test("log context panel open — screenshot", async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/nlq", (route) =>
      route.fulfill({
        json: {
          type: "frame",
          frame: {
            data: [
              {
                log_id: "log-panel-shot",
                timestamp_unix_nano: T_NS,
                observed_timestamp_unix_nano: T_NS,
                severity_number: 17,
                body: "checkout failed: timeout after 30s connecting to payments",
                service_name: "checkout",
                environment: "prod",
                host_id: "prod-host-1",
                trace_id: "abc123def456",
                span_id: "span001",
                fingerprint: null,
                attributes: { "error.type": "TimeoutError", "http.method": "POST" },
                resource_attributes: {
                  "k8s.pod.name": "checkout-6d8f9-xkpqr",
                  "k8s.namespace.name": "production",
                  "k8s.node.name": "prod-node-1",
                  "service.version": "v1.4.2",
                },
              },
            ],
          },
        },
      })
    );
    await page.route("**/v1/tenants/**/logs/histogram**", (route) =>
      route.fulfill({ json: { buckets: [] } })
    );
    await page.goto("/logs");
    await page.waitForSelector('[aria-label="Log results"]');
    await page.locator('[aria-label="Log results"] tbody tr').first().click();
    await page.waitForSelector('[aria-label="Selected log context"]');
    await page.screenshot({ path: "e2e/screenshots/panel-log-open.png", fullPage: true });
  });

  test("span context panel open — screenshot", async ({ page }) => {
    await mockAuth(page);
    await page.route(`**/v1/traces/${TRACE_ID}`, (route) =>
      route.fulfill({
        json: {
          trace_id: TRACE_ID,
          spans: [
            {
              tenant_id: "00000000-0000-0000-0000-000000000001",
              trace_id: TRACE_ID,
              span_id: "span001",
              parent_span_id: null,
              service_name: "checkout",
              operation_name: "GET /order",
              start_time_unix_nano: 1_000_000_000,
              end_time_unix_nano: 6_000_000_000,
              duration_ns: 5_000_000_000,
              status_code: "ERROR",
              span_kind: "SERVER",
              service_version: "v1.4.2",
              attributes: {
                "http.method": "GET",
                "http.url": "https://api.internal/order/123",
                "http.status_code": 500,
                "error.message": "upstream timeout",
              },
              resource_attributes: {
                "k8s.pod.name": "checkout-6d8f9-xkpqr",
                "k8s.namespace.name": "production",
                "service.version": "v1.4.2",
              },
            },
            {
              tenant_id: "00000000-0000-0000-0000-000000000001",
              trace_id: TRACE_ID,
              span_id: "span002",
              parent_span_id: "span001",
              service_name: "payments",
              operation_name: "POST /charge",
              start_time_unix_nano: 2_000_000_000,
              end_time_unix_nano: 5_000_000_000,
              duration_ns: 3_000_000_000,
              status_code: "OK",
              span_kind: "CLIENT",
              service_version: null,
              attributes: {},
              resource_attributes: {},
            },
          ],
        },
      })
    );
    await page.route("**/v1/logs**", (route) =>
      route.fulfill({ json: { logs: [], total: 0, facets: {} } })
    );
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=GET /order");
    await page.locator('[role="button"]', { hasText: /checkout.*GET/ }).click();
    await page.waitForSelector('[aria-label="Selected span context"]');
    await page.screenshot({ path: "e2e/screenshots/panel-span-open.png", fullPage: true });
  });
});
```

- [ ] **Step 2: Run both panel screenshot tests**

```bash
cd apps/frontend
npx playwright test e2e/navigation.spec.ts --grep "panel screenshots" --reporter=list
```

Expected: 2 passed. Review `e2e/screenshots/panel-log-open.png` and `e2e/screenshots/panel-span-open.png` — the panel should be visible alongside the table/waterfall without the page being taller than the viewport.

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/e2e/navigation.spec.ts
git commit -m "test(e2e): add panel-open visual screenshots (post overflow-fix baselines)"
```

---

## Task 4: Add `test:visual` npm script and verify full suite runs

**Files:**
- Modify: `apps/frontend/package.json`

- [ ] **Step 1: Add the script**

In `apps/frontend/package.json`, in the `"scripts"` block, add:

```json
"test:visual": "playwright test e2e/visual.spec.ts e2e/navigation.spec.ts --reporter=list"
```

The full `scripts` block after the change:

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "typecheck": "tsc --noEmit",
    "lint": "eslint src",
    "test": "vitest",
    "test:a11y": "playwright test",
    "test:visual": "playwright test e2e/visual.spec.ts e2e/navigation.spec.ts --reporter=list"
  }
}
```

- [ ] **Step 2: Run the full visual suite**

```bash
cd apps/frontend
npm run test:visual
```

Expected output: all tests listed, all pass. Count as of plan writing: `visual.spec.ts` has 6 tests, `navigation.spec.ts` has ~17 tests (13 original + 2 overflow regressions + 2 panel screenshots). Total: ~25 tests. Any failures must be fixed before proceeding.

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/package.json
git commit -m "chore(frontend): add test:visual script for Playwright visual suite"
```

---

## Task 5: Add runbook to `AGENTS.md`

**Files:**
- Modify: `AGENTS.md` (repo root)

- [ ] **Step 1: Read the current AGENTS.md to find the right insertion point**

```bash
grep -n "test\|Test\|playwright\|Playwright\|visual\|UI" AGENTS.md | head -20
```

Find the section that describes testing conventions or the development workflow. The new section goes there (or at the end of the Development section).

- [ ] **Step 2: Add the visual verification runbook**

Insert the following into `AGENTS.md` in the appropriate section (after existing test instructions, or as a new `### UI Visual Verification` subsection):

```markdown
### UI Visual Verification

Run the visual suite **before and after any change that touches layout, CSS classes, component structure, or page-level routing**. It is not required for backend-only or documentation changes.

**Run:**
```bash
cd apps/frontend
npm run test:visual
```

This runs two spec files:
- `e2e/visual.spec.ts` — full-page screenshots of every main route (Traces, Logs, Services, Infrastructure, Alerts, Dashboards) with mocked data. No backend needed.
- `e2e/navigation.spec.ts` — clicks sidebar links, row drilldowns (trace → detail, service → detail), view toggles (List ↔ Topology), tab switches (Alerts tabs), and panel-open states. Includes overflow regression tests.

Screenshots are written to `apps/frontend/e2e/screenshots/`. Review them visually — the suite will pass even if the UI looks wrong, because it's not doing pixel-diff comparison. Use your eyes on the output images.

**When to update the tests:**
- New page or route added → add a test to `visual.spec.ts` following the existing pattern (mock auth + page data, `waitForSelector` on a stable landmark, `screenshot()`).
- New navigation flow or interactive widget added → add a test to `navigation.spec.ts`.
- New context panel or slide-over added → add an overflow regression test and a panel-open screenshot test to the `"panel overflow (regression)"` and `"panel screenshots"` describe blocks.

**Mock data pattern:**
All tests use `page.route()` to intercept API calls. Copy the mock structure from an existing test in the same spec file — all fixtures are at the top of each file. The auth mock (`mockAuth`) is a shared helper declared at the top of each spec. Mocked endpoints must match the glob patterns used by the real API (e.g. `**/v1/nlq`, `**/v1/tenants/**/logs/histogram**`).
```

- [ ] **Step 3: Confirm the runbook reads correctly**

```bash
grep -A 40 "UI Visual Verification" AGENTS.md
```

Expected: the section appears with correct formatting.

- [ ] **Step 4: Commit**

```bash
git add AGENTS.md
git commit -m "docs(agents): add UI visual verification runbook for Playwright suite"
```

---

## Self-Review

**Spec coverage check:**

| Requirement | Task |
|---|---|
| Fix overflow when selecting a log | Task 1 |
| Fix overflow when selecting a span | Task 2 |
| Screenshot test of log panel open | Task 3 |
| Screenshot test of span panel open | Task 3 |
| Overflow regression tests (non-regress on fix) | Tasks 1 & 2 |
| `test:visual` single command to run suite | Task 4 |
| AGENTS.md — when to run | Task 5 |
| AGENTS.md — how to add new tests | Task 5 |

**Placeholder scan:** No TBDs, no "similar to Task N", all code blocks complete.

**Type consistency:** `TRACE_ID`, `T_NS`, and `mockAuth` are used across all new tests — they are already declared at the top of `navigation.spec.ts` and must not be re-declared in new test blocks.

**One gap noted:** `visual.spec.ts` does not yet have a trace-detail screenshot (it only covers list pages). The `navigation.spec.ts` trace drilldown test does produce `nav-trace-detail.png` — that is sufficient coverage for now. A dedicated `visual.spec.ts` trace-detail entry can be added later if wanted.
