# P3-S6e: Accessibility Regression Coverage Design

**Slice:** P3-S6e  
**Date:** 2026-04-26  
**Source spec:** `spec/05-frontend.md` §9.3 Phase 1 and frontend slice operating rules in `docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`

---

## Goal

Add automated `playwright-axe` accessibility regression coverage for the trace detail waterfall and one additional major view, with a reusable harness that future slices can extend without reinventing the setup.

---

## Scope

**In scope:**
- Playwright + axe-core installation and configuration under `apps/frontend`
- Accessibility spec covering trace detail waterfall and log search
- Minimal semantic fixes to the trace waterfall span rows to clear axe violations
- Integration into `local-ci.sh` (graceful skip if Chromium not installed)

**Out of scope:**
- Visual regression testing
- Exhaustive coverage of every route
- WCAG manual audit or remediation beyond what axe catches automatically

---

## Dependencies

Added to `apps/frontend/package.json` devDependencies:

| Package | Version |
|---|---|
| `@playwright/test` | `^1.59.1` |
| `@axe-core/playwright` | `^4.11.2` |

Browser prerequisite (one-time local setup):
```bash
npx playwright install --with-deps chromium
```

---

## Architecture

### Config

`apps/frontend/playwright.config.ts` — Chromium only, launches vite dev server via `webServer`, test directory is `e2e/`.

```
apps/frontend/
  playwright.config.ts       ← Playwright config
  e2e/
    accessibility.spec.ts    ← Accessibility tests
```

### API Mocking

Tests use `page.route()` to intercept backend API calls and return minimal fixture JSON. No MSW service worker is needed in the Playwright context — `page.route()` is simpler and has no dependency on the service worker lifecycle.

### Test Shape

Each view test follows this sequence:
1. Register `page.route()` handlers for the API calls the view makes
2. Navigate to the route
3. Wait for a key content element to be visible (prevents scanning a loading state)
4. Call `checkA11y(page)` from `@axe-core/playwright`
5. Assert zero violations

### Views Covered

| View | Route | Key wait condition |
|---|---|---|
| Trace detail waterfall | `/traces/:traceId` | Waterfall container visible |
| Log search | `/logs` | Log results or empty state visible |

### Negative Regression Proof

One test injects a known axe violation (an `<img>` with no `alt` attribute) into the DOM and asserts that `checkA11y` throws. This proves the harness catches regressions and does not silently pass.

---

## Semantic Fixes

The trace waterfall span rows currently render as `<div onClick>` with no keyboard role. Axe flags these as violations (`interactive-supports-focus`, `keyboard`). Fix: add `role="button"` and `tabIndex={0}` to each clickable span row in `TraceDetail.tsx`. No visual change.

---

## Script

New script in `apps/frontend/package.json`:

```json
"test:a11y": "playwright test"
```

---

## local-ci.sh Integration

A new step added inside the `SKIP_FRONTEND` block, after the existing `npm run test` step:

```bash
step "Frontend accessibility tests"
# skip gracefully if Chromium browser binaries are not installed
if <chromium-installed-check>; then
  npm run test:a11y --workspace=apps/frontend && ok "a11y" || fail "a11y"
else
  echo "SKIP  a11y (playwright browsers not installed — run: npx playwright install --with-deps chromium)"
fi
```

The exact browser-installed check is resolved during implementation (e.g. checking the Playwright browser cache directory). If Chromium is not installed, the step prints a hint and exits zero rather than failing the gate.

---

## Verification

- `checkA11y` on trace detail passes with zero violations after semantic fixes
- `checkA11y` on log search passes with zero violations
- Negative test fails (proves harness catches real violations)
- `npm run test:a11y --workspace=apps/frontend` passes locally
- `bash scripts/local-ci.sh` passes end-to-end (or skips a11y gracefully)

---

## Checkpoint

Does the accessibility harness catch regressions on the Phase 1 waterfall without forcing every future slice to invent its own a11y test shape? Yes — `checkA11y(page)` is a one-liner that any future spec can call after navigating to a new route, with `page.route()` providing the fixture data pattern to follow.
