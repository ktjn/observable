import { test, expect } from "@playwright/test";

// ── Shared mocks ──────────────────────────────────────────────────────────────

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

const T_NS = 1_700_000_000_000_000_000;
const TRACE_ID = "aaaa000000000000000000000000001a";

// ── Sidebar navigation ────────────────────────────────────────────────────────

test.describe("sidebar navigation", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    // stub all data endpoints so pages can render without backend
    await page.route("**/v1/nlq", (route) => route.fulfill({ json: { type: "frame", frame: { data: [] } } }));
    await page.route("**/v1/tenants/**/traces/histogram**", (route) => route.fulfill({ json: { buckets: [] } }));
    await page.route("**/v1/tenants/**/logs/histogram**", (route) => route.fulfill({ json: { buckets: [] } }));
    await page.route("**/v1/services/summary**", (route) => route.fulfill({ json: { items: [] } }));
    await page.route("**/v1/services", (route) => route.fulfill({ json: { items: [] } }));
    await page.route("**/v1/topology**", (route) => route.fulfill({ json: { edges: [] } }));
    await page.route("**/v1/alerts/rules**", (route) => route.fulfill({ json: { items: [] } }));
    await page.route("**/v1/slos**", (route) => route.fulfill({ json: { items: [] } }));
    await page.route("**/v1/notification-channels**", (route) => route.fulfill({ json: [] }));
    await page.route("**/v1/dashboards", (route) => route.fulfill({ json: { items: [] } }));
  });

  test("clicking Traces in sidebar navigates to /traces", async ({ page }) => {
    await page.goto("/logs");
    await page.waitForSelector("text=Logs");
    await page.locator('a[href="/traces"]').click();
    await expect(page).toHaveURL(/\/traces/);
    await page.screenshot({ path: "e2e/screenshots/nav-sidebar-traces.png", fullPage: true });
  });

  test("clicking Logs in sidebar navigates to /logs", async ({ page }) => {
    await page.goto("/traces");
    await page.waitForSelector("text=Traces");
    await page.locator('a[href="/logs"]').click();
    await expect(page).toHaveURL(/\/logs/);
    await page.screenshot({ path: "e2e/screenshots/nav-sidebar-logs.png", fullPage: true });
  });

  test("clicking Infrastructure in sidebar navigates to /infrastructure", async ({ page }) => {
    await page.goto("/traces");
    await page.waitForSelector("text=Traces");
    await page.locator('a[href="/infrastructure"]').click();
    await expect(page).toHaveURL(/\/infrastructure/);
    await page.screenshot({ path: "e2e/screenshots/nav-sidebar-infrastructure.png", fullPage: true });
  });

  test("clicking Dashboards in sidebar navigates to /dashboards", async ({ page }) => {
    await page.goto("/traces");
    await page.waitForSelector("text=Traces");
    await page.locator('a[href="/dashboards"]').click();
    await expect(page).toHaveURL(/\/dashboards/);
    await page.screenshot({ path: "e2e/screenshots/nav-sidebar-dashboards.png", fullPage: true });
  });

  test("clicking Alerts & SLOs in sidebar navigates to /alerts", async ({ page }) => {
    await page.goto("/traces");
    await page.waitForSelector("text=Traces");
    await page.locator('a[href="/alerts"]').click();
    await expect(page).toHaveURL(/\/alerts/);
    await page.screenshot({ path: "e2e/screenshots/nav-sidebar-alerts.png", fullPage: true });
  });
});

// ── Trace row → detail drilldown ──────────────────────────────────────────────

test.describe("trace drilldown", () => {
  test("clicking a trace ID link navigates to trace detail", async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/nlq", (route) =>
      route.fulfill({
        json: {
          type: "frame",
          frame: {
            data: [
              { trace_id: TRACE_ID, root_service: "checkout", root_operation: "GET /order", duration_ms: 150, status_code: "ERROR", environment: "prod", start_time_unix_nano: T_NS },
            ],
          },
        },
      })
    );
    await page.route("**/v1/tenants/**/traces/histogram**", (route) => route.fulfill({ json: { buckets: [] } }));
    await page.route(`**/v1/traces/${TRACE_ID}`, (route) =>
      route.fulfill({
        json: {
          trace_id: TRACE_ID,
          spans: [
            { tenant_id: "00000000-0000-0000-0000-000000000001", trace_id: TRACE_ID, span_id: "span001", service_name: "checkout", operation_name: "GET /order", start_time_unix_nano: 1_000_000_000, end_time_unix_nano: 6_000_000_000, duration_ns: 5_000_000_000, status_code: "ERROR" },
            { tenant_id: "00000000-0000-0000-0000-000000000001", trace_id: TRACE_ID, span_id: "span002", service_name: "payments", operation_name: "POST /charge", start_time_unix_nano: 2_000_000_000, end_time_unix_nano: 5_000_000_000, duration_ns: 3_000_000_000, status_code: "OK" },
          ],
        },
      })
    );
    await page.route("**/v1/logs**", (route) => route.fulfill({ json: { logs: [], total: 0, facets: {} } }));

    await page.goto("/traces");
    await page.waitForSelector('[aria-label="Trace results"]');
    await page.locator('[aria-label="Trace results"] tbody tr').first().locator("a").click();
    await expect(page).toHaveURL(new RegExp(TRACE_ID));
    await page.waitForSelector("text=GET /order");
    await page.screenshot({ path: "e2e/screenshots/nav-trace-detail.png", fullPage: true });
  });
});

