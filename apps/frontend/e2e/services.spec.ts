import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

// ── Shared auth mocks ─────────────────────────────────────────────────────────

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
    route.fulfill({ json: { environments: ["prod"] } })
  );
}

// ── Fixtures ──────────────────────────────────────────────────────────────────

const FIXTURE_CHECKOUT = {
  service_name: "checkout",
  request_rate: 42.5,
  error_rate: 0.001,
  p95_latency_ms: 85,
  health_state: "healthy",
  active_alert_count: 0,
  latest_deployment: "v1.4.2",
};

const FIXTURE_PAYMENTS = {
  service_name: "payments",
  request_rate: 18.2,
  error_rate: 0.065,
  p95_latency_ms: 620,
  health_state: "breach",
  active_alert_count: 3,
  latest_deployment: null,
};

const FIXTURE_NOTIFICATIONS = {
  service_name: "notifications",
  request_rate: 5.1,
  error_rate: 0.012,
  p95_latency_ms: 210,
  health_state: "watch",
  active_alert_count: 1,
  latest_deployment: "v2.0.1",
};

const FIXTURE_SUMMARY_RESPONSE = {
  items: [FIXTURE_CHECKOUT, FIXTURE_PAYMENTS, FIXTURE_NOTIFICATIONS],
};

const FIXTURE_SERVICES_LIST = {
  items: ["checkout", "payments", "notifications"],
};

const FIXTURE_TOPOLOGY = {
  edges: [
    { caller: "checkout", callee: "payments", request_count: 1000, error_rate: 0.02, p95_latency_ms: 45 },
  ],
};

// Helper: wait for the service catalog table to be present (only rendered after data loads)
async function waitForTable(page: import("@playwright/test").Page) {
  await page.waitForSelector("table[aria-label='Service catalog']");
}

// ── Services list view ────────────────────────────────────────────────────────

test.describe("services list view", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/services/summary**", (route) =>
      route.fulfill({ json: FIXTURE_SUMMARY_RESPONSE })
    );
    await page.route("**/v1/services", (route) =>
      route.fulfill({ json: FIXTURE_SERVICES_LIST })
    );
    await page.route("**/v1/topology**", (route) =>
      route.fulfill({ json: FIXTURE_TOPOLOGY })
    );
  });

  test("renders service rows", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    await expect(page.locator("td", { hasText: "checkout" }).first()).toBeVisible();
    await expect(page.locator("td", { hasText: "payments" }).first()).toBeVisible();
    await expect(page.locator("td", { hasText: "notifications" }).first()).toBeVisible();
  });

  test("renders health filter pills", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    await expect(page.locator("button", { hasText: "All health" })).toBeVisible();
    await expect(page.locator("button", { hasText: "healthy" })).toBeVisible();
    await expect(page.locator("button", { hasText: "watch" })).toBeVisible();
    await expect(page.locator("button", { hasText: "breach" })).toBeVisible();
  });

  test("renders search input", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    await expect(page.getByPlaceholder("Search services…")).toBeVisible();
  });

  test("health pill shows count per state", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    await expect(page.locator("button", { hasText: "healthy" })).toContainText("(1)");
    await expect(page.locator("button", { hasText: "breach" })).toContainText("(1)");
    await expect(page.locator("button", { hasText: "watch" })).toContainText("(1)");
  });

  test("clicking health pill filters to that health state", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    await page.locator("button", { hasText: "breach" }).click();

    const table = page.locator("table[aria-label='Service catalog']");
    await expect(table.locator("td", { hasText: "payments" }).first()).toBeVisible();
    await expect(table.locator("td", { hasText: "checkout" })).not.toBeVisible();
    await expect(table.locator("td", { hasText: "notifications" })).not.toBeVisible();
  });

  test("search input filters rows by service name", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    await page.getByPlaceholder("Search services…").fill("notif");

    const table = page.locator("table[aria-label='Service catalog']");
    await expect(table.locator("td", { hasText: "notifications" }).first()).toBeVisible();
    await expect(table.locator("td", { hasText: "checkout" })).not.toBeVisible();
    await expect(table.locator("td", { hasText: "payments" })).not.toBeVisible();
  });

  test("breach rows have left border accent", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    const breachRow = page.locator("table[aria-label='Service catalog'] tr", { hasText: "payments" });
    await expect(breachRow).toHaveClass(/border-l-2/);
  });

  test("healthy rows have no breach border", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    const healthyRow = page.locator("table[aria-label='Service catalog'] tr", { hasText: "checkout" });
    const cls = await healthyRow.getAttribute("class");
    expect(cls ?? "").not.toContain("border-l-2");
  });

  test("error rate cell uses tonal color for breach service", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    const paymentsRow = page.locator("table[aria-label='Service catalog'] tr", { hasText: "payments" });
    const errorCell = paymentsRow.locator("span.tabular-nums").first();
    const style = await errorCell.getAttribute("style");
    expect(style).toContain("var(--bad)");
  });

  test("low error rate uses good tonal color", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    const checkoutRow = page.locator("table[aria-label='Service catalog'] tr", { hasText: "checkout" });
    const errorCell = checkoutRow.locator("span.tabular-nums").first();
    const style = await errorCell.getAttribute("style");
    expect(style).toContain("var(--good)");
  });

  test("deployment column shows latest deployment or dash", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    const table = page.locator("table[aria-label='Service catalog']");
    await expect(table.locator("tr", { hasText: "checkout" })).toContainText("v1.4.2");
    await expect(table.locator("tr", { hasText: "payments" })).toContainText("--");
  });

  test("metric cards render summary values", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    const summary = page.locator("[aria-label='Service summary']");
    await expect(summary.locator("text=Services")).toBeVisible();
    await expect(summary.locator("text=Active Alerts")).toBeVisible();
    await expect(summary.locator("text=Avg P95")).toBeVisible();
    await expect(summary.locator("text=Avg Error Rate")).toBeVisible();
  });

  test("has no axe violations in list view", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});

// ── View toggle ───────────────────────────────────────────────────────────────

test.describe("services view toggle", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/services/summary**", (route) =>
      route.fulfill({ json: FIXTURE_SUMMARY_RESPONSE })
    );
    await page.route("**/v1/services", (route) =>
      route.fulfill({ json: FIXTURE_SERVICES_LIST })
    );
    await page.route("**/v1/topology**", (route) =>
      route.fulfill({ json: FIXTURE_TOPOLOGY })
    );
  });

  test("List button is active by default", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    const listBtn = page.locator("button", { hasText: "List" });
    await expect(listBtn).toHaveClass(/bg-\[var\(--brand\)\]/);
  });

  test("clicking Topology tab hides the table and shows topology canvas", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    await page.locator("button", { hasText: "Topology" }).click();

    await expect(page.locator("table[aria-label='Service catalog']")).not.toBeVisible();
    await expect(page.locator("[data-testid='topology-background']")).toBeVisible();
  });

  test("switching back to List tab shows the table again", async ({ page }) => {
    await page.goto("/services");
    await waitForTable(page);

    await page.locator("button", { hasText: "Topology" }).click();
    await page.locator("button", { hasText: "List" }).click();

    await expect(page.locator("table[aria-label='Service catalog']")).toBeVisible();
  });
});
