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

const FIXTURE_METRICS_LIST = {
  items: [
    { metric_name: "http_request_duration_seconds", type: "histogram", unit: "s" },
    { metric_name: "http_requests_total", type: "sum", unit: "" },
    { metric_name: "process_cpu_seconds_total", type: "sum", unit: "s" },
  ],
};

const FIXTURE_METRIC_QUERY = {
  type: "frame",
  frame: {
    data: [
      { timestamp: "2026-05-15T10:00:00Z", value: 42.5, labels: { service: "checkout" } },
      { timestamp: "2026-05-15T10:01:00Z", value: 43.1, labels: { service: "checkout" } },
    ],
  },
};

const FIXTURE_DASHBOARDS = {
  items: [
    {
      dashboard_id: "dash-1",
      name: "Test Dashboard",
      panels: [
        {
          panel_id: "p1",
          title: "Request Rate",
          panel_kind: "query",
          query_kind: "metrics",
          service: "checkout",
          preset: "1h",
          filters: {},
          query_text: "",
          content: null,
          layout: { x: 0, y: 0, w: 6, h: 4 },
          time_range: { mode: "preset", preset: "1h" },
        },
      ],
      created_at: "2026-05-05T00:00:00Z",
    },
  ],
};

test.describe("metric exploration", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/metrics", (route) =>
      route.fulfill({ json: FIXTURE_METRICS_LIST })
    );
    await page.route("**/v1/nlq**", (route) =>
      route.fulfill({ json: FIXTURE_METRIC_QUERY })
    );
  });

  test("renders metrics list page", async ({ page }) => {
    await page.goto("/metrics");
    await expect(page.locator("text=http_request_duration_seconds")).toBeVisible();
    await expect(page.locator("text=http_requests_total")).toBeVisible();
  });

  test("passes accessibility audit", async ({ page }) => {
    await page.goto("/metrics");
    await page.waitForSelector("text=http_request_duration_seconds");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});

test.describe("dashboard creation flow", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/dashboards", (route) => {
      if (route.request().method() === "GET") {
        return route.fulfill({ json: FIXTURE_DASHBOARDS });
      }
      if (route.request().method() === "POST") {
        return route.fulfill({
          status: 201,
          json: {
            dashboard_id: "dash-new",
            name: "New Dashboard",
            panels: [],
            created_at: "2026-05-15T10:00:00Z",
          },
        });
      }
      return route.continue();
    });
    await page.route("**/v1/dashboards/dash-1", (route) =>
      route.fulfill({ json: FIXTURE_DASHBOARDS.items[0] })
    );
    await page.route("**/v1/nlq**", (route) =>
      route.fulfill({ json: FIXTURE_METRIC_QUERY })
    );
  });

  test("dashboard list shows existing dashboard", async ({ page }) => {
    await page.goto("/dashboards");
    await page.waitForSelector("[aria-label='Dashboard cards']");
    await expect(page.locator("text=Test Dashboard")).toBeVisible();
    await expect(page.locator("text=1 panel")).toBeVisible();
  });

  test("dashboard detail page shows panels", async ({ page }) => {
    await page.goto("/dashboards/dash-1");
    await expect(page.locator("text=Request Rate")).toBeVisible();
  });

  test("dashboard detail passes accessibility audit", async ({ page }) => {
    await page.goto("/dashboards/dash-1");
    await page.waitForSelector("text=Request Rate");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});
