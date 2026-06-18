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
    route.fulfill({ json: { environments: [{ environment: "prod" }] } })
  );
}

// ── Fixtures ──────────────────────────────────────────────────────────────────

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
  notification_channels: [],
  auto_trigger_incident: true,
};

const FIXTURE_RULE_OK = {
  rule_id: "rule-2",
  name: "Low Request Rate",
  metric_name: "request_rate",
  operator: "lt",
  threshold: 1.0,
  severity: "warning",
  silenced: false,
  state: "ok",
  firing: false,
  last_fired_at: null,
  notification_channels: [],
  auto_trigger_incident: false,
};

const FIXTURE_RULE_SILENCED = {
  rule_id: "rule-3",
  name: "Memory Pressure",
  metric_name: "memory_usage",
  operator: "gt",
  threshold: 0.9,
  severity: "info",
  silenced: true,
  state: "ok",
  firing: false,
  last_fired_at: null,
  notification_channels: [],
  auto_trigger_incident: false,
};

const FIXTURE_RULES = { items: [FIXTURE_RULE_FIRING, FIXTURE_RULE_OK, FIXTURE_RULE_SILENCED] };

const FIXTURE_SLO = {
  slo_id: "slo-1",
  service_name: "checkout",
  environment: "prod",
  sli_type: "availability",
  target: 0.999,
  window_days: 30,
  burn_rate_fast_threshold: 14.4,
  burn_rate_slow_threshold: 1.0,
  description: "Checkout availability SLO",
  firing: false,
  last_fired_at: null,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

const FIXTURE_SLOS = { items: [FIXTURE_SLO] };
const FIXTURE_CHANNELS = [];

async function mockAlertsApi(page: import("@playwright/test").Page) {
  await page.route("**/v1/alerts/rules**", (route) => route.fulfill({ json: FIXTURE_RULES }));
  await page.route("**/v1/slos**", (route) => route.fulfill({ json: FIXTURE_SLOS }));
  await page.route("**/v1/notification-channels**", (route) => route.fulfill({ json: FIXTURE_CHANNELS }));
}

async function waitForTable(page: import("@playwright/test").Page) {
  await page.waitForSelector("table[aria-label='Alert rules']");
}

// ── Alert summary cards ───────────────────────────────────────────────────────

test.describe("alert summary cards", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await mockAlertsApi(page);
    await page.goto("/alerts");
    await waitForTable(page);
  });

  test("renders metric cards above tabs", async ({ page }) => {
    const summary = page.locator("[aria-label='Alert summary']");
    await expect(summary).toBeVisible();
    await expect(summary.getByText("Total Rules")).toBeVisible();
    await expect(summary.getByText("Firing")).toBeVisible();
    await expect(summary.getByText("Silenced")).toBeVisible();
    await expect(summary.getByText("SLOs")).toBeVisible();
  });

  test("metric cards show correct counts", async ({ page }) => {
    const summary = page.locator("[aria-label='Alert summary']");
    // 3 total rules
    const totalCard = summary.locator("article", { hasText: "Total Rules" });
    await expect(totalCard).toContainText("3");
    // 1 firing
    const firingCard = summary.locator("article", { hasText: "Firing" });
    await expect(firingCard).toContainText("1");
    // 1 SLO
    const sloCard = summary.locator("article", { hasText: "SLOs" });
    await expect(sloCard).toContainText("1");
  });
});

// ── Filter pills ──────────────────────────────────────────────────────────────

