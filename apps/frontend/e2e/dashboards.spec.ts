import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

// ── Shared auth/tenant mocks ──────────────────────────────────────────────────

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

const FIXTURE_DASHBOARD_1 = {
  dashboard_id: "dash-1",
  name: "Checkout Overview",
  panels: [
    { panel_id: "p1", title: "Errors", panel_kind: "query", query_kind: "logs", service: "checkout", preset: "1h", filters: {}, query_text: "", content: null, layout: { x: 0, y: 0, w: 6, h: 4 }, time_range: { mode: "preset", preset: "1h" } },
    { panel_id: "p2", title: "Latency", panel_kind: "query", query_kind: "traces", service: "checkout", preset: "1h", filters: {}, query_text: "", content: null, layout: { x: 6, y: 0, w: 6, h: 4 }, time_range: { mode: "preset", preset: "1h" } },
  ],
  created_at: "2026-05-05T00:00:00Z",
};

const FIXTURE_DASHBOARD_2 = {
  dashboard_id: "dash-2",
  name: "Payments Health",
  panels: [
    { panel_id: "p3", title: "Rate", panel_kind: "query", query_kind: "metrics", service: "payments", preset: "1h", filters: {}, query_text: "", content: null, layout: { x: 0, y: 0, w: 6, h: 4 }, time_range: { mode: "preset", preset: "1h" } },
  ],
  created_at: "2026-05-06T00:00:00Z",
};

const FIXTURE_LIST_RESPONSE = {
  items: [FIXTURE_DASHBOARD_1, FIXTURE_DASHBOARD_2],
};

const FIXTURE_EMPTY_RESPONSE = {
  items: [],
};

// ── Dashboard list ────────────────────────────────────────────────────────────

test.describe("dashboard list", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/dashboards", (route) =>
      route.fulfill({ json: FIXTURE_LIST_RESPONSE })
    );
  });

  test("renders dashboard cards in a grid", async ({ page }) => {
    await page.goto("/dashboards");
    await page.waitForSelector("[aria-label='Dashboard cards']");

    await expect(page.locator("text=Checkout Overview")).toBeVisible();
    await expect(page.locator("text=Payments Health")).toBeVisible();
  });

  test("each card shows panel count", async ({ page }) => {
    await page.goto("/dashboards");
    await page.waitForSelector("[aria-label='Dashboard cards']");

    const checkoutCard = page.locator("[aria-label='Dashboard cards'] > div", { hasText: "Checkout Overview" });
    await expect(checkoutCard).toContainText("2 panels");

    const paymentsCard = page.locator("[aria-label='Dashboard cards'] > div", { hasText: "Payments Health" });
    await expect(paymentsCard).toContainText("1 panel");
  });

  test("each card has an Open link", async ({ page }) => {
    await page.goto("/dashboards");
    await page.waitForSelector("[aria-label='Dashboard cards']");

    const checkoutCard = page.locator("[aria-label='Dashboard cards'] > div", { hasText: "Checkout Overview" });
    const openLink = checkoutCard.locator("a", { hasText: "Open" });
    await expect(openLink).toBeVisible();
    await expect(openLink).toHaveAttribute("href", "/dashboards/dash-1");
  });

  test("each card has Export and Delete buttons", async ({ page }) => {
    await page.goto("/dashboards");
    await page.waitForSelector("[aria-label='Dashboard cards']");

    const checkoutCard = page.locator("[aria-label='Dashboard cards'] > div", { hasText: "Checkout Overview" });
    await expect(checkoutCard.locator("button", { hasText: "Export" })).toBeVisible();
    await expect(checkoutCard.locator("button", { hasText: "Delete" })).toBeVisible();
  });

  test("has no axe violations", async ({ page }) => {
    await page.goto("/dashboards");
    await page.waitForSelector("[aria-label='Dashboard cards']");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});

// ── Empty state ───────────────────────────────────────────────────────────────

test.describe("dashboard empty state", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/dashboards", (route) =>
      route.fulfill({ json: FIXTURE_EMPTY_RESPONSE })
    );
  });

  test("renders empty state when no dashboards", async ({ page }) => {
    await page.goto("/dashboards");
    await page.waitForSelector("text=No dashboards yet");

    await expect(page.locator("text=Create your first dashboard")).toBeVisible();
  });

  test("has no axe violations in empty state", async ({ page }) => {
    await page.goto("/dashboards");
    await page.waitForSelector("text=No dashboards yet");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});
