# P3-S6e Accessibility Regression Coverage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `@playwright/test` + `@axe-core/playwright` accessibility regression coverage for the trace detail waterfall and log search, wired into `local-ci.sh` with a graceful skip when Chromium is not installed.

**Architecture:** Playwright lives entirely in `apps/frontend` (config + `e2e/` test directory). Tests launch the vite dev server via `webServer`, intercept API calls with `page.route()`, and run `AxeBuilder` scans after navigating to each view. Minimal `role`/`tabIndex`/`onKeyDown` semantic fixes are applied to the two components whose interactive `<div>` and `<li>` rows violate axe rules.

**Tech Stack:** `@playwright/test@^1.59.1`, `@axe-core/playwright@^4.11.2`, Chromium, Vite 8 dev server, `page.route()` for API mocking.

---

### File Map

| Action | Path | Purpose |
|---|---|---|
| Modify | `apps/frontend/package.json` | Add deps + `test:a11y` script |
| Create | `apps/frontend/playwright.config.ts` | Playwright config (Chromium, vite dev server, `e2e/` dir) |
| Create | `apps/frontend/e2e/accessibility.spec.ts` | All accessibility tests |
| Modify | `apps/frontend/src/pages/TraceDetail.tsx` | Add `role="button"` + `tabIndex` + `onKeyDown` to span rows |
| Modify | `apps/frontend/src/components/FacetSidebar.tsx` | Add `role="button"` + `tabIndex` + `onKeyDown` to facet `<li>` items |
| Modify | `scripts/local-ci.sh` | Add a11y step (graceful skip) |
| Modify | `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md` | Mark P3-S6e complete |

---

### Task 1: Install dependencies and configure Playwright

**Files:**
- Modify: `apps/frontend/package.json`
- Create: `apps/frontend/playwright.config.ts`

- [ ] **Step 1: Add Playwright and axe-core to devDependencies**

