import { test, expect, type Page } from "@playwright/test";

const TENANT_ID = "00000000-0000-0000-0000-000000000001";

interface MockOptions {
  errorRate: number;
  p95LatencyMs: number;
  sloBreaching: boolean;
  slos: Array<{
    slo_id: string;
    name: string;
    sli_type: string;
    target_ratio: number;
    target_latency_ms: number | null;
    window_days: number;
    is_active: boolean;
  }>;
}

async function mockServicePerfApi(page: Page, opts: MockOptions) {
  await page.route("**/v1/auth/me", (route) =>
    route.fulfill({
      json: {
        user_id: "00000000-0000-0000-0000-000000000001",
        email: "operator@example.com",
        tenants: [{ tenant_id: TENANT_ID, role: "admin" }],
      },
    }),
  );
  await page.route("**/v1/tenants", (route) =>
    route.fulfill({ json: { tenants: [{ id: TENANT_ID, name: "observable" }] } }),
  );
  await page.route("**/v1/tenants/**/environments", (route) =>
    route.fulfill({ json: { environments: [{ environment: "prod" }] } }),
  );
  await page.route("**/v1/services/checkout/summary**", (route) =>
    route.fulfill({
      json: {
        service: {
          service_name: "checkout",
          request_rate: 12.5,
          request_count: 1200,
          error_rate: opts.errorRate,
          p95_latency_ms: opts.p95LatencyMs,
          slo_breaching: opts.sloBreaching,
          active_alert_count: 1,
          latest_deployment: "v1.4.2",
        },
      },
    }),
  );
  await page.route("**/v1/services/checkout/reliability**", (route) =>
    route.fulfill({
      json: {
        tenant_id: TENANT_ID,
        service_name: "checkout",
        from: new Date(Date.now() - 3600_000).toISOString(),
        to: new Date().toISOString(),
        slos: opts.slos,
      },
    }),
  );
  await page.route("**/v1/services/checkout/response-time-history**", (route) => {
    const url = new URL(route.request().url());
    const fromIso = url.searchParams.get("from");
    const toIso = url.searchParams.get("to");
    const fromMs = fromIso ? Date.parse(fromIso) : Date.now() - 3600_000;
    const toMs = toIso ? Date.parse(toIso) : Date.now();
    const step = Math.round((toMs - fromMs) / 4);
    const isBaseline = fromMs < Date.now() - 2 * 3600_000;
    return route.fulfill({
      json: {
        buckets: isBaseline
          ? [
              { start_ms: fromMs, end_ms: fromMs + step, p50_ms: 40, p95_ms: 140, request_rate: 10 },
              { start_ms: fromMs + step, end_ms: fromMs + 2 * step, p50_ms: 45, p95_ms: 150, request_rate: 11 },
              { start_ms: fromMs + 2 * step, end_ms: fromMs + 3 * step, p50_ms: 42, p95_ms: 145, request_rate: 12 },
              { start_ms: fromMs + 3 * step, end_ms: toMs, p50_ms: 38, p95_ms: 135, request_rate: 9 },
            ]
          : [
              { start_ms: fromMs, end_ms: fromMs + step, p50_ms: 50, p95_ms: 210, request_rate: 14 },
              { start_ms: fromMs + step, end_ms: fromMs + 2 * step, p50_ms: 55, p95_ms: 220, request_rate: 15 },
              { start_ms: fromMs + 2 * step, end_ms: fromMs + 3 * step, p50_ms: 0, p95_ms: 0, request_rate: 0 },
              { start_ms: fromMs + 3 * step, end_ms: toMs, p50_ms: 60, p95_ms: 240, request_rate: 16 },
            ],
      },
    });
  });
  await page.route("**/v1/deployments**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/change-events**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/metrics?*", (route) => route.fulfill({ json: { metrics: [] } }));
  await page.route("**/v1/traces/histogram**", (route) => route.fulfill({ json: { buckets: [] } }));
  await page.route("**/v1/nlq**", (route) => route.fulfill({ json: { rows: [], sql: "", row_count: 0 } }));
}

test.describe("Phase 4 S1: Service performance, demand gaps, and degraded vs unavailable", () => {
  test("OBS-04-01: unassessed without approved latency target, slow when breaching approved target", async ({ page }) => {
    await mockServicePerfApi(page, {
      errorRate: 0.01,
      p95LatencyMs: 180,
      sloBreaching: false,
      slos: [],
    });

    await page.goto("/services/checkout");
    const panel = page.getByTestId("responsiveness-assessment");
    await expect(panel).toContainText("Unassessed — no approved target");
    await expect(panel).not.toContainText("Meeting expectation");

    await mockServicePerfApi(page, {
      errorRate: 0.01,
      p95LatencyMs: 420,
      sloBreaching: true,
      slos: [
        {
          slo_id: "slo-lat-1",
          name: "Checkout P95 Latency",
          sli_type: "latency",
          target_ratio: 0.99,
          target_latency_ms: 200,
          window_days: 30,
          is_active: true,
        },
      ],
    });

    await page.goto("/services/checkout");
    await expect(page.getByTestId("responsiveness-assessment")).toContainText("Slow — breaching target");
  });

  test("OBS-04-02: compare=prev overlays baseline demand and missing demand is shown as gaps", async ({ page }) => {
    await mockServicePerfApi(page, {
      errorRate: 0.01,
      p95LatencyMs: 220,
      sloBreaching: false,
      slos: [],
    });

    await page.goto("/services/checkout?compare=prev");
    const graph = page.getByRole("group", { name: "Service response time and throughput graph" });
    await expect(graph).toBeVisible();
    await expect(graph).toContainText("Req/s Baseline");
    await expect(page.getByTestId("service-histogram-disclosure")).toContainText("missing demand shown as gaps");

    const reqRatePolylines = graph.locator('polyline[data-series-key="request_rate"]');
    await expect(reqRatePolylines).toHaveCount(2);
  });

  test("OBS-04-03: functioning slow service is degraded (never unavailable), total failure is unavailable", async ({ page }) => {
    await mockServicePerfApi(page, {
      errorRate: 0.01,
      p95LatencyMs: 650,
      sloBreaching: false,
      slos: [],
    });

    await page.goto("/services/checkout");
    const stateLabel = page.getByTestId("service-health-state").locator("[data-state]");
    await expect(stateLabel).toHaveAttribute("data-state", "degraded");
    await expect(stateLabel).toContainText("Degraded");
    await expect(stateLabel).not.toContainText("Unavailable");

    await mockServicePerfApi(page, {
      errorRate: 1.0,
      p95LatencyMs: 650,
      sloBreaching: true,
      slos: [],
    });

    await page.goto("/services/checkout");
    const unavailableLabel = page.getByTestId("service-health-state").locator("[data-state]");
    await expect(unavailableLabel).toHaveAttribute("data-state", "unavailable");
    await expect(unavailableLabel).toContainText("Unavailable");
    await expect(unavailableLabel).not.toContainText("Degraded");
  });
});