// ── Service row → detail drilldown ───────────────────────────────────────────

test.describe("service drilldown", () => {
  test("clicking a service name navigates to service detail", async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/services/summary**", (route) =>
      route.fulfill({
        json: {
          items: [
            { service_name: "checkout", request_rate: 42.5, error_rate: 0.001, p95_latency_ms: 85, health_state: "healthy", active_alert_count: 0, latest_deployment: "v1.4.2" },
          ],
        },
      })
    );
    await page.route("**/v1/services", (route) => route.fulfill({ json: { items: ["checkout"] } }));
    await page.route("**/v1/topology**", (route) => route.fulfill({ json: { edges: [] } }));
    await page.route("**/v1/services/checkout/summary**", (route) =>
      route.fulfill({
        json: {
          service: { service_name: "checkout", request_rate: 42.5, error_rate: 0.001, p95_latency_ms: 85, health_state: "healthy", active_alert_count: 0, latest_deployment: "v1.4.2" },
        },
      })
    );
    await page.route("**/v1/deployments**", (route) => route.fulfill({ json: { items: [] } }));
    await page.route("**/v1/infrastructure**", (route) => route.fulfill({ json: { items: [] } }));
    await page.route("**/v1/metrics**", (route) => route.fulfill({ json: { series: [] } }));
    await page.route("**/v1/alerts/rules**", (route) => route.fulfill({ json: { items: [] } }));

    await page.goto("/services");
    await page.waitForSelector("table[aria-label='Service catalog']");
    await page.locator("table[aria-label='Service catalog'] a", { hasText: "checkout" }).click();
    await expect(page).toHaveURL(/\/services\/checkout/);
    await page.screenshot({ path: "e2e/screenshots/nav-service-detail.png", fullPage: true });
  });
});

// ── Services: List ↔ Topology toggle ─────────────────────────────────────────

test.describe("services view toggle", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/services/summary**", (route) =>
      route.fulfill({
        json: {
          items: [
            { service_name: "checkout", request_rate: 42.5, error_rate: 0.001, p95_latency_ms: 85, health_state: "healthy", active_alert_count: 0, latest_deployment: "v1.4.2" },
          ],
        },
      })
    );
    await page.route("**/v1/services", (route) => route.fulfill({ json: { items: ["checkout"] } }));
    await page.route("**/v1/topology**", (route) =>
      route.fulfill({ json: { edges: [{ caller: "checkout", callee: "payments", request_count: 1000, error_rate: 0.02, p95_latency_ms: 45 }] } })
    );
  });

  test("switching to Topology view shows the canvas", async ({ page }) => {
    await page.goto("/services");
    await page.waitForSelector("table[aria-label='Service catalog']");
    await page.locator("button", { hasText: "Topology" }).click();
    await expect(page.locator("[data-testid='topology-background']")).toBeVisible();
    await page.screenshot({ path: "e2e/screenshots/nav-services-topology.png", fullPage: true });
  });

  test("switching back to List view restores the table", async ({ page }) => {
    await page.goto("/services");
    await page.waitForSelector("table[aria-label='Service catalog']");
    await page.locator("button", { hasText: "Topology" }).click();
    await expect(page.locator("[data-testid='topology-background']")).toBeVisible();
    await page.locator("button", { hasText: "List" }).click();
    await expect(page.locator("table[aria-label='Service catalog']")).toBeVisible();
    await page.screenshot({ path: "e2e/screenshots/nav-services-list-restored.png", fullPage: true });
  });
});

// ── Alerts: tab navigation ────────────────────────────────────────────────────

