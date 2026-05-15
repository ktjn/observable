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

const FIXTURE_HOST = {
  entity_type: "host",
  entity_id: "prod-host-1",
  display_name: "prod-host-1",
  parent_id: null,
  parent_display_name: null,
  environment: "prod",
  health_state: "healthy",
  last_seen_unix_nano: 1_700_000_000_000_000_000,
  related_services: ["checkout"],
  log_rate_per_minute: 4.2,
  error_rate: 0.001,
  restart_count: 0,
  cpu_usage: 0.45,
  memory_usage: 0.52,
  disk_usage: 0.30,
  network_io: 1024,
};

const FIXTURE_POD_BREACH = {
  entity_type: "pod",
  entity_id: "prod-cluster/payments/checkout-pod-1",
  display_name: "checkout-pod-1",
  parent_id: "payments",
  parent_display_name: "payments",
  environment: "prod",
  health_state: "breach",
  last_seen_unix_nano: 1_700_000_000_000_000_000,
  related_services: ["checkout", "payments"],
  log_rate_per_minute: 18.0,
  error_rate: 0.05,
  restart_count: 3,
  cpu_usage: 0.87,
  memory_usage: 0.92,
  disk_usage: 0.78,
  network_io: null,
};

const FIXTURE_CONTAINER_WATCH = {
  entity_type: "container",
  entity_id: "prod-cluster/payments/checkout-pod-1/main",
  display_name: "main-container",
  parent_id: "checkout-pod-1",
  parent_display_name: "checkout-pod-1",
  environment: "prod",
  health_state: "watch",
  last_seen_unix_nano: 1_700_000_000_000_000_000,
  related_services: [],
  log_rate_per_minute: null,
  error_rate: null,
  restart_count: 1,
  cpu_usage: 0.65,
  memory_usage: null,
  disk_usage: null,
  network_io: null,
};

const FIXTURE_NAMESPACE = {
  entity_type: "namespace",
  entity_id: "prod-cluster/kube-system",
  display_name: "kube-system",
  parent_id: "prod-cluster",
  parent_display_name: "prod-cluster",
  environment: "prod",
  health_state: "healthy",
  last_seen_unix_nano: 1_700_000_000_000_000_000,
  related_services: [],
  log_rate_per_minute: 0.5,
  error_rate: 0.0,
  restart_count: 0,
  cpu_usage: null,
  memory_usage: null,
  disk_usage: null,
  network_io: null,
};

const NLQ_FRAME_RESPONSE = {
  type: "frame",
  frame: {
    data: [FIXTURE_HOST, FIXTURE_POD_BREACH, FIXTURE_CONTAINER_WATCH, FIXTURE_NAMESPACE],
  },
};

// ── Infrastructure inventory ──────────────────────────────────────────────────

