import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const MOCK_USER = {
  user_id: "00000000-0000-0000-0000-000000000001",
  email: "test@example.com",
  tenants: [{ tenant_id: "00000000-0000-0000-0000-000000000001", role: "admin" }],
};

async function mockAuth(page: import("@playwright/test").Page) {
  await page.route("**/v1/auth/me", (route) => route.fulfill({ json: MOCK_USER }));
  await page.route("**/v1/tenants", (route) =>
    route.fulfill({ json: { tenants: [{ id: "00000000-0000-0000-0000-000000000001", name: "observable" }] } })
  );
  await page.route("**/v1/tenants/**/environments", (route) =>
    route.fulfill({ json: { environments: [{ environment: "prod" }] } })
  );
}

const T_NS = 1_700_000_000_000_000_000;

function makeTrace(id: string, statusCode: string, durationMs: number) {
  return {
    trace_id: id,
    root_service: "checkout",
    root_operation: `GET /order/${id}`,
    duration_ms: durationMs,
    status_code: statusCode,
    environment: "prod",
    start_time_unix_nano: T_NS,
  };
}

const NLQ_TRACES_RESPONSE = {
  type: "frame",
  frame: {
    data: [
      makeTrace("aaa", "ERROR", 150),
      makeTrace("bbb", "OK", 50),
      makeTrace("ccc", "OK", 3000),  // slow — >2s
    ],
  },
};

const NLQ_HISTOGRAM_RESPONSE = {
  buckets: [
    { start_ms: 1_700_000_000_000, end_ms: 1_700_000_060_000, count: 3 },
  ],
};

test.describe("trace explorer", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/nlq", (route) => route.fulfill({ json: NLQ_TRACES_RESPONSE }));
    await page.route("**/v1/tenants/**/traces/histogram**", (route) =>
      route.fulfill({ json: NLQ_HISTOGRAM_RESPONSE })
    );
  });

  test("renders status filter pills with counts", async ({ page }) => {
    await page.goto("/traces");
    await page.waitForSelector('[aria-label="Filter by status"]');
    const pills = page.locator('[aria-label="Filter by status"] button');
    await expect(pills).toHaveCount(3); // All, Error, OK
    await expect(pills.filter({ hasText: "All" })).toBeVisible();
    await expect(pills.filter({ hasText: "Error" })).toBeVisible();
    await expect(pills.filter({ hasText: "OK" })).toBeVisible();
  });

  test("status pill filter narrows displayed rows", async ({ page }) => {
    await page.goto("/traces");
    await page.waitForSelector('[aria-label="Trace results"]');
    await expect(page.locator('[aria-label="Trace results"] tbody tr')).toHaveCount(3);
    await page.locator('[aria-label="Filter by status"] button').filter({ hasText: "Error" }).click();
    await expect(page.locator('[aria-label="Trace results"] tbody tr')).toHaveCount(1);
  });

  test("empty state shown when no traces match filter", async ({ page }) => {
    await page.route("**/v1/nlq", (route) =>
      route.fulfill({ json: { type: "frame", frame: { data: [] } } })
    );
    await page.goto("/traces");
    await page.waitForSelector("text=No traces found");
    await expect(page.locator("text=No traces found")).toBeVisible();
  });

  test("error rows have red left border accent", async ({ page }) => {
    await page.goto("/traces");
    await page.waitForSelector('[aria-label="Trace results"]');
    const errorRow = page.locator('[aria-label="Trace results"] tbody tr').first();
    await expect(errorRow).toHaveClass(/border-l-\[var\(--bad\)\]/);
  });

  test("duration column uses tonal colors", async ({ page }) => {
    await page.goto("/traces");
    await page.waitForSelector('[aria-label="Trace results"]');
    // toHaveCSS resolves computed styles to rgb(), so resolve the CSS variables
    // the same way the browser does rather than comparing against the raw var() text.
    const resolveColorVar = (varName: string) =>
      page.evaluate((name) => {
        const el = document.createElement("span");
        el.style.color = `var(${name})`;
        document.body.appendChild(el);
        const resolved = getComputedStyle(el).color;
        el.remove();
        return resolved;
      }, varName);
    const rows = page.locator('[aria-label="Trace results"] tbody tr');
    // Row 2 (bbb, 50ms) should have green duration — color applied via inline style
    const fastSpan = rows.nth(1).locator("td").nth(4).locator("span.tabular-nums");
    await expect(fastSpan).toHaveCSS("color", await resolveColorVar("--good"));
    // Row 3 (ccc, 3000ms) is slow (>500ms), red duration
    const slowSpan = rows.nth(2).locator("td").nth(4).locator("span.tabular-nums");
    await expect(slowSpan).toHaveCSS("color", await resolveColorVar("--bad"));
  });

  test("passes accessibility audit", async ({ page }) => {
    await page.goto("/traces");
    await page.waitForSelector('[aria-label="Trace results"]');
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});
