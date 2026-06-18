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

const T = 1_700_000_000_000_000_000;

const FIXTURE_ERROR_LOG = {
  log_id: "log-1",
  timestamp_unix_nano: T,
  observed_timestamp_unix_nano: T,
  severity_number: 17,
  body: "checkout failed",
  service_name: "checkout",
  environment: "prod",
  host_id: "host-1",
  trace_id: null,
  span_id: null,
  fingerprint: null,
  attributes: {},
  resource_attributes: {},
};

const FIXTURE_WARN_LOG = {
  log_id: "log-2",
  timestamp_unix_nano: T + 1_000_000_000,
  observed_timestamp_unix_nano: T + 1_000_000_000,
  severity_number: 13,
  body: "slow query detected",
  service_name: "checkout",
  environment: "prod",
  host_id: "host-1",
  trace_id: null,
  span_id: null,
  fingerprint: null,
  attributes: {},
  resource_attributes: {},
};

const FIXTURE_INFO_LOG = {
  log_id: "log-3",
  timestamp_unix_nano: T + 2_000_000_000,
  observed_timestamp_unix_nano: T + 2_000_000_000,
  severity_number: 9,
  body: "order processed",
  service_name: "checkout",
  environment: "prod",
  host_id: "host-1",
  trace_id: "abc123",
  span_id: "def456",
  fingerprint: null,
  attributes: {},
  resource_attributes: {},
};

const NLQ_LOGS_RESPONSE = {
  type: "frame",
  frame: { data: [FIXTURE_ERROR_LOG, FIXTURE_WARN_LOG, FIXTURE_INFO_LOG] },
};

const NLQ_HISTOGRAM_RESPONSE = {
  buckets: [
    { start_ms: 1_700_000_000_000, end_ms: 1_700_000_060_000, counts: { "17": 1, "13": 1, "9": 1 } },
  ],
};

test.describe("log explorer", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/nlq", (route) => route.fulfill({ json: NLQ_LOGS_RESPONSE }));
    await page.route("**/v1/tenants/**/logs/histogram**", (route) =>
      route.fulfill({ json: NLQ_HISTOGRAM_RESPONSE })
    );
  });

  test("renders severity filter pills with counts", async ({ page }) => {
    await page.goto("/logs");
    await page.waitForSelector('[aria-label="Filter by severity"]');
    const pills = page.locator('[aria-label="Filter by severity"] button');
    await expect(pills).toHaveCount(5); // All, Error, Warn, Info, Debug
    await expect(pills.filter({ hasText: "All" })).toBeVisible();
    await expect(pills.filter({ hasText: "Error" })).toBeVisible();
    await expect(pills.filter({ hasText: "Warn" })).toBeVisible();
  });

  test("severity pill filter narrows displayed rows", async ({ page }) => {
    await page.goto("/logs");
    await page.waitForSelector('[aria-label="Log results"]');
    // All 3 rows visible initially
    await expect(page.locator('[aria-label="Log results"] tbody tr')).toHaveCount(3);
    // Filter to Error only
    await page.locator('[aria-label="Filter by severity"] button').filter({ hasText: "Error" }).click();
    await expect(page.locator('[aria-label="Log results"] tbody tr')).toHaveCount(1);
  });

  test("message search filters rows", async ({ page }) => {
    await page.goto("/logs");
    await page.waitForSelector('[aria-label="Log results"]');
    await page.fill('[aria-label="Search log messages"]', "checkout");
    await expect(page.locator('[aria-label="Log results"] tbody tr')).toHaveCount(1);
    await expect(page.locator('[aria-label="Log results"]')).toContainText("checkout failed");
  });

  test("empty state shown when no logs match filter", async ({ page }) => {
    await page.route("**/v1/nlq", (route) =>
      route.fulfill({ json: { type: "frame", frame: { data: [] } } })
    );
    await page.goto("/logs");
    await page.waitForSelector("text=No logs found");
    await expect(page.locator("text=No logs found")).toBeVisible();
  });

  test("error rows have red left border accent", async ({ page }) => {
    await page.goto("/logs");
    await page.waitForSelector('[aria-label="Log results"]');
    const errorRow = page.locator('[aria-label="Log results"] tbody tr').first();
    await expect(errorRow).toHaveClass(/border-l-\[var\(--bad\)\]/);
  });

  test("passes accessibility audit", async ({ page }) => {
    await page.goto("/logs");
    await page.waitForSelector('[aria-label="Log results"]');
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});
