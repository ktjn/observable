import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const MOCK_USER = {
  user_id: "00000000-0000-0000-0000-000000000001",
  email: "admin@example.com",
  tenants: [{ tenant_id: "00000000-0000-0000-0000-000000000001", role: "tenant_admin" }],
};

const MOCK_VIEWER = {
  user_id: "00000000-0000-0000-0000-000000000099",
  email: "viewer@example.com",
  tenants: [{ tenant_id: "00000000-0000-0000-0000-000000000001", role: "viewer" }],
};

async function mockAuth(page: import("@playwright/test").Page, user = MOCK_USER) {
  await page.route("**/v1/auth/me", (route) => route.fulfill({ json: user }));
  await page.route("**/v1/tenants", (route) =>
    route.fulfill({ json: { tenants: [{ id: "00000000-0000-0000-0000-000000000001", name: "observable" }] } })
  );
  await page.route("**/v1/tenants/**/environments", (route) =>
    route.fulfill({ json: { environments: [{ environment: "prod" }] } })
  );
}

const FIXTURE_MEMBERS = {
  members: [
    {
      user_id: "00000000-0000-0000-0000-000000000001",
      email: "admin@example.com",
      name: "Admin User",
      role: "tenant_admin",
    },
    {
      user_id: "00000000-0000-0000-0000-000000000002",
      email: "dev@example.com",
      name: "Dev User",
      role: "member",
    },
    {
      user_id: "00000000-0000-0000-0000-000000000003",
      email: "readonly@example.com",
      name: null,
      role: "viewer",
    },
  ],
};

async function mockMembersApi(page: import("@playwright/test").Page) {
  await page.route("**/v1/tenants/**/members**", (route) => {
    if (route.request().method() === "GET") {
      return route.fulfill({ json: FIXTURE_MEMBERS });
    }
    if (route.request().method() === "PUT") {
      return route.fulfill({ status: 200, json: {} });
    }
    if (route.request().method() === "POST") {
      return route.fulfill({ status: 201, json: {} });
    }
    if (route.request().method() === "DELETE") {
      return route.fulfill({ status: 204 });
    }
    return route.continue();
  });
  await page.route("**/v1/tenants/**/members/**/sessions", (route) =>
    route.fulfill({ status: 204 })
  );
}

test.describe("admin member management", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await mockMembersApi(page);
  });

  test("renders member table with all members", async ({ page }) => {
    await page.goto("/admin/members");
    await page.waitForSelector("text=Members (3)");
    await expect(page.locator("text=Admin User")).toBeVisible();
    await expect(page.locator("text=Dev User")).toBeVisible();
    await expect(page.locator("text=readonly@example.com")).toBeVisible();
  });

  test("shows 'you' label next to current user", async ({ page }) => {
    await page.goto("/admin/members");
    await page.waitForSelector("text=Members (3)");
    const adminRow = page.locator("tr", { hasText: "Admin User" });
    await expect(adminRow.locator("text=you")).toBeVisible();
  });

  test("admin sees role dropdowns for other members", async ({ page }) => {
    await page.goto("/admin/members");
    await page.waitForSelector("text=Members (3)");
    const devRow = page.locator("tr", { hasText: "Dev User" });
    await expect(devRow.locator("select[aria-label='Role for dev@example.com']")).toBeVisible();
  });

  test("admin sees Remove button for other members", async ({ page }) => {
    await page.goto("/admin/members");
    await page.waitForSelector("text=Members (3)");
    const devRow = page.locator("tr", { hasText: "Dev User" });
    await expect(devRow.locator("[aria-label='Remove dev@example.com']")).toBeVisible();
  });

  test("admin does not see Remove button for self", async ({ page }) => {
    await page.goto("/admin/members");
    await page.waitForSelector("text=Members (3)");
    const adminRow = page.locator("tr", { hasText: "Admin User" });
    await expect(adminRow.locator("[aria-label='Remove admin@example.com']")).not.toBeVisible();
  });

  test("admin sees Revoke sessions button", async ({ page }) => {
    await page.goto("/admin/members");
    await page.waitForSelector("text=Members (3)");
    const devRow = page.locator("tr", { hasText: "Dev User" });
    await expect(devRow.locator("[aria-label='Revoke sessions for dev@example.com']")).toBeVisible();
  });

  test("admin sees Add member form", async ({ page }) => {
    await page.goto("/admin/members");
    await page.waitForSelector("text=Add member");
    await expect(page.locator("label", { hasText: "Email" })).toBeVisible();
    await expect(page.locator("label", { hasText: "Role" })).toBeVisible();
    await expect(page.locator("button", { hasText: "Add" })).toBeVisible();
  });

  test("role dropdown has correct options", async ({ page }) => {
    await page.goto("/admin/members");
    await page.waitForSelector("text=Members (3)");
    const devRow = page.locator("tr", { hasText: "Dev User" });
    const select = devRow.locator("select[aria-label='Role for dev@example.com']");
    const options = select.locator("option");
    await expect(options).toHaveCount(3);
  });

  test("passes accessibility audit", async ({ page }) => {
    await page.goto("/admin/members");
    await page.waitForSelector("text=Members (3)");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});

test.describe("viewer cannot manage members", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page, MOCK_VIEWER);
    await mockMembersApi(page);
  });

  test("viewer does not see Add member form", async ({ page }) => {
    await page.goto("/admin/members");
    await page.waitForSelector("text=Members");
    await expect(page.locator("text=Add member")).not.toBeVisible();
  });

  test("viewer sees role badges instead of dropdowns", async ({ page }) => {
    await page.goto("/admin/members");
    await page.waitForSelector("text=Members");
    await expect(page.locator("select[aria-label='Role for dev@example.com']")).not.toBeVisible();
  });
});