test.describe("alerts tab navigation", () => {
  test.beforeEach(async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/alerts/rules**", (route) =>
      route.fulfill({
        json: {
          items: [
            { rule_id: "r1", name: "High Error Rate", metric_name: "error_rate", operator: "gt", threshold: 0.05, severity: "critical", silenced: false, state: "active", firing: true, last_fired_at: "2026-05-15T10:00:00Z", notification_channels: [], auto_trigger_incident: true },
          ],
        },
      })
    );
    await page.route("**/v1/slos**", (route) =>
      route.fulfill({
        json: {
          items: [
            { slo_id: "slo-1", service_name: "checkout", environment: "prod", sli_type: "availability", target: 0.999, window_days: 30, burn_rate_fast_threshold: 14.4, burn_rate_slow_threshold: 1.0, description: "Checkout availability SLO", firing: false, last_fired_at: null, created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z" },
          ],
        },
      })
    );
    await page.route("**/v1/notification-channels**", (route) => route.fulfill({ json: [] }));
  });

  test("clicking SLOs tab shows SLO content", async ({ page }) => {
    await page.goto("/alerts");
    await page.waitForSelector("table[aria-label='Alert rules']");
    await page.getByRole("tab", { name: "SLOs" }).click();
    await expect(page.locator("text=Checkout availability SLO")).toBeVisible();
    await page.screenshot({ path: "e2e/screenshots/nav-alerts-slos-tab.png", fullPage: true });
  });

  test("clicking Notification Channels tab shows channels content", async ({ page }) => {
    await page.goto("/alerts");
    await page.waitForSelector("table[aria-label='Alert rules']");
    await page.getByRole("tab", { name: "Notification Channels" }).click();
    await page.screenshot({ path: "e2e/screenshots/nav-alerts-channels-tab.png", fullPage: true });
  });

  test("clicking back to Alert Rules tab restores the table", async ({ page }) => {
    await page.goto("/alerts");
    await page.waitForSelector("table[aria-label='Alert rules']");
    await page.getByRole("tab", { name: "SLOs" }).click();
    await expect(page.locator("text=Checkout availability SLO")).toBeVisible();
    await page.getByRole("tab", { name: "Alert Rules" }).click();
    await expect(page.locator("table[aria-label='Alert rules']")).toBeVisible();
    await page.screenshot({ path: "e2e/screenshots/nav-alerts-rules-restored.png", fullPage: true });
  });
});

// ── Dashboard: Open card → detail ────────────────────────────────────────────

test.describe("dashboard navigation", () => {
  test("clicking Open on a dashboard card navigates to dashboard detail", async ({ page }) => {
    await mockAuth(page);
    await page.route("**/v1/dashboards", (route) =>
      route.fulfill({
        json: {
          items: [
            { dashboard_id: "dash-1", name: "Checkout Overview", panels: [], created_at: "2026-05-05T00:00:00Z" },
          ],
        },
      })
    );
    await page.route("**/v1/dashboards/dash-1", (route) =>
      route.fulfill({
        json: { dashboard_id: "dash-1", name: "Checkout Overview", panels: [], created_at: "2026-05-05T00:00:00Z" },
      })
    );
    await page.route("**/v1/nlq", (route) => route.fulfill({ json: { type: "frame", frame: { data: [] } } }));

    await page.goto("/dashboards");
    await page.waitForSelector("[aria-label='Dashboard cards']");
    await page.locator("[aria-label='Dashboard cards'] a", { hasText: "Open" }).first().click();
    await expect(page).toHaveURL(/\/dashboards\/dash-1/);
    await page.screenshot({ path: "e2e/screenshots/nav-dashboard-detail.png", fullPage: true });
  });
});

// ── Panel overflow regression ─────────────────────────────────────────────────

test.describe("panel overflow (regression)", () => {
  test("log context panel stays within viewport when tall", async ({ page }) => {
    await mockAuth(page);
    // Mock a log with many resource attributes to produce a tall panel
    const manyAttrs: Record<string, string> = {};
    for (let i = 0; i < 30; i++) manyAttrs[`resource.key.${i}`] = `value-${i}`;
    await page.route("**/v1/nlq", (route) =>
      route.fulfill({
        json: {
          type: "frame",
          frame: {
            data: [
              {
                log_id: "log-overflow",
                timestamp_unix_nano: T_NS,
                observed_timestamp_unix_nano: T_NS,
                severity_number: 9,
                body: "test log",
                service_name: "checkout",
                environment: "prod",
                host_id: "h1",
                trace_id: null,
                span_id: null,
                fingerprint: null,
                attributes: {},
                resource_attributes: manyAttrs,
              },
            ],
          },
        },
      })
    );
    await page.route("**/v1/tenants/**/logs/histogram**", (route) =>
      route.fulfill({ json: { buckets: [] } })
    );
    await page.goto("/logs");
    await page.waitForSelector('[aria-label="Log results"]');
    await page.locator('[aria-label="Log results"] tbody tr').first().click();
    await page.waitForSelector('[aria-label="Selected log context"]');
    // The aside must not extend beyond the viewport — scrollHeight of the panel
    // should be <= the viewport height (any overflow must be inside the panel's
    // own scroll, not the page scroll).
    const pageScrollHeight = await page.evaluate(() => document.documentElement.scrollHeight);
    const viewportHeight = await page.evaluate(() => window.innerHeight);
    expect(pageScrollHeight).toBeLessThanOrEqual(viewportHeight + 20); // 20px tolerance
    await page.screenshot({ path: "e2e/screenshots/panel-log-overflow-BEFORE.png", fullPage: true });
  });
});
