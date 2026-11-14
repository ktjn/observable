import { test, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const MOCK_USER = {
  user_id: "00000000-0000-0000-0000-000000000001",
  email: "oncall@example.com",
  tenants: [{ tenant_id: "00000000-0000-0000-0000-000000000001", role: "admin" }],
};

async function mockCommon(page: Page) {
  await page.route("**/v1/auth/me", (route) => route.fulfill({ json: MOCK_USER }));
  await page.route("**/v1/tenants", (route) =>
    route.fulfill({ json: { tenants: [{ id: "00000000-0000-0000-0000-000000000001", name: "observable" }] } }),
  );
  await page.route("**/v1/tenants/**/environments", (route) =>
    route.fulfill({ json: { environments: [{ environment: "prod" }] } }),
  );
  await page.route("**/v1/config", (route) => route.fulfill({ json: {} }));
}

async function mockIncident(
  page: Page,
  options: { failAcknowledge?: boolean; delayAcknowledgeMs?: number } = {},
) {
  let currentStatus = "triggered";
  let ackCalls = 0;
  let resolveCalls = 0;

  await mockCommon(page);
  await page.route("**/v1/incidents/inc-1", (route) =>
    route.fulfill({
      json: {
        id: "inc-1",
        title: "Checkout latency spike",
        severity: "critical",
        status: currentStatus,
        triggered_at: "2026-11-14T07:00:00Z",
        resolved_at: currentStatus === "resolved" ? "2026-11-14T07:15:00Z" : null,
        acknowledged_by: currentStatus === "acknowledged" ? "oncall@example.com" : null,
        impacted_service: "checkout",
        triggered_by_rule_id: null,
        timeline: [],
      },
    }),
  );
  await page.route("**/v1/incidents/inc-1/acknowledge", async (route) => {
    ackCalls += 1;
    if (options.delayAcknowledgeMs) {
      await new Promise((r) => setTimeout(r, options.delayAcknowledgeMs));
    }
    if (options.failAcknowledge) {
      return route.fulfill({ status: 500, json: { error: "upstream timeout" } });
    }
    currentStatus = "acknowledged";
    return route.fulfill({ json: { status: "acknowledged" } });
  });
  await page.route("**/v1/incidents/inc-1/resolve", (route) => {
    resolveCalls += 1;
    currentStatus = "resolved";
    return route.fulfill({ json: { status: "resolved" } });
  });
  await page.route("**/v1/topology/map**", (route) =>
    route.fulfill({ json: { nodes: [], edges: [] } }),
  );

  return {
    getAckCalls: () => ackCalls,
    getResolveCalls: () => resolveCalls,
  };
}

test.describe("Phase 3 S4: Interaction states, action feedback & WCAG 2.2 AA", () => {
  test("OBS-24-03, OBS-24-05, OBS-24-09 & UX-09: confirm dialog, keyboard acknowledge lifecycle, 400% zoom, and WCAG 2.2 AA", async ({
    page,
  }) => {
    const tracker = await mockIncident(page, { delayAcknowledgeMs: 150 });
    await page.goto("/incidents/inc-1?preset=1h");

    // OBS-24-03: Single click on Resolve opens confirmation dialog without mutating.
    await page.getByRole("button", { name: "Resolve" }).click();
    const dialog = page.getByRole("alertdialog", { name: "Resolve incident" });
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("Consequence:");
    expect(tracker.getResolveCalls()).toBe(0);

    // Cancel aborts without mutating.
    await dialog.getByRole("button", { name: "Cancel" }).click();
    await expect(dialog).toBeHidden();
    expect(tracker.getResolveCalls()).toBe(0);

    // OBS-24-09 & UX-09: Keyboard-only Acknowledge shows pending then completed feedback.
    const ackBtn = page.getByRole("button", { name: "Acknowledge" });
    await ackBtn.focus();
    await page.keyboard.press("Enter");
    await expect(page.getByTestId("action-feedback")).toContainText("Acknowledging incident");
    await expect(page.getByTestId("action-feedback")).toContainText("Incident acknowledged");
    await expect(page).toHaveURL(/preset=1h/);

    // OBS-24-05: WCAG 2.2 AA automated check on incident investigation view.
    const axe = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
      .analyze();
    expect(axe.violations).toEqual([]);

    // OBS-24-05 Negative: Operable at 400% zoom (320px CSS viewport width).
    await page.setViewportSize({ width: 320, height: 568 });
    await expect(page.getByRole("button", { name: "Resolve" })).toBeVisible();
    await page.getByRole("button", { name: "Resolve" }).click();
    await page.getByRole("button", { name: "Confirm resolve" }).click();
    expect(tracker.getResolveCalls()).toBe(1);
    await expect(page.getByTestId("action-feedback")).toContainText("Incident resolved");
  });

  test("OBS-24-09 Negative & UX-09 failure: failed acknowledgement is announced as error and never marked completed", async ({
    page,
  }) => {
    await mockIncident(page, { failAcknowledge: true });
    await page.goto("/incidents/inc-1?preset=1h");

    const ackBtn = page.getByRole("button", { name: "Acknowledge" });
    await ackBtn.focus();
    await page.keyboard.press("Enter");

    const feedback = page.getByRole("alert");
    await expect(feedback).toContainText("Acknowledgement failed");
    await expect(feedback).not.toContainText("Incident acknowledged");
    await expect(feedback.getByRole("button", { name: "Retry" })).toBeVisible();
  });

  test("OBS-24-04 & OBS-24-05: failed chart data retrieval shows ErrorState with retry instead of empty chart", async ({
    page,
  }) => {
    let shouldFail = true;
    await mockCommon(page);
    await page.route("**/v1/services/checkout/summary**", (route) =>
      route.fulfill({
        json: {
          service: {
            service_name: "checkout",
            request_rate: 10,
            error_rate: 0.01,
            p95_latency_ms: 120,
            slo_breaching: false,
            active_alert_count: 0,
            latest_deployment: null,
          },
        },
      }),
    );
    await page.route("**/v1/services/checkout/response-time-history**", (route) => {
      if (shouldFail) {
        return route.fulfill({ status: 500, json: { error: "clickhouse unavailable" } });
      }
      const now = Date.now();
      return route.fulfill({
        json: {
          buckets: [
            { start_ms: now - 30 * 60_000, p95_ms: 120, p50_ms: 40, request_rate: 10 },
            { start_ms: now, p95_ms: 140, p50_ms: 45, request_rate: 12 },
          ],
        },
      });
    });
    await page.route("**/v1/deployments**", (route) => route.fulfill({ json: { items: [] } }));
    await page.route("**/v1/events/changes**", (route) => route.fulfill({ json: { items: [] } }));
    await page.route("**/v1/change-events**", (route) => route.fulfill({ json: { items: [] } }));
    await page.route("**/v1/metrics**", (route) => route.fulfill({ json: { metrics: [] } }));
    await page.route("**/v1/nlq**", (route) => route.fulfill({ json: { type: "frame", frame: { schema: [], data: [] } } }));
    await page.route("**/v1/logs**", (route) => route.fulfill({ json: { items: [], total: 0 } }));
    await page.route("**/v1/traces/histogram**", (route) => route.fulfill({ json: { buckets: [] } }));

    await page.goto("/services/checkout");

    const errorAlert = page.getByRole("alert").filter({ hasText: "Failed to load response time history" });
    await expect(errorAlert).toBeVisible();
    await expect(page.getByRole("group", { name: "Service response time and throughput graph" })).toHaveCount(0);

    shouldFail = false;
    await errorAlert.getByRole("button", { name: "Retry" }).click();
    await expect(page.getByRole("group", { name: "Service response time and throughput graph" })).toBeVisible();

    const axe = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21aa", "wcag22aa"])
      .analyze();
    expect(axe.violations).toEqual([]);
  });
});
