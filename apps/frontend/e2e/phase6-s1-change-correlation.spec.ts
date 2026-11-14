import { test, expect, type Page } from "@playwright/test";

const TENANT_ID = "00000000-0000-0000-0000-000000000001";

async function mockServiceChangeApi(page: Page) {
  const now = Date.now();
  const deployTime = new Date(now - 20 * 60_000).toISOString();
  const flagTime = new Date(now - 10 * 60_000).toISOString();

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
          error_rate: 0.08,
          p95_latency_ms: 420,
          slo_breaching: true,
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
        from: new Date(now - 3600_000).toISOString(),
        to: new Date(now).toISOString(),
        slos: [],
      },
    }),
  );
  await page.route("**/v1/services/checkout/response-time-history**", (route) => {
    const url = new URL(route.request().url());
    const fromIso = url.searchParams.get("from");
    const toIso = url.searchParams.get("to");
    const fromMs = fromIso ? Date.parse(fromIso) : now - 3600_000;
    const toMs = toIso ? Date.parse(toIso) : now;
    const step = Math.round((toMs - fromMs) / 4);
    return route.fulfill({
      json: {
        buckets: [
          { start_ms: fromMs, end_ms: fromMs + step, p50_ms: 50, p95_ms: 190, request_rate: 12 },
          { start_ms: fromMs + step, end_ms: fromMs + 2 * step, p50_ms: 85, p95_ms: 350, request_rate: 14 },
          { start_ms: fromMs + 2 * step, end_ms: fromMs + 3 * step, p50_ms: 110, p95_ms: 420, request_rate: 15 },
          { start_ms: fromMs + 3 * step, end_ms: toMs, p50_ms: 95, p95_ms: 390, request_rate: 13 },
        ],
      },
    });
  });
  await page.route("**/v1/deployments**", (route) =>
    route.fulfill({
      json: {
        items: [
          {
            deployment_id: "dep-142",
            service_name: "checkout",
            service_version: "v1.4.2",
            environment: "prod",
            status: "deployed",
            started_at: deployTime,
            completed_at: deployTime,
            commit_sha: "abc1234",
            deployer: "ci-bot",
          },
        ],
      },
    }),
  );
  await page.route("**/v1/events/changes**", (route) =>
    route.fulfill({
      json: {
        items: [
          {
            change_event_id: "chg-flag-1",
            tenant_id: TENANT_ID,
            event_type: "feature_flag",
            service_name: "checkout",
            environment: "prod",
            title: "Enable express checkout flag",
            description: "Toggled outside release pipeline",
            actor: "oncall",
            occurred_at: flagTime,
          },
        ],
      },
    }),
  );
  await page.route("**/v1/incidents**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/services/checkout/traces**", (route) =>
    route.fulfill({ json: { traces: [] } }),
  );
  await page.route("**/v1/logs**", (route) => route.fulfill({ json: { logs: [] } }));
}

test.describe("Phase 6 S1: Change and release correlation (OBS-11-01)", () => {
  test("shows both recent deployment release and non-release change alongside service regression", async ({
    page,
  }) => {
    await mockServiceChangeApi(page);
    await page.goto("/services/checkout");

    // Service regression state is visible
    await expect(page.getByText("8.0%").or(page.getByText(/SLO Breaching|Degraded|420/i)).first()).toBeVisible();

    // Recent release deployment marker is visible on the performance graph
    await expect(page.locator('[aria-label="Deployment v1.4.2 — deployed"]')).toBeVisible();

    // Non-release change event (feature_flag) is also visible alongside the regression
    await expect(
      page.locator('[aria-label="feature_flag: Enable express checkout flag"]'),
    ).toBeVisible();
  });
});