test.describe("alert filter pills", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await mockAlertsApi(page);
    await page.goto("/alerts");
    await waitForTable(page);
  });

  test("renders All, Firing, Silenced filter pills", async ({ page }) => {
    const group = page.locator("[aria-label='Filter alert rules']");
    await expect(group.locator("button", { hasText: "All" })).toBeVisible();
    await expect(group.locator("button", { hasText: "Firing" })).toBeVisible();
    await expect(group.locator("button", { hasText: "Silenced" })).toBeVisible();
  });

  test("pills show counts", async ({ page }) => {
    const group = page.locator("[aria-label='Filter alert rules']");
    await expect(group.locator("button", { hasText: "All" })).toContainText("(3)");
    await expect(group.locator("button", { hasText: "Firing" })).toContainText("(1)");
    await expect(group.locator("button", { hasText: "Silenced" })).toContainText("(1)");
  });

  test("Firing pill filters to only firing rules", async ({ page }) => {
    const group = page.locator("[aria-label='Filter alert rules']");
    await group.locator("button", { hasText: "Firing" }).click();

    const table = page.locator("table[aria-label='Alert rules']");
    await expect(table.locator("td", { hasText: "High Error Rate" })).toBeVisible();
    await expect(table.locator("td", { hasText: "Low Request Rate" })).not.toBeVisible();
    await expect(table.locator("td", { hasText: "Memory Pressure" })).not.toBeVisible();
  });

  test("Silenced pill filters to only silenced rules", async ({ page }) => {
    const group = page.locator("[aria-label='Filter alert rules']");
    await group.locator("button", { hasText: "Silenced" }).click();

    const table = page.locator("table[aria-label='Alert rules']");
    await expect(table.locator("td", { hasText: "Memory Pressure" })).toBeVisible();
    await expect(table.locator("td", { hasText: "High Error Rate" })).not.toBeVisible();
  });
});

// ── Row tinting ───────────────────────────────────────────────────────────────

test.describe("alert rule row tinting", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await mockAlertsApi(page);
    await page.goto("/alerts");
    await waitForTable(page);
  });

  test("firing rules have red left border", async ({ page }) => {
    const table = page.locator("table[aria-label='Alert rules']");
    const firingRow = table.locator("tr", { hasText: "High Error Rate" });
    await expect(firingRow).toHaveClass(/border-l-\[var\(--bad\)\]/);
  });

  test("silenced rules have warn border and are dimmed", async ({ page }) => {
    const table = page.locator("table[aria-label='Alert rules']");
    const silencedRow = table.locator("tr", { hasText: "Memory Pressure" });
    await expect(silencedRow).toHaveClass(/border-l-\[var\(--warn\)\]/);
    await expect(silencedRow).toHaveClass(/opacity-60/);
  });

  test("ok rules have transparent border", async ({ page }) => {
    const table = page.locator("table[aria-label='Alert rules']");
    const okRow = table.locator("tr", { hasText: "Low Request Rate" });
    await expect(okRow).toHaveClass(/border-l-transparent/);
  });
});

// ── Severity badge tones ──────────────────────────────────────────────────────

test.describe("severity badge tones", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await mockAlertsApi(page);
    await page.goto("/alerts");
    await waitForTable(page);
  });

  test("critical severity badge is visible", async ({ page }) => {
    const table = page.locator("table[aria-label='Alert rules']");
    const firingRow = table.locator("tr", { hasText: "High Error Rate" });
    await expect(firingRow.getByText("critical")).toBeVisible();
  });

  test("warning severity badge is visible", async ({ page }) => {
    const table = page.locator("table[aria-label='Alert rules']");
    const warnRow = table.locator("tr", { hasText: "Low Request Rate" });
    await expect(warnRow.getByText("warning")).toBeVisible();
  });

  test("info severity badge is visible", async ({ page }) => {
    const table = page.locator("table[aria-label='Alert rules']");
    const infoRow = table.locator("tr", { hasText: "Memory Pressure" });
    await expect(infoRow.getByText("info")).toBeVisible();
  });
});

// ── SLO compliance bar ────────────────────────────────────────────────────────

test.describe("SLO compliance bar", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await mockAlertsApi(page);
    await page.goto("/alerts");
    await page.locator("[role='tab']", { hasText: "SLOs" }).click();
    await page.waitForSelector("article", { state: "visible" });
  });

  test("SLO card shows compliance progressbar", async ({ page }) => {
    const card = page.locator("article", { hasText: "Checkout availability SLO" });
    const bar = card.locator("[role='progressbar']");
    await expect(bar).toBeVisible();
    await expect(bar).toHaveAttribute("aria-label", /SLO target/);
  });

  test("SLO compliance bar shows target percentage", async ({ page }) => {
    const card = page.locator("article", { hasText: "Checkout availability SLO" });
    await expect(card).toContainText("99.9%");
  });
});

// ── Accessibility ─────────────────────────────────────────────────────────────

test("alerts page has no accessibility violations", async ({ page }) => {
  await mockAuth(page);
  await mockAlertsApi(page);
  await page.goto("/alerts");
  await waitForTable(page);

  const results = await new AxeBuilder({ page }).analyze();
  expect(results.violations).toEqual([]);
});