Open `apps/frontend/package.json`. Add two entries to `devDependencies` and one new script, producing this shape (keep all existing entries, only add the highlighted lines):

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "typecheck": "tsc --noEmit",
    "lint": "eslint src",
    "test": "vitest",
    "test:a11y": "playwright test"
  },
  "devDependencies": {
    "@axe-core/playwright": "^4.11.2",
    "@playwright/test": "^1.59.1",
    "@tailwindcss/vite": "^4.1.14",
    "@eslint/js": "^10.0.1",
    "@testing-library/jest-dom": "^6.0.0",
    "@testing-library/react": "^16.0.0",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@typescript-eslint/eslint-plugin": "^8.0.0",
    "@typescript-eslint/parser": "^8.0.0",
    "@vitejs/plugin-react": "^6.0.1",
    "@vitest/ui": "^4.1.4",
    "eslint": "^10.2.1",
    "jsdom": "^29.0.2",
    "tailwindcss": "^4.1.14",
    "typescript": "^6.0.3",
    "vite": "^8.0.8",
    "vitest": "^4.1.4"
  }
}
```

- [ ] **Step 2: Install the new packages**

Run:

```bash
npm install --workspace=apps/frontend
```

Expected:

```text
lockfile updated, @playwright/test and @axe-core/playwright installed
```

- [ ] **Step 3: Create `apps/frontend/playwright.config.ts`**

Create the file with this exact content:

```ts
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  retries: 0,
  reporter: "list",
  use: {
    baseURL: "http://localhost:5173",
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: {
    command: "npm run dev",
    url: "http://localhost:5173",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
```

- [ ] **Step 4: Verify Playwright is reachable**

Run:

```bash
npm run test:a11y --workspace=apps/frontend -- --list
```

Expected:

```text
No tests found (the e2e/ directory does not exist yet) — exit 0 or a "no tests found" message, not a crash
```

- [ ] **Step 5: Commit the setup**

Run:

```bash
git add apps/frontend/package.json package-lock.json apps/frontend/playwright.config.ts
git commit -m "feat(frontend): add playwright and axe-core foundation for a11y tests"
```

---

### Task 2: Write accessibility tests (expect failures)

**Files:**
- Create: `apps/frontend/e2e/accessibility.spec.ts`

- [ ] **Step 1: Create the `e2e/` directory and write the failing spec**

Create `apps/frontend/e2e/accessibility.spec.ts` with this content:

```ts
import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

// ── Fixtures ──────────────────────────────────────────────────────────────────

const TRACE_ID = "aaaa000000000000000000000000001a";

const FIXTURE_TRACE = {
  trace_id: TRACE_ID,
  spans: [
    {
      tenant_id: "00000000-0000-0000-0000-000000000001",
      trace_id: TRACE_ID,
      span_id: "span00000000001a",
      service_name: "checkout",
      operation_name: "POST /order",
      start_time_unix_nano: 1_000_000_000,
      end_time_unix_nano: 6_000_000_000,
      duration_ns: 5_000_000_000,
      status_code: "OK",
    },
    {
      tenant_id: "00000000-0000-0000-0000-000000000001",
      trace_id: TRACE_ID,
      span_id: "span00000000002a",
      service_name: "payments",
      operation_name: "POST /charge",
      start_time_unix_nano: 2_000_000_000,
      end_time_unix_nano: 5_000_000_000,
      duration_ns: 3_000_000_000,
      status_code: "OK",
    },
  ],
};

const EMPTY_LOGS = { logs: [], total: 0, facets: {} };

const FIXTURE_LOGS = {
  logs: [
    {
      tenant_id: "00000000-0000-0000-0000-000000000001",
      log_id: "log-0001",
      timestamp_unix_nano: "1700000000000000000",
      severity_number: 9,
      severity_text: "INFO",
      body: "order received",
      service_name: "checkout",
      resource_attributes: {},
    },
  ],
  total: 1,
  facets: {
    service_name: [{ value: "checkout", count: 1 }],
  },
};

// ── Trace detail waterfall ────────────────────────────────────────────────────

test.describe("trace detail waterfall", () => {
  test.beforeEach(async ({ page }) => {
    await page.route(`**/v1/traces/${TRACE_ID}`, (route) =>
      route.fulfill({ json: FIXTURE_TRACE })
    );
    await page.route("**/v1/logs**", (route) =>
      route.fulfill({ json: EMPTY_LOGS })
    );
  });

  test("has no axe violations", async ({ page }) => {
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=POST /order");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});

// ── Log search ────────────────────────────────────────────────────────────────

test.describe("log search", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/v1/logs**", (route) =>
      route.fulfill({ json: FIXTURE_LOGS })
    );
  });

  test("has no axe violations", async ({ page }) => {
    await page.goto("/logs");
    await page.waitForSelector("text=order received");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});

// ── Negative proof ────────────────────────────────────────────────────────────

test("detects injected violation (harness proof)", async ({ page }) => {
  await page.route("**/v1/logs**", (route) =>
    route.fulfill({ json: EMPTY_LOGS })
  );
  await page.goto("/logs");
  await page.waitForSelector("text=No logs found");
  await page.evaluate(() => {
    const img = document.createElement("img");
    img.setAttribute("src", "data:image/gif;base64,R0lGODlhAQABAAD/ACwAAAAAAQABAAACADs=");
    // intentionally no alt — axe image-alt violation
    document.body.appendChild(img);
  });
  const results = await new AxeBuilder({ page }).analyze();
  const imageAltViolation = results.violations.find((v) => v.id === "image-alt");
  expect(imageAltViolation).toBeDefined();
});
```

- [ ] **Step 2: Run the tests and observe failures**

Run:

```bash
npm run test:a11y --workspace=apps/frontend
```

Expected:

```text
trace detail waterfall > has no axe violations — FAILED (axe violations on interactive span rows)
log search > has no axe violations — FAILED (axe violations on facet <li> items)
detects injected violation (harness proof) — PASSED
```

The exact violations will appear in the output. The trace detail failure will reference `interactive-supports-focus` or `onclick-key-events` on the span row divs. The log search failure will reference the same rules on the facet list items.

---

### Task 3: Fix interactive element semantics

**Files:**
- Modify: `apps/frontend/src/pages/TraceDetail.tsx:66-124`
- Modify: `apps/frontend/src/components/FacetSidebar.tsx:23-32`

- [ ] **Step 1: Fix the clickable span rows in `TraceDetail.tsx`**

The `<div>` at line 71 in `TraceDetail.tsx` has `onClick` but no keyboard role. Replace the outer span-row `<div>` open tag (lines 71–88) so it includes `role`, `tabIndex`, and `onKeyDown`:

```tsx
            <div
              key={span.span_id}
              role="button"
              tabIndex={0}
              onClick={() =>
                setSelectedSpanId(
                  span.span_id === selectedSpanId ? undefined : span.span_id
                )
              }
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  setSelectedSpanId(
                    span.span_id === selectedSpanId ? undefined : span.span_id
                  );
                }
              }}
              style={{
                display: "flex",
                alignItems: "center",
                marginBottom: 4,
                cursor: "pointer",
                background:
                  selectedSpanId === span.span_id ? "#edf2f7" : "transparent",
                borderRadius: "4px",
                padding: "2px 0",
              }}
            >
```

No other lines in `TraceDetail.tsx` change.

- [ ] **Step 2: Fix the clickable facet items in `FacetSidebar.tsx`**

The `<li>` at line 23 in `FacetSidebar.tsx` has `onClick` but no keyboard role. Replace the `<li>` open tag so it includes `role`, `tabIndex`, and `onKeyDown`:

```tsx
              <li
                key={v.value}
                role="button"
                tabIndex={0}
                onClick={() => onFacetClick(field, v.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    onFacetClick(field, v.value);
                  }
                }}
                className="facet-item"
              >
