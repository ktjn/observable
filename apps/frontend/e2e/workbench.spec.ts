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
    route.fulfill({
      json: { tenants: [{ id: "00000000-0000-0000-0000-000000000001", name: "observable" }] },
    }),
  );
  await page.route("**/v1/tenants/**/environments", (route) =>
    route.fulfill({ json: { environments: [{ environment: "prod" }] } }),
  );
}

test.describe("workbench", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
  });

  test("has no axe violations", async ({ page }) => {
    await page.goto("/workbench");
    await page.waitForSelector("[data-testid='workbench-block-metrics']");
    await page.waitForSelector("[data-testid='workbench-editor']");

    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});
