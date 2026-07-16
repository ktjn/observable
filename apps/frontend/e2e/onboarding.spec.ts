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

test.describe("onboarding wizard", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/setup/status", (route) =>
      route.fulfill({ json: { state: "waiting", traces: 0, logs: 0, metrics: 0 } })
    );
  });

  test("renders language picker on first step", async ({ page }) => {
    await page.goto("/getting-started");
    await page.waitForSelector("text=Choose language");
    await expect(page.locator("button", { hasText: "Node.js" })).toBeVisible();
    await expect(page.locator("button", { hasText: "Python" })).toBeVisible();
    await expect(page.locator("button", { hasText: "Go" })).toBeVisible();
    await expect(page.locator("button", { hasText: "Java" })).toBeVisible();
  });

  test("Next button is disabled until a language is selected", async ({ page }) => {
    await page.goto("/getting-started");
    await page.waitForSelector("text=Choose language");
    const nextBtn = page.locator("button", { hasText: /Next/ });
    await expect(nextBtn).toBeDisabled();
    await page.locator("button", { hasText: "Node.js" }).click();
    await expect(nextBtn).toBeEnabled();
  });

  test("selecting language advances to API key step", async ({ page }) => {
    await page.goto("/getting-started");
    await page.waitForSelector("text=Choose language");
    await page.locator("button", { hasText: "Python" }).click();
    await page.locator("button", { hasText: /Next/ }).click();
    await expect(page.locator("text=Get API key")).toBeVisible();
    await expect(page.locator("text=Create API key")).toBeVisible();
  });

  test("API key step shows install command for selected language", async ({ page }) => {
    await page.goto("/getting-started");
    await page.waitForSelector("text=Choose language");
    await page.locator("button", { hasText: "Node.js" }).click();
    await page.locator("button", { hasText: /Next/ }).click();
    await expect(page.locator("text=Install the SDK")).toBeVisible();
    await expect(page.locator("pre")).toContainText("npm install");
  });

  test("creating API key shows plaintext token", async ({ page }) => {
    await page.route("**/v1/tokens", (route) => {
      if (route.request().method() === "POST") {
        return route.fulfill({
          json: {
            id: "tok-1",
            name: "onboarding-nodejs",
            tenant_name: "observable",
            environment: "production",
            created_at: "2026-05-15T00:00:00Z",
            revoked: false,
            plaintext: "osk_test_key_12345",
          },
        });
      }
      return route.fulfill({ json: { tokens: [] } });
    });
    await page.goto("/getting-started");
    await page.waitForSelector("text=Choose language");
    await page.locator("button", { hasText: "Node.js" }).click();
    await page.locator("button", { hasText: /Next/ }).click();
    await page.locator("button", { hasText: "Create API key" }).click();
    await expect(page.locator("text=osk_test_key_12345")).toBeVisible();
  });

  test("wizard progress bar shows correct steps", async ({ page }) => {
    await page.goto("/getting-started");
    const progress = page.locator("[aria-label='Wizard progress']");
    await expect(progress).toBeVisible();
    await expect(progress).toContainText("Choose language");
    await expect(progress).toContainText("Get API key");
    await expect(progress).toContainText("Send data");
    await expect(progress).toContainText("Done");
  });

  test("Skip wizard button is visible", async ({ page }) => {
    await page.goto("/getting-started");
    await page.waitForSelector("text=Choose language");
    await expect(page.locator("button", { hasText: "Skip wizard" })).toBeVisible();
  });

  test("signal detection shows success with badge counts", async ({ page }) => {
    await page.route("**/v1/tokens", (route) => {
      if (route.request().method() === "POST") {
        return route.fulfill({
          json: {
            id: "tok-1",
            name: "onboarding-nodejs",
            tenant_name: "observable",
            environment: "production",
            created_at: "2026-05-15T00:00:00Z",
            revoked: false,
            plaintext: "osk_test_key",
          },
        });
      }
      return route.fulfill({ json: { tokens: [] } });
    });
    await page.route("**/v1/setup/status", (route) =>
      route.fulfill({
        json: { state: "detected", traces: 5, logs: 3, metrics: 1 },
      })
    );
    await page.goto("/getting-started");
    await page.waitForSelector("text=Choose language");
    await page.locator("button", { hasText: "Node.js" }).click();
    await page.locator("button", { hasText: /Next/ }).click();
    await page.locator("button", { hasText: "Create API key" }).click();
    await expect(page.locator("text=Your first signal arrived!")).toBeVisible({ timeout: 10000 });
    await expect(page.locator("text=5 traces")).toBeVisible();
    await expect(page.locator("text=3 logs")).toBeVisible();
    await expect(page.locator("text=1 metric")).toBeVisible();
  });

  test("passes accessibility audit", async ({ page }) => {
    await page.goto("/getting-started");
    await page.waitForSelector("text=Choose language");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});
