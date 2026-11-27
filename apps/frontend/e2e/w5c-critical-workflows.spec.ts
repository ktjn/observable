// W5c (docs/assurance-relocations.md): OBS-19-04 accessibility of the critical incident
// workflows against WCAG 2.2 AA, and OBS-19-05 one incident timeline read from two time zones.
// Runs against the production bundle (FRONTEND_PREBUILT=1) with mocked API routes.
import { test, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const TENANT_ID = "00000000-0000-0000-0000-000000000001";
const INCIDENT_ID = "11111111-1111-1111-1111-111111111111";
const WCAG_22_AA = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

const TIMELINE = [
  { event_time: "2026-11-14T23:30:00Z", event_type: "triggered", actor: "alert-evaluator", message: "Error rate > 5%" },
  { event_time: "2026-11-14T23:41:07Z", event_type: "comment", actor: "payments@example.com", message: "Card retries paused" },
  { event_time: "2026-11-15T00:05:59Z", event_type: "comment", actor: "platform@example.com", message: "Failover complete" },
];

async function mockShell(page: Page) {
  await page.route("**/v1/auth/me", (route) =>
    route.fulfill({
      json: { user_id: TENANT_ID, email: "responder@example.com", tenants: [{ tenant_id: TENANT_ID, role: "admin" }] },
    }),
  );
  await page.route("**/v1/tenants", (route) =>
    route.fulfill({ json: { tenants: [{ id: TENANT_ID, name: "observable" }] } }),
  );
  await page.route("**/v1/tenants/**/environments", (route) =>
    route.fulfill({ json: { environments: [{ environment: "prod" }] } }),
  );
  await page.route("**/v1/environments**", (route) => route.fulfill({ json: { items: ["prod"] } }));
  await page.route("**/v1/topology**", (route) => route.fulfill({ json: { edges: [] } }));
  await page.route("**/v1/deployments**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/events/changes**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/logs**", (route) => route.fulfill({ json: { logs: [], total: 0, facets: {} } }));
}

/** Incident list and detail with stateful acknowledge, status and resolve. Returns the mutation log. */
async function mockIncidents(page: Page) {
  const calls: Array<{ path: string; body: unknown }> = [];
  let current = {
    incident_id: INCIDENT_ID,
    title: "Checkout 5xx elevated error rate",
    severity: "critical",
    status: "triggered",
    dedup_key: "dedup-checkout-5xx",
    triggered_at: TIMELINE[0].event_time,
    acknowledged_at: null as string | null,
    acknowledged_by: null as string | null,
    resolved_at: null as string | null,
    resolved_by: null as string | null,
    triggered_by_rule_id: "22222222-2222-2222-2222-222222222222",
    related_deployment_id: null,
    runbook_url: "https://runbooks.example.com/checkout-5xx",
    rule_name: "Checkout 5xx",
    impacted_service: "checkout",
    affected_services: ["checkout", "payments"],
    owner: null as string | null,
    impact_summary: null,
    timeline: [...TIMELINE],
    findings: [],
  };
  const append = (event_type: string, message: string) => ({
    event_time: "2026-11-15T00:10:00Z",
    event_type,
    actor: "responder@example.com",
    message,
  });

  await page.route(/\/v1\/incidents(\?.*)?$/, (route) =>
    route.fulfill({
      json: {
        items: [
          {
            incident_id: current.incident_id,
            title: current.title,
            severity: current.severity,
            status: current.status,
            triggered_at: current.triggered_at,
            triggered_by_rule_id: current.triggered_by_rule_id,
          },
          {
            incident_id: "33333333-3333-3333-3333-333333333333",
            title: "Ledger replica lag",
            severity: "warning",
            status: "resolved",
            triggered_at: "2026-11-13T08:00:00Z",
            resolved_at: "2026-11-13T08:40:00Z",
          },
        ],
      },
    }),
  );
  await page.route(`**/v1/incidents/${INCIDENT_ID}`, (route) => route.fulfill({ json: current }));
  await page.route(`**/v1/incidents/${INCIDENT_ID}/acknowledge`, async (route) => {
    calls.push({ path: "acknowledge", body: route.request().postDataJSON() });
    current = {
      ...current,
      status: "acknowledged",
      owner: "responder@example.com",
      acknowledged_by: "responder@example.com",
      acknowledged_at: "2026-11-15T00:10:00Z",
      timeline: [...current.timeline, append("acknowledged", "Acknowledged")],
    };
    await route.fulfill({ json: current });
  });
  await page.route(`**/v1/incidents/${INCIDENT_ID}/resolve`, async (route) => {
    calls.push({ path: "resolve", body: route.request().postDataJSON() });
    current = {
      ...current,
      status: "resolved",
      resolved_by: "responder@example.com",
      resolved_at: "2026-11-15T00:20:00Z",
      timeline: [...current.timeline, append("resolved", "Resolved")],
    };
    await route.fulfill({ json: current });
  });
  await page.route("**/v1/services/checkout/**", (route) =>
    route.fulfill({
      json: {
        buckets: [],
        slos: [],
        service: {
          service_name: "checkout",
          request_rate: 12.5,
          request_count: 100,
          error_rate: 0.025,
          p95_latency_ms: 245,
          slo_breaching: false,
          active_alert_count: 2,
          latest_deployment: "checkout@2026.11.14",
        },
      },
    }),
  );
  return calls;
}

async function mockServiceHealth(page: Page) {
  await page.route("**/v1/infrastructure**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/metrics**", (route) => route.fulfill({ json: { metrics: [] } }));
  await page.route("**/v1/metrics/points**", (route) => route.fulfill({ json: { points: [] } }));
}

async function mockAlertFeed(page: Page) {
  const rule = (rule_id: string, name: string, severity: string, firing: boolean) => ({
    rule_id,
    name,
    metric_name: "error_rate",
    operator: "gt",
    threshold: 0.05,
    severity,
    silenced: false,
    state: firing ? "active" : "ok",
    firing,
    last_fired_at: firing ? "2026-11-14T23:30:00Z" : null,
    notification_channels: [],
    auto_trigger_incident: firing,
  });
  await page.route("**/v1/alerts/rules**", (route) =>
    route.fulfill({
      json: { items: [rule("rule-1", "Checkout 5xx", "critical", true), rule("rule-2", "Ledger lag", "warning", false)] },
    }),
  );
  await page.route("**/v1/slos**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/notification-channels**", (route) => route.fulfill({ json: [] }));
}

async function expectNoWcagViolations(page: Page) {
  const results = await new AxeBuilder({ page }).withTags(WCAG_22_AA).analyze();
  expect(results.violations).toEqual([]);
}

/** Moves focus with Tab only until the focused element's text is `label`. */
async function tabTo(page: Page, label: string, max = 150) {
  for (let i = 0; i < max; i++) {
    await page.keyboard.press("Tab");
    const focused = await page.evaluate(() => document.activeElement?.textContent?.trim() ?? "");
    if (focused === label) return;
  }
  throw new Error(`"${label}" is not reachable with Tab within ${max} presses`);
}

test.describe("OBS-19-04: critical workflows meet WCAG 2.2 AA", () => {
  test.beforeEach(async ({ page }) => {
    await mockShell(page);
    await mockIncidents(page);
    await mockServiceHealth(page);
    await mockAlertFeed(page);
  });

  test("incident list has no WCAG 2.2 AA violations", async ({ page }) => {
    await page.goto("/incidents");
    await expect(page.getByText("Checkout 5xx elevated error rate")).toBeVisible();
    await expectNoWcagViolations(page);
  });

  test("incident detail has no WCAG 2.2 AA violations", async ({ page }) => {
    await page.goto(`/incidents/${INCIDENT_ID}`);
    await expect(page.getByRole("button", { name: "Acknowledge" })).toBeVisible();
    await expectNoWcagViolations(page);
  });

  test("service health has no WCAG 2.2 AA violations", async ({ page }) => {
    await page.goto("/services/checkout");
    await page.waitForSelector("nav[aria-label='Service signals']");
    await expectNoWcagViolations(page);
  });

  test("alert feed has no WCAG 2.2 AA violations", async ({ page }) => {
    await page.goto("/alerts");
    await page.waitForSelector("table[aria-label='Alert rules']");
    await expectNoWcagViolations(page);
  });

  test("a responder acknowledges and resolves an incident with the keyboard only", async ({ page }) => {
    const calls = await mockIncidents(page);
    await page.goto(`/incidents/${INCIDENT_ID}`);
    await expect(page.getByRole("button", { name: "Acknowledge" })).toBeVisible();

    await tabTo(page, "Acknowledge");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("button", { name: "Acknowledge", exact: true })).toHaveCount(0);

    await tabTo(page, "Resolve");
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();
    await expectNoWcagViolations(page);

    await tabTo(page, "Confirm resolve", 10);
    await page.keyboard.press("Enter");
    await expect(dialog).toBeHidden();
    await expect(page.getByRole("button", { name: "Resolve", exact: true })).toHaveCount(0);
    expect(calls.map((c) => c.path)).toEqual(["acknowledge", "resolve"]);
  });
});

