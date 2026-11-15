import { test, expect } from "@playwright/test";

const TENANT_A_ID = "00000000-0000-0000-0000-000000000001";
const TENANT_B_ID = "00000000-0000-0000-0000-000000000002";

const MOCK_USER_TENANT_A = {
  user_id: "00000000-0000-0000-0000-000000000001",
  email: "user-a@example.com",
  tenants: [{ tenant_id: TENANT_A_ID, role: "admin" }],
};

const MOCK_USER_MULTI_TENANT = {
  user_id: "00000000-0000-0000-0000-000000000001",
  email: "multi@example.com",
  tenants: [
    { tenant_id: TENANT_A_ID, role: "admin" },
    { tenant_id: TENANT_B_ID, role: "member" },
  ],
};

async function mockDataEndpoints(page: import("@playwright/test").Page) {
  await page.route("**/v1/nlq**", (route) =>
    route.fulfill({ json: { type: "frame", frame: { data: [] } } })
  );
  await page.route("**/v1/services/summary**", (route) =>
    route.fulfill({ json: { items: [] } })
  );
  await page.route("**/v1/services", (route) =>
    route.fulfill({ json: { items: [] } })
  );
  await page.route("**/v1/topology**", (route) =>
    route.fulfill({ json: { edges: [] } })
  );
}

async function mockAuthSingleTenant(page: import("@playwright/test").Page) {
  await page.route("**/v1/auth/me", (route) => route.fulfill({ json: MOCK_USER_TENANT_A }));
  await page.route("**/v1/tenants", (route) =>
    route.fulfill({
      json: { tenants: [{ id: TENANT_A_ID, name: "Tenant A" }] },
    })
  );
  await page.route("**/v1/tenants/**/environments", (route) =>
    route.fulfill({ json: { environments: [{ environment: "prod" }] } })
  );
}

async function mockAuthMultiTenant(page: import("@playwright/test").Page) {
  await page.route("**/v1/auth/me", (route) => route.fulfill({ json: MOCK_USER_MULTI_TENANT }));
  await page.route("**/v1/tenants", (route) =>
    route.fulfill({
      json: {
        tenants: [
          { id: TENANT_A_ID, name: "Tenant A" },
          { id: TENANT_B_ID, name: "Tenant B" },
        ],
      },
    })
  );
  await page.route("**/v1/tenants/**/environments", (route) =>
    route.fulfill({ json: { environments: [{ environment: "prod" }] } })
  );
}

test.describe("tenant isolation — single tenant", () => {
  test("renders single tenant selector with active tenant", async ({ page }) => {
    await mockAuthSingleTenant(page);
    await mockDataEndpoints(page);
    await page.goto("/services");
    const select = page.locator("select[aria-label='Tenant']");
    await expect(select).toBeVisible();
    await expect(select.locator("option")).toHaveCount(1);
    await expect(select).toHaveValue(TENANT_A_ID);
  });
});

test.describe("tenant isolation — multi tenant", () => {
  test("tenant selector switches tenant via /v1/auth/switch-tenant", async ({ page }) => {
    await mockAuthMultiTenant(page);
    await mockDataEndpoints(page);
    let switchedTenantId: string | null = null;
    await page.route("**/v1/auth/switch-tenant", async (route) => {
      const body = route.request().postDataJSON() as { tenant_id: string };
      switchedTenantId = body.tenant_id;
      await route.fulfill({ json: { tenant_id: body.tenant_id, role: "member" } });
    });
    await page.goto("/services");
    const select = page.locator("select[aria-label='Tenant']");
    await expect(select).toBeVisible();
    const options = select.locator("option");
    await expect(options).toHaveCount(2);
    await expect(select).toHaveValue(TENANT_A_ID);
    await select.selectOption(TENANT_B_ID);
    await expect.poll(() => switchedTenantId).toBe(TENANT_B_ID);
  });
});

test.describe("unauthenticated access", () => {
  test("redirects to login when auth fails", async ({ page }) => {
    await page.route("**/v1/auth/me", (route) =>
      route.fulfill({ status: 401, json: { error: "unauthorized" } })
    );
    await page.route("**/v1/auth/login**", (route) => {
      route.fulfill({ status: 200, body: "login page" });
    });
    await page.goto("/services");
    await page.waitForURL("**/login**", { timeout: 5000 });
  });
});
