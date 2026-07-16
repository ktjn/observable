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

const TRACE_ID = "aaaaaaaaaaaaaaaa1111111111111111";
const T_NS = "1700000000000000000";

const FIXTURE_SPANS = {
  spans: [
    {
      trace_id: TRACE_ID,
      span_id: "span-root-001",
      parent_span_id: null,
      service_name: "checkout",
      operation_name: "POST /checkout",
      start_time_unix_nano: T_NS,
      end_time_unix_nano: "1700000000050000000",
      status_code: "OK",
      resource_attributes: {},
      attributes: {},
    },
    {
      trace_id: TRACE_ID,
      span_id: "span-child-002",
      parent_span_id: "span-root-001",
      service_name: "payment",
      operation_name: "charge",
      start_time_unix_nano: "1700000000010000000",
      end_time_unix_nano: "1700000000040000000",
      status_code: "ERROR",
      resource_attributes: {},
      attributes: { "error.message": "timeout" },
    },
  ],
};

const FIXTURE_CORRELATED_LOGS = {
  items: [
    {
      timestamp: "2026-05-15T10:00:00.010Z",
      severity: "ERROR",
      body: "payment charge failed: timeout",
      service_name: "payment",
      trace_id: TRACE_ID,
      span_id: "span-child-002",
      resource_attributes: {},
      attributes: {},
    },
    {
      timestamp: "2026-05-15T10:00:00.001Z",
      severity: "INFO",
      body: "checkout initiated",
      service_name: "checkout",
      trace_id: TRACE_ID,
      span_id: "span-root-001",
      resource_attributes: {},
      attributes: {},
    },
  ],
};

test.describe("trace detail and correlated logs", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route(`**/v1/traces/${TRACE_ID}`, (route) =>
      route.fulfill({ json: FIXTURE_SPANS })
    );
    await page.route("**/v1/logs**", (route) =>
      route.fulfill({ json: FIXTURE_CORRELATED_LOGS })
    );
  });

  test("renders waterfall with service names", async ({ page }) => {
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=checkout");
    await expect(page.locator("text=checkout")).toBeVisible();
    await expect(page.locator("text=payment")).toBeVisible();
  });

  test("renders trace-correlated logs panel", async ({ page }) => {
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=Trace-correlated logs");
    await expect(page.locator("text=Trace-correlated logs")).toBeVisible();
  });

  test("correlated logs show log entries", async ({ page }) => {
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=Trace-correlated logs");
    await expect(page.locator("text=payment charge failed")).toBeVisible();
    await expect(page.locator("text=checkout initiated")).toBeVisible();
  });

  test("shows error span count in summary", async ({ page }) => {
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=checkout");
    await expect(page.locator("text=1").first()).toBeVisible();
  });

  test("clicking a span updates log panel title", async ({ page }) => {
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=checkout");
    await page.locator("text=charge").click();
    await expect(page.locator("text=Exact span logs")).toBeVisible();
  });

  test("passes accessibility audit", async ({ page }) => {
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=checkout");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});