test.describe("OBS-19-05: one timeline across time zones", () => {
  test("responders in Tokyo and New York read the same instants in the same order", async ({ browser, baseURL }) => {
    const read = async (timezoneId: string) => {
      const context = await browser.newContext({ baseURL, timezoneId, locale: "en-US" });
      const page = await context.newPage();
      await mockShell(page);
      await mockIncidents(page);
      await page.goto(`/incidents/${INCIDENT_ID}`);
      const times = page.locator("time[datetime]");
      await expect(times).toHaveCount(TIMELINE.length);
      const instants = await times.evaluateAll((els) => els.map((e) => e.getAttribute("datetime")));
      const shown = await times.allTextContents();
      const zone = await page.getByTestId("timeline-time-zone").textContent();
      await context.close();
      return { instants, shown, zone };
    };

    const tokyo = await read("Asia/Tokyo");
    const newYork = await read("America/New_York");

    const expected = TIMELINE.map((e) => new Date(e.event_time).toISOString());
    expect(tokyo.instants).toEqual(expected);
    expect(newYork.instants).toEqual(expected);
    expect(tokyo.zone).toContain("Asia/Tokyo");
    expect(newYork.zone).toContain("America/New_York");
    // Same instants, each shown in the reader's own zone: 23:30Z is 08:30 next day in Tokyo, 18:30 in New York.
    expect(tokyo.shown[0]).toContain("2026-11-15 08:30:00");
    expect(newYork.shown[0]).toContain("2026-11-14 18:30:00");
    tokyo.shown.forEach((text, i) => expect(text).not.toEqual(newYork.shown[i]));
  });
});
