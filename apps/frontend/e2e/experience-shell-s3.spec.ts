import { test, expect, type Page } from "@playwright/test";

const TENANT_ID = "00000000-0000-0000-0000-000000000001";

async function mockS3Api(page: Page, options: { unauthorizedTraces?: boolean } = {}) {
  await page.route("**/v1/auth/me", (route) =>
    route.fulfill({
      json: {
        user_id: "00000000-0000-0000-0000-000000000001",
        email: "responder@example.com",
        tenants: [{ tenant_id: TENANT_ID, role: "admin" }],
      },
    }),
  );
  await page.route("**/v1/tenants", (route) =>
    route.fulfill({ json: { tenants: [{ id: TENANT_ID, name: "observable" }] } }),
  );
  await page.route("**/v1/tenants/**/environments", (route) =>
    route.fulfill({ json: { environments: [{ environment: "prod" }, { environment: "staging" }] } }),
  );
  await page.route("**/v1/services/checkout/summary**", (route) =>
    route.fulfill({
      json: {
        service: {
          service_name: "checkout",
          request_rate: 12.5,
          request_count: 1200,
          error_rate: 0.08,
          p95_latency_ms: 420,
          slo_breaching: false,
          active_alert_count: 1,
          latest_deployment: "v1.4.2",
        },
      },
    }),
  );
  await page.route("**/v1/services/checkout/response-time-history**", (route) => {
    const url = new URL(route.request().url());
    const fromIso = url.searchParams.get("from");
    const toIso = url.searchParams.get("to");
    const fromMs = fromIso ? Date.parse(fromIso) : Date.now() - 3600_000;
    const toMs = toIso ? Date.parse(toIso) : Date.now();
    const midMs = Math.round((fromMs + toMs) / 2);
    // Return partial coverage (1 empty bucket) so baseline/current missing-data labeling is exercised.
    return route.fulfill({
      json: {
        buckets: [
          { start_ms: fromMs, end_ms: midMs, p50_ms: 45, p95_ms: 180, request_rate: 12.5 },
          { start_ms: midMs, end_ms: toMs, p50_ms: 0, p95_ms: 0, request_rate: 0 },
        ],
      },
    });
  });
  await page.route("**/v1/deployments**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/change-events**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/metrics?*", (route) =>
    route.fulfill({
      json: {
        metrics: [
          {
            tenant_id: "default",
            metric_name: "http.server.request.count",
            metric_type: "Sum",
            unit: "count",
            description: "HTTP requests",
            service_name: "checkout",
            environment: "prod",
            series_count: 1,
          },
        ],
      },
    }),
  );
  await page.route("**/v1/metrics/points**", (route) => {
    const nowNs = BigInt(Date.now()) * 1_000_000n;
    return route.fulfill({
      json: {
        points: [
          { time_unix_nano: String(nowNs - 60_000_000_000n), value_int: 10 },
          { time_unix_nano: String(nowNs), value_int: 20 },
        ],
      },
    });
  });
  await page.route("**/v1/traces/histogram**", (route) => {
    const now = Date.now();
    return route.fulfill({
      json: {
        buckets: [
          { start_ms: now - 30 * 60_000, end_ms: now - 15 * 60_000, count: 4 },
          { start_ms: now - 15 * 60_000, end_ms: now, count: 2 },
        ],
      },
    });
  });
  await page.route("**/v1/nlq**", (route) => {
    if (options.unauthorizedTraces) {
      return route.fulfill({ status: 403, json: { error: "unauthorized" } });
    }
    const nowNs = BigInt(Date.now()) * 1_000_000n;
    return route.fulfill({
      json: {
        type: "frame",
        frame: {
          schema: [],
          data: [
            {
              trace_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
              root_service: "checkout",
              root_operation: "POST /orders",
              status_code: "ERROR",
              start_time_unix_nano: String(nowNs - 5_000_000_000n),
              duration_ms: 120,
              attributes: { channel: "web" },
            },
            {
              trace_id: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
              root_service: "checkout",
              root_operation: "POST /orders",
              status_code: "OK",
              start_time_unix_nano: String(nowNs - 2_000_000_000n),
              duration_ms: 45,
              attributes: { channel: "mobile" },
            },
          ],
        },
      },
    });
  });
  await page.route("**/v1/logs**", (route) => route.fulfill({ json: { items: [], total: 0 } }));
}

test.describe("Phase 3 S3: Visual exploration & baseline comparison", () => {
  test("OBS-22-01 & OBS-22-05 & UX-02: baseline comparison, period-length mismatch, units/aggregation, and pinned refresh", async ({ page }) => {
    await mockS3Api(page);
    await page.goto("/services/checkout");

    const graph = page.getByRole("group", { name: "Service response time and throughput graph" });
    await expect(graph).toBeVisible();
    await expect(graph).toContainText("P95 (p95 · ms)");
    await expect(graph).toContainText("Req/s (rate · rps)");
    await expect(graph.getByTestId("chart-time-context")).toBeVisible();

    // Activate baseline comparison.
    await page.getByLabel("Baseline comparison").selectOption("prev");
    const banner = page.getByTestId("baseline-comparison-banner");
    await expect(banner).toBeVisible();
    await expect(banner).toContainText("Baseline has missing data");
    await expect(graph).toContainText("P95 Baseline (p95 · ms)");

    // Different-length custom baseline + pinned interval inspection (ux-02-refresh).
    const now = Date.now();
    await page.goto(
      `/services/checkout?from=${now - 3600_000}&to=${now}&compare=custom&compareFrom=${now - 3 * 3600_000}&compareTo=${now - 3600_000}`,
    );
    await expect(page.getByTestId("baseline-comparison-banner")).toContainText("Period length differs");
    await expect(page.getByTestId("service-histogram-disclosure")).toContainText("Inspection pinned");
    await expect(page.getByRole("button", { name: "Refresh now" })).toBeVisible();

    // Aggregation change on metrics workspace updates legend & never labels rate as count.
    await page.goto("/services/checkout/metrics");
    const metricGraph = page.getByRole("group", { name: "Graph for http.server.request.count" });
    await expect(metricGraph).toContainText("http.server.request.count (raw · count)");
    await page.getByLabel("Metric aggregation").selectOption("rate");
    await expect(metricGraph).toContainText("http.server.request.count (rate · 1/s)");
    await expect(metricGraph).not.toContainText("(rate · count)");
  });

  test("OBS-22-02, OBS-22-03 & OBS-22-11: authorized facets, URL manipulation guard, and withheld contributing evidence", async ({ page }) => {
    await mockS3Api(page);
    await page.goto("/traces?dimension=secret_pii");

    await expect(page.getByTestId("restricted-filter-notice")).toContainText('Restricted filter dimension "secret_pii"');
    await expect(page.getByTestId("trace-evidence-disclosure")).toContainText("Units: count");

    // Authorized facet refinement preserves time range.
    const facets = page.getByRole("complementary", { name: "Trace facets" });
    await expect(facets).toContainText("channel");
    await facets.getByRole("button", { name: /web/ }).click();
    await expect(page.getByTestId("trace-evidence-disclosure")).toContainText("Channel: web");

    // Unauthorized contributing evidence is explicitly stated as withheld.
    await mockS3Api(page, { unauthorizedTraces: true });
    await page.goto("/traces");
    await expect(page.getByText("Contributing evidence withheld")).toBeVisible();
  });
});
