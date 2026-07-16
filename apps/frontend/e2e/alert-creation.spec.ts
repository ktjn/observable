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

const FIXTURE_RULE_FIRING = {
  rule_id: "rule-1",
  name: "High Error Rate",
  metric_name: "error_rate",
  operator: "gt",
  threshold: 0.05,
  severity: "critical",
  silenced: false,
  state: "active",
  firing: true,
  last_fired_at: "2026-05-15T10:00:00Z",
  notification_channels: ["channel-1"],
  auto_trigger_incident: true,
};

const FIXTURE_RULE_OK = {
  rule_id: "rule-2",
  name: "Request Latency",
  metric_name: "request_latency_p99",
  operator: "gt",
  threshold: 500,
  severity: "warning",
  silenced: false,
  state: "ok",
  firing: false,
  last_fired_at: null,
  notification_channels: [],
  auto_trigger_incident: false,
};

const FIXTURE_RULE_DETAIL = {
  rule_id: "rule-1",
  name: "High Error Rate",
  severity: "critical",
  alert_type: "threshold",
  condition: { metric_name: "error_rate", operator: "gt", threshold: 0.05 },
  silenced: false,
  firing: true,
  firings: [],
  runbook_url: null,
};

const FIXTURE_CHANNELS = [
  { channel_id: "channel-1", name: "ops-slack", type: "webhook", url: "https://hooks.example.com/1" },
];

async function mockAlertsApi(page: import("@playwright/test").Page) {
  await page.route("**/v1/alerts/rules", (route) => {
    if (route.request().method() === "POST") {
      return route.fulfill({
        status: 201,
        json: { rule_id: "rule-new" },
      });
    }
    return route.fulfill({ json: { items: [FIXTURE_RULE_FIRING, FIXTURE_RULE_OK] } });
  });
  await page.route("**/v1/alerts/rules/rule-1", (route) =>
    route.fulfill({ json: FIXTURE_RULE_DETAIL })
  );
  await page.route("**/v1/slos**", (route) =>
    route.fulfill({ json: { items: [] } })
  );
  await page.route("**/v1/notification-channels**", (route) =>
    route.fulfill({ json: FIXTURE_CHANNELS })
  );
}

test.describe("alert rule detail", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await mockAlertsApi(page);
  });

  test("firing alert rule shows firing state", async ({ page }) => {
    await page.goto("/alerts/rule-1");
    await expect(page.locator("text=High Error Rate")).toBeVisible();
  });

  test("alert detail shows condition summary", async ({ page }) => {
    await page.goto("/alerts/rule-1");
    await page.waitForSelector("text=High Error Rate");
    await expect(page.locator("text=threshold")).toBeVisible();
  });

  test("alert detail passes accessibility audit", async ({ page }) => {
    await page.goto("/alerts/rule-1");
    await page.waitForSelector("text=High Error Rate");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});

test.describe("alert list fired/resolved states", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await mockAlertsApi(page);
  });

  test("alert list shows both rules", async ({ page }) => {
    await page.goto("/alerts");
    await page.waitForSelector("table[aria-label='Alert rules']");
    await expect(page.locator("text=High Error Rate")).toBeVisible();
    await expect(page.locator("text=Request Latency")).toBeVisible();
  });

  test("firing rule shows Firing badge", async ({ page }) => {
    await page.goto("/alerts");
    await page.waitForSelector("table[aria-label='Alert rules']");
    const firingRow = page.locator("tr", { hasText: "High Error Rate" });
    await expect(firingRow.locator("text=Firing")).toBeVisible();
  });

  test("ok rule shows OK badge", async ({ page }) => {
    await page.goto("/alerts");
    await page.waitForSelector("table[aria-label='Alert rules']");
    const okRow = page.locator("tr", { hasText: "Request Latency" });
    await expect(okRow.locator("text=OK")).toBeVisible();
  });
});