test.describe("infrastructure inventory", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/nlq", (route) =>
      route.fulfill({ json: NLQ_FRAME_RESPONSE })
    );
  });

  test("renders entity type filter pills", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=prod-host-1");

    await expect(page.locator("button", { hasText: "All types" })).toBeVisible();
    await expect(page.locator("button", { hasText: "host" })).toBeVisible();
    await expect(page.locator("button", { hasText: "pod" })).toBeVisible();
    await expect(page.locator("button", { hasText: "container" })).toBeVisible();
    await expect(page.locator("button", { hasText: "namespace" })).toBeVisible();
    await expect(page.locator("button", { hasText: "cluster" })).toBeVisible();
  });

  test("renders health filter pills", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=prod-host-1");

    await expect(page.locator("button", { hasText: "All health" })).toBeVisible();
    await expect(page.locator("button", { hasText: "healthy" })).toBeVisible();
    await expect(page.locator("button", { hasText: "watch" })).toBeVisible();
    await expect(page.locator("button", { hasText: "breach" })).toBeVisible();
  });

  test("renders search input", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=prod-host-1");

    await expect(page.getByPlaceholder("Search entities…")).toBeVisible();
  });

  test("type filter pill shows count for each entity type", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=prod-host-1");

    // Each type pill should show a count badge
    await expect(page.locator("button", { hasText: "host" })).toContainText("(1)");
    await expect(page.locator("button", { hasText: "pod" })).toContainText("(1)");
    await expect(page.locator("button", { hasText: "cluster" })).toContainText("(0)");
  });

  test("clicking type pill filters table to that entity type", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=prod-host-1");

    await page.locator("button", { hasText: "pod" }).click();

    await expect(page.locator("td", { hasText: "checkout-pod-1" })).toBeVisible();
    await expect(page.locator("td", { hasText: "prod-host-1" })).not.toBeVisible();
    await expect(page.locator("td", { hasText: "kube-system" })).not.toBeVisible();
  });

  test("clicking health pill filters table to that health state", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=prod-host-1");

    await page.locator("button", { hasText: "breach" }).click();

    await expect(page.locator("td", { hasText: "checkout-pod-1" })).toBeVisible();
    await expect(page.locator("td", { hasText: "prod-host-1" })).not.toBeVisible();
  });

  test("search input filters table by entity name", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=prod-host-1");

    await page.getByPlaceholder("Search entities…").fill("kube");

    await expect(page.locator("td", { hasText: "kube-system" })).toBeVisible();
    await expect(page.locator("td", { hasText: "prod-host-1" })).not.toBeVisible();
  });

  test("renders utilization bars with percentage labels for cpu, memory, disk", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=prod-host-1");

    // Host row has CPU 45%, memory 52%, disk 30% — check percentage spans exist
    const hostRow = page.locator("tr", { hasText: "prod-host-1" });
    const percentSpans = hostRow.locator("span.tabular-nums");
    await expect(percentSpans.first()).toContainText("%");
    await expect(percentSpans).toHaveCount(3); // cpu, memory, disk
  });

  test("null utilization values render as -- not Unavailable", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=kube-system");

    // Namespace has null cpu/memory/disk — expect -- in those cells
    const nsRow = page.locator("tr", { hasText: "kube-system" });
    await expect(nsRow).toContainText("--");
    await expect(nsRow).not.toContainText("Unavailable");
  });

  test("breach rows have left border accent class", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=checkout-pod-1");

    const breachRow = page.locator("tr", { hasText: "checkout-pod-1" });
    await expect(breachRow).toHaveClass(/border-l-2/);
  });

  test("healthy rows have no breach border", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=prod-host-1");

    const healthyRow = page.locator("tr", { hasText: "prod-host-1" });
    const cls = await healthyRow.getAttribute("class");
    expect(cls ?? "").not.toContain("border-l-2");
  });

  test("column order puts health before related services", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=prod-host-1");

    const headers = page.locator("table thead th");
    const texts = await headers.allTextContents();
    const healthIdx = texts.indexOf("Health");
    const relatedIdx = texts.indexOf("Related services");
    expect(healthIdx).toBeGreaterThan(-1);
    expect(relatedIdx).toBeGreaterThan(healthIdx);
  });

  test("has no axe violations", async ({ page }) => {
    await page.goto("/infrastructure");
    await page.waitForSelector("text=prod-host-1");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});

// ── Infrastructure detail ─────────────────────────────────────────────────────

test.describe("infrastructure detail", () => {
  const HIGH_PRESSURE_ENTITY = {
    ...FIXTURE_POD_BREACH,
    cpu_usage: 0.87,
    memory_usage: 0.92,
    error_rate: 0.05,
  };

  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/infrastructure/pod/**", (route) =>
      route.fulfill({
        json: {
          entity: HIGH_PRESSURE_ENTITY,
          links: {
            logs: "/logs?resource_attr=k8s.pod.name:checkout-pod-1",
            traces: "/traces?resource_attr=k8s.pod.name:checkout-pod-1",
            metrics: "/services/checkout/metrics",
          },
        },
      })
    );
  });

  test("high cpu applies tonal inline style", async ({ page }) => {
    await page.goto("/infrastructure/pod/prod-cluster%2Fpayments%2Fcheckout-pod-1");
    await page.waitForSelector("text=checkout-pod-1");

    const cpuDd = page.locator("xpath=//dt[text()='CPU usage']/following-sibling::dd[1]");
    const styleAttr = await cpuDd.getAttribute("style");
    expect(styleAttr).toContain("var(--bad)");
  });

  test("high error rate applies tonal inline style", async ({ page }) => {
    await page.goto("/infrastructure/pod/prod-cluster%2Fpayments%2Fcheckout-pod-1");
    await page.waitForSelector("text=checkout-pod-1");

    const errorDd = page.locator("xpath=//dt[text()='Error rate']/following-sibling::dd[1]");
    const styleAttr = await errorDd.getAttribute("style");
    expect(styleAttr).toContain("var(--bad)");
  });

  test("has no axe violations", async ({ page }) => {
    await page.goto("/infrastructure/pod/prod-cluster%2Fpayments%2Fcheckout-pod-1");
    await page.waitForSelector("text=checkout-pod-1");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});