```

No other lines in `FacetSidebar.tsx` change.

- [ ] **Step 3: Run the existing unit tests to confirm no regression**

Run:

```bash
npm run test --workspace=apps/frontend -- --run
```

Expected:

```text
all existing unit and component tests pass
```

- [ ] **Step 4: Run the a11y tests and confirm both view tests now pass**

Run:

```bash
npm run test:a11y --workspace=apps/frontend
```

Expected:

```text
trace detail waterfall > has no axe violations — PASSED
log search > has no axe violations — PASSED
detects injected violation (harness proof) — PASSED

3 passed
```

If any additional violations appear beyond the span rows and facet items, fix them by applying the same `role="button"` + `tabIndex={0}` + `onKeyDown` pattern to the offending element, then re-run until all three tests pass.

- [ ] **Step 5: Commit the semantic fixes and tests**

Run:

```bash
git add apps/frontend/e2e/accessibility.spec.ts \
        apps/frontend/src/pages/TraceDetail.tsx \
        apps/frontend/src/components/FacetSidebar.tsx
git commit -m "feat(frontend): add accessibility regression tests and fix interactive element semantics"
```

---

### Task 4: Wire into local-ci.sh and update the roadmap plan

**Files:**
- Modify: `scripts/local-ci.sh:61-73`
- Modify: `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`

- [ ] **Step 1: Add the a11y step to `local-ci.sh`**

Inside the `if [[ $SKIP_FRONTEND -eq 0 ]]; then` block, after the existing `npm run test` line (currently line 72), insert:

```bash
  step "Frontend accessibility tests"
  if node -e "
    const { chromium } = require('./apps/frontend/node_modules/playwright-core');
    const fs = require('fs');
    process.exit(fs.existsSync(chromium.executablePath()) ? 0 : 1);
  " 2>/dev/null; then
    npm run test:a11y --workspace=apps/frontend && ok "a11y" || fail "a11y"
  else
    echo "SKIP  a11y (Chromium not installed — run: cd apps/frontend && npx playwright install chromium)"
  fi
