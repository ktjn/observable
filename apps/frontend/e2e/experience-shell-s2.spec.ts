import { test, expect, type Page } from "@playwright/test";

const TENANT_ID = "00000000-0000-0000-0000-000000000001";

async function mockS2Api(page: Page) {
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
  await page.route("**/v1/services/summary**", (route) =>
    route.fulfill({
      json: {
        items: [
          {
            service_name: "checkout",
            request_rate: 12.5,
            request_count: 1200,
            error_rate: 0.08,
            p95_latency_ms: 420,
            slo_breaching: false,
            active_alert_count: 1,
            latest_deployment: "v1.4.2",
          },
        ],
      },
    }),
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
    const now = Date.now();
    return route.fulfill({
      json: {
        buckets: [
          { start_ms: now - 30 * 60_000, end_ms: now - 20 * 60_000, p50_ms: 45, p95_ms: 180, request_rate: 12.5 },
          { start_ms: now - 20 * 60_000, end_ms: now - 10 * 60_000, p50_ms: 0, p95_ms: 0, request_rate: 0 },
          { start_ms: now - 10 * 60_000, end_ms: now, p50_ms: 52, p95_ms: 210, request_rate: 14.2 },
        ],
      },
    });
  });
  await page.route("**/v1/deployments**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/change-events**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/alerts/rules**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/incidents**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/logs**", (route) => route.fulfill({ json: { items: [], total: 0 } }));

  const checkoutService = {
    id: "11111111-1111-4111-8111-111111111111",
    business_name: "Checkout",
    operational_name: "checkout",
    domain: "commerce",
    criticality: "critical",
    lifecycle: "live",
    accountable: true,
  };
  const billingService = {
    id: "22222222-2222-4222-8222-222222222222",
    business_name: "Billing",
    operational_name: "billing",
    domain: "finance",
    criticality: "critical",
    lifecycle: "live",
    accountable: false,
  };

  await page.route("**/v1/review/services**", (route) => {
    const url = new URL(route.request().url());
    const from = url.searchParams.get("from");
    const to = url.searchParams.get("to");
    const isHistorical = Boolean(from && to);

    return route.fulfill({
      json: {
        confidence: {
          confidence: "full",
          affected: [],
          functions: [],
          checked_at: "2026-11-14T07:00:00Z",
        },
        period: isHistorical
          ? {
              as_of: to,
              from,
              to,
              historical: true,
              definitions_as_of: to,
              freshness_seconds: 60,
            }
          : { as_of: "2026-11-14T07:00:00Z", freshness_seconds: 15 },
        items: [
          {
            service: checkoutService,
            live: true,
            health: {
              state: isHistorical ? "unavailable" : "degraded",
              evidence_status: "current",
              evaluated_at: "2026-11-14T07:00:00Z",
              last_evidence_at: "2026-11-14T06:59:45Z",
            },
            expectation: { status: "breached", reason: "High error rate", objectives: [] },
            issues: { status: "current", open: 2 },
            ...(isHistorical
              ? {
                  history: {
                    transitions: [
                      {
                        at: from,
                        from_state: "normal",
                        to_state: "unavailable",
                        evidence_status: "current",
                        rule_id: "default",
                        rule_revision: 2,
                      },
                    ],
                    interruptions: [
                      { started_at: from, ended_at: to, worst_state: "unavailable" },
                    ],
                  },
                }
              : {}),
          },
          {
            service: billingService,
            live: true,
            health: {
              state: "normal",
              evidence_status: "current",
              evaluated_at: "2026-11-14T07:00:00Z",
              last_evidence_at: "2026-11-14T06:59:45Z",
            },
            expectation: { status: "compliant", reason: "Within target", objectives: [] },
            issues: { status: "current", open: 0 },
          },
        ],
        total: 2,
      },
    });
  });

  await page.route("**/v1/overview/services**", (route) =>
    route.fulfill({
      json: {
        scope: { environment_class: "production", context: "business_critical" },
        period: { as_of: "2026-11-14T07:00:00Z", freshness_seconds: 15 },
        confidence: {
          confidence: "full",
          affected: [],
          functions: [],
          checked_at: "2026-11-14T07:00:00Z",
        },
        items: [
          {
            service: checkoutService,
            live: true,
            state: "degraded",
            freshness: { status: "current", last_evidence_at: "2026-11-14T06:59:45Z", age_seconds: 15, bound_seconds: 300 },
            issues: { status: "current", open: 2 },
            fit_for_decision: true,
            unfit_reasons: [],
          },
          {
            service: billingService,
            live: true,
            state: "normal",
            freshness: { status: "current", last_evidence_at: "2026-11-14T06:59:45Z", age_seconds: 15, bound_seconds: 300 },
            issues: { status: "current", open: 0 },
            fit_for_decision: true,
            unfit_reasons: [],
          },
        ],
        total: 2,
      },
    }),
  );
}

test.describe("Phase 3 S2: Service reporting & scoped summary", () => {
  test("OBS-13-01: accountable services and critical issues remain visible despite personal filters", async ({ page }) => {
    await mockS2Api(page);
    await page.goto("/");

    const criticalTable = page.getByRole("table", { name: "Critical services" });
    await expect(criticalTable).toContainText("Checkout");
    await expect(criticalTable).toContainText("Accountable");
    await expect(criticalTable).toContainText("commerce");

    // Apply a personal filter that excludes Checkout from the table.
    await page.getByLabel("Filter critical services by area or name").fill("finance");
    await expect(criticalTable).toContainText("Billing");
    await expect(criticalTable).not.toContainText("Checkout");

    // Critical issue for the accountable service is never hidden by the personal filter.
    const criticalBanner = page.getByTestId("accountable-critical-issues");
    await expect(criticalBanner).toBeVisible();
    await expect(criticalBanner).toContainText("Checkout");
    await expect(criticalBanner).toContainText("expectation breached");

    // Also verify Service Overview pins hidden critical accountable issues when filtered by state=normal.
    await page.goto("/catalog?view=overview&state=normal");
    await expect(page.getByTestId("overview-accountable-critical-issues")).toContainText("Checkout");
    await expect(page.getByTestId("overview-accountable-critical-issues")).toContainText("2 open issues");
  });

  test("OBS-13-02: monthly service review reproduces past period and service histogram discloses period and partial coverage", async ({ page }) => {
    await mockS2Api(page);
    await page.goto("/catalog?view=review");

    const periodSelect = page.getByLabel("Reporting period");
    const firstPastMonth = await periodSelect.locator("option").nth(1).getAttribute("value");
    expect(firstPastMonth).toBeTruthy();

    await periodSelect.selectOption(firstPastMonth!);
    const disclosure = page.getByTestId("review-period-disclosure");
    await expect(disclosure).toBeVisible();
    await expect(disclosure).toContainText("Reproduced historical report");
    await expect(page.getByRole("table", { name: "Service review" })).toContainText("1 transition(s)");

    // Service detail histogram discloses evaluated period and partial coverage.
    await page.goto("/services/checkout");
    const histDisclosure = page.getByTestId("service-histogram-disclosure");
    await expect(histDisclosure).toBeVisible();
    await expect(histDisclosure).toContainText("Evaluated period:");
    await expect(histDisclosure).toContainText("Partial coverage: 2 of 3 intervals contain observed traffic");
  });
});
