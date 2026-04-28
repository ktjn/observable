import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

// ── Fixtures ──────────────────────────────────────────────────────────────────

const TRACE_ID = "aaaa000000000000000000000000001a";

const FIXTURE_TRACE = {
  trace_id: TRACE_ID,
  spans: [
    {
      tenant_id: "00000000-0000-0000-0000-000000000001",
      trace_id: TRACE_ID,
      span_id: "span00000000001a",
      service_name: "checkout",
      operation_name: "POST /order",
      start_time_unix_nano: 1_000_000_000,
      end_time_unix_nano: 6_000_000_000,
      duration_ns: 5_000_000_000,
      status_code: "OK",
    },
    {
      tenant_id: "00000000-0000-0000-0000-000000000001",
      trace_id: TRACE_ID,
      span_id: "span00000000002a",
      service_name: "payments",
      operation_name: "POST /charge",
      start_time_unix_nano: 2_000_000_000,
      end_time_unix_nano: 5_000_000_000,
      duration_ns: 3_000_000_000,
      status_code: "OK",
    },
  ],
};

const EMPTY_LOGS = { logs: [], total: 0, facets: {} };

const FIXTURE_LOGS = {
  logs: [
    {
      tenant_id: "00000000-0000-0000-0000-000000000001",
      log_id: "log-0001",
      timestamp_unix_nano: "1700000000000000000",
      severity_number: 9,
      severity_text: "INFO",
      body: "order received",
      service_name: "checkout",
      resource_attributes: {},
    },
  ],
  total: 1,
  facets: {
    service_name: [{ value: "checkout", count: 1 }],
  },
};

const FIXTURE_ENVIRONMENTS = {
  items: ["local-dev", "prod"],
};

const FIXTURE_SERVICES = {
  items: [
    {
      service_name: "checkout",
      environment: "local-dev",
      request_rate: 12.4,
      error_rate: 0.004,
      p95_latency_ms: 184,
      health_state: "healthy",
      active_alert_count: 0,
      last_deployment_at: null,
    },
    {
      service_name: "payments",
      environment: "local-dev",
      request_rate: 4.9,
      error_rate: 0.021,
      p95_latency_ms: 312,
      health_state: "watch",
      active_alert_count: 1,
      last_deployment_at: null,
    },
  ],
};

const FIXTURE_SERVICE_SUMMARY = {
  service: {
    service_name: "checkout",
    request_rate: 12.5,
    error_rate: 0.025,
    p95_latency_ms: 245,
    health_state: "watch",
    active_alert_count: 2,
    latest_deployment: "checkout@2026.04.21",
  },
};

const FIXTURE_INFRASTRUCTURE_ENTITY = {
  entity_type: "pod",
  entity_id: "prod-cluster/payments/checkout-pod-1",
  display_name: "checkout-pod-1",
  parent_id: "payments",
  parent_display_name: "payments",
  environment: "prod",
  health_state: "watch",
  last_seen_unix_nano: 42,
  related_services: ["checkout"],
  log_rate_per_minute: 8.5,
  error_rate: 0.02,
  restart_count: null,
  cpu_usage: 0.42,
  memory_usage: 0.31,
  disk_usage: null,
  network_io: null,
};

// ── Trace detail waterfall ────────────────────────────────────────────────────

test.describe("trace detail waterfall", () => {
  test.beforeEach(async ({ page }) => {
    await page.route(`**/v1/traces/${TRACE_ID}`, (route) =>
      route.fulfill({ json: FIXTURE_TRACE })
    );
    await page.route("**/v1/logs**", (route) =>
      route.fulfill({ json: EMPTY_LOGS })
    );
  });

  test("has no axe violations", async ({ page }) => {
    await page.goto(`/traces/${TRACE_ID}`);
    await page.waitForSelector("text=POST /order");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});

// ── Log search ────────────────────────────────────────────────────────────────

test.describe("log search", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/v1/logs**", (route) =>
      route.fulfill({ json: FIXTURE_LOGS })
    );
  });

  test("has no axe violations", async ({ page }) => {
    await page.goto("/logs");
    await page.waitForSelector("text=order received");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});

// ── Services catalog ──────────────────────────────────────────────────────────

test.describe("services catalog", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/v1/environments**", (route) =>
      route.fulfill({ json: FIXTURE_ENVIRONMENTS })
    );
    await page.route("**/v1/services**", (route) =>
      route.fulfill({ json: FIXTURE_SERVICES })
    );
  });

  test("has no axe violations", async ({ page }) => {
    await page.goto("/services");
    await page.waitForSelector("text=checkout");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});

// ── Service and infrastructure detail ────────────────────────────────────────

test.describe("service and infrastructure detail", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/v1/services/checkout/summary**", (route) =>
      route.fulfill({ json: FIXTURE_SERVICE_SUMMARY })
    );
    await page.route("**/v1/deployments**", (route) =>
      route.fulfill({ json: { items: [] } })
    );
    await page.route("**/v1/infrastructure**", (route) =>
      route.fulfill({ json: { items: [FIXTURE_INFRASTRUCTURE_ENTITY] } })
    );
    await page.route("**/v1/infrastructure/pod/**", (route) =>
      route.fulfill({
        json: {
          entity: FIXTURE_INFRASTRUCTURE_ENTITY,
          links: {
            logs: "/logs?resource_attr=k8s.pod.name:checkout-pod-1",
            traces: "/traces?resource_attr=k8s.pod.name:checkout-pod-1",
            metrics: "/services/checkout/metrics?resource_attr=k8s.pod.name:checkout-pod-1",
          },
        },
      })
    );
  });

  test("service detail has no axe violations", async ({ page }) => {
    await page.goto("/services/checkout");
    await page.waitForSelector("text=checkout@2026.04.21");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });

  test("infrastructure detail has no axe violations", async ({ page }) => {
    await page.goto("/infrastructure/pod/prod-cluster%2Fpayments%2Fcheckout-pod-1");
    await page.waitForSelector("text=checkout-pod-1");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});

// ── Negative proof ────────────────────────────────────────────────────────────

test("detects injected violation (harness proof)", async ({ page }) => {
  await page.route("**/v1/logs**", (route) =>
    route.fulfill({ json: EMPTY_LOGS })
  );
  await page.goto("/logs");
  await page.waitForSelector("text=No logs found");
  await page.evaluate(() => {
    const img = document.createElement("img");
    img.setAttribute("src", "data:image/gif;base64,R0lGODlhAQABAAD/ACwAAAAAAQABAAACADs=");
    // intentionally no alt — axe image-alt violation
    document.body.appendChild(img);
  });
  const results = await new AxeBuilder({ page }).analyze();
  const imageAltViolation = results.violations.find((v) => v.id === "image-alt");
  expect(imageAltViolation).toBeDefined();
});