```

The block from line 61 now reads:

```bash
if [[ $SKIP_FRONTEND -eq 0 ]]; then
  step "Frontend typecheck"
  npm run typecheck --workspace=apps/frontend && ok "typecheck" || fail "typecheck"

  step "Frontend lint"
  npm run lint --workspace=apps/frontend && ok "lint" || fail "lint"

  step "Frontend build"
  npm run build --workspace=apps/frontend && ok "build" || fail "build"

  step "Frontend tests"
  npm run test --workspace=apps/frontend -- --run && ok "tests" || fail "tests"

  step "Frontend accessibility tests"
  if node -e "
    const { chromium } = require('./apps/frontend/node_modules/playwright-core');
    const fs = require('fs');
    process.exit(fs.existsSync(chromium.executablePath()) ? 0 : 1);
  " 2>/dev/null; then
    npm run test:a11y --workspace=apps/frontend && ok "a11y" || fail "a11y"
  else
    echo "SKIP  a11y (Chromium not installed — run: cd apps/frontend && npx playwright install chromium)"
  fi
fi
```

- [ ] **Step 2: Mark P3-S6e complete in the phases plan**

In `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`, find the line:

```
- [ ] **P3-S6e: Add explicit accessibility regression coverage for the trace waterfall and other major new views**
```

Replace it with:

```
- [x] **P3-S6e: Add explicit accessibility regression coverage for the trace waterfall and other major new views**
```

Also append the outcome block immediately after the existing slice description (after the `Checkpoint:` line for P3-S6e):

```
  - Outcome: `apps/frontend` now has a Playwright + axe-core harness under `e2e/accessibility.spec.ts`. The trace detail waterfall and log search views each have a `checkA11y` scan; interactive span rows in `TraceDetail.tsx` and facet items in `FacetSidebar.tsx` received `role="button"` + `tabIndex` + `onKeyDown` semantic fixes. A negative proof test injects an `image-alt` violation and asserts it is caught. `local-ci.sh` runs the suite when Chromium is installed and skips gracefully otherwise.
  - Checkpoint: does the accessibility harness catch regressions on the Phase 1 waterfall without forcing every future slice to invent its own a11y test shape? Answer: yes. Future slices can call `new AxeBuilder({ page }).analyze()` after navigating to a new route, using `page.route()` to supply fixture data.
```

- [ ] **Step 3: Run `local-ci.sh` (frontend only) to confirm the new step works**

Run:

```bash
bash scripts/local-ci.sh --skip-docker
```

Expected:

```text
OK  typecheck
OK  lint
OK  build
OK  tests
OK  a11y          ← if Chromium is installed
  (or SKIP  a11y  ← if Chromium is not installed)
```

If `a11y` runs and fails, check the failure output and fix the remaining violation before proceeding.

- [ ] **Step 4: Commit**

Run:

```bash
git add scripts/local-ci.sh docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md
git commit -m "feat(ci): add frontend accessibility gate to local-ci.sh; close P3-S6e"
```

---

## Self-Review

### Spec coverage

| Spec requirement | Task |
|---|---|
| `@playwright/test` + `@axe-core/playwright` installed | Task 1 |
| `apps/frontend/playwright.config.ts` — Chromium, vite dev, `e2e/` | Task 1 |
| `apps/frontend/e2e/accessibility.spec.ts` | Task 2 |
| Trace detail waterfall coverage with `page.route()` mocking | Task 2 |
| Log search coverage with `page.route()` mocking | Task 2 |
| Negative regression proof test | Task 2 |
| Semantic fixes: `TraceDetail.tsx` span rows | Task 3 |
| Semantic fixes: `FacetSidebar.tsx` facet items | Task 3 |
| `test:a11y` script in `package.json` | Task 1 |
| `local-ci.sh` integration with graceful skip | Task 4 |

No uncovered spec requirement remains.

### Placeholder scan

No TBDs, TODOs, or vague steps. All code blocks show the exact content to write.

### Type consistency

- `AxeBuilder` imported from `@axe-core/playwright` — used consistently in both view tests and the negative proof test.
- Fixture shapes (`FIXTURE_TRACE`, `FIXTURE_LOGS`, `EMPTY_LOGS`) match the interfaces in `apps/frontend/src/api/traces.ts` and `apps/frontend/src/api/logs.ts`.
- `page.route("**/v1/traces/${TRACE_ID}", ...)` — glob pattern matches vite-proxied requests from `window.location.origin`.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-04-26-p3-s6e-accessibility.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

Which approach?
