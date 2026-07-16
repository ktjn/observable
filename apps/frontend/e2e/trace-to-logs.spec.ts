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

const FIXTURE_TRACE = {
  trace_id: TRACE_ID,
  spans: [
    {
      tenant_id: "00000000-0000-0000-0000-000000000001",
      trace_id: TRACE_ID,
      span_id: "span-root-001",
      parent_span_id: null,
      service_name: "checkout",
      operation_name: "POST /checkout",
      start_time_unix_nano: 1_000_000_000,
      end_time_unix_nano: 6_000_000_000,
      duration_ns: 5_000_000_000,
      status_code: "OK",
      span_kind: "SERVER",
      service_version: null,
      attributes: {},
      resource_attributes: {},
    },
    {
      tenant_id: "00000000-0000-0000-0000-000000000001",
      trace_id: TRACE_ID,
      span_id: "span-child-002",
      parent_span_id: "span-root-001",
      service_name: "payment",
      operation_name: "charge",
      start_time_unix_nano: 2_000_000_000,
      end_time_unix_nano: 5_000_000_000,
      duration_ns: 3_000_000_000,
      status_code: "ERROR",
      span_kind: "CLIENT",
      service_version: null,
      attributes: { "error.message": "timeout" },
      resource_attributes: {},
    },
  ],
  events: [],
};

test.describe("trace detail and correlated logs", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route(`**/v1/traces/${TRACE_ID}`, (route) =>
      route.fulfill({ json: FIXTURE_TRACE })
    );
    await page.route("**/v1/logs**", (route) =>
      route.fulfill({ json: { logs: [], total: 0, facets: {} } })
    );
  });

  test("renders waterfall with service names", async ({ page }) => {
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=POST /checkout");
    await expect(page.getByText("checkout", { exact: true })).toBeVisible();
    await expect(page.getByText("payment", { exact: true })).toBeVisible();
  });

  test("renders trace detail heading", async ({ page }) => {
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=POST /checkout");
    await expect(page.locator("text=POST /checkout")).toBeVisible();
  });

  test("shows span operations", async ({ page }) => {
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=POST /checkout");
    await expect(page.locator("text=charge")).toBeVisible();
  });

  test("passes accessibility audit", async ({ page }) => {
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=POST /checkout");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});
