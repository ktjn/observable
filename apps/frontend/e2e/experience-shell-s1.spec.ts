import { test, expect, type Page } from "@playwright/test";

const TENANT_ID = "00000000-0000-0000-0000-000000000001";
const INCIDENT_ID = "11111111-1111-4111-8111-111111111111";
const RESTRICTED_INCIDENT_ID = "99999999-9999-4999-8999-999999999999";

async function mockShellApi(page: Page) {
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
  await page.route("**/v1/services/**", (route) =>
    route.fulfill({
      json: {
        service: {
          service_name: "checkout",
          request_rate: 12.5,
          request_count: 1200,
          error_rate: 0.08,
          p95_latency_ms: 420,
        },
        buckets: [],
        operations: [],
      },
    }),
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
          },
        ],
      },
    }),
  );
  await page.route("**/v1/alerts/rules**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/catalog/services**", (route) =>
    route.fulfill({
      json: {
        items: [
          {
            id: "svc-checkout",
            business_name: "Checkout",
            operational_name: "checkout",
            domain: "commerce",
            criticality: "critical",
            lifecycle: "live",
          },
        ],
        total: 1,
      },
    }),
  );
  await page.route("**/v1/review/services**", (route) =>
    route.fulfill({
      json: {
        confidence: {
          confidence: "full",
          affected: [],
          functions: [],
          checked_at: "2026-11-13T19:00:00Z",
        },
        period: { as_of: "2026-11-13T19:00:00Z", freshness_seconds: 15 },
        items: [
          {
            service: {
              id: "svc-checkout",
              business_name: "Checkout",
              operational_name: "checkout",
              domain: "commerce",
              criticality: "critical",
              lifecycle: "live",
            },
            live: true,
            health: {
              state: "degraded",
              evidence_status: "current",
              evaluated_at: "2026-11-13T19:00:00Z",
              last_evidence_at: "2026-11-13T18:59:45Z",
            },
            expectation: { status: "at_risk", reason: "High error rate", objectives: [] },
          },
          {
            service: {
              id: "svc-billing",
              business_name: "Billing",
              operational_name: "billing",
              domain: "commerce",
              criticality: "critical",
              lifecycle: "live",
            },
            live: true,
            health: null,
            expectation: null,
          },
        ],
        total: 2,
      },
    }),
  );
  await page.route("**/v1/incidents**", (route) =>
    route.fulfill({
      json: {
        items: [
          {
            incident_id: INCIDENT_ID,
            title: "Checkout latency spike",
            severity: "critical",
            status: "triggered",
            triggered_at: "2026-11-13T18:50:00Z",
          },
        ],
      },
    }),
  );
  await page.route(`**/v1/incidents/${INCIDENT_ID}`, (route) =>
    route.fulfill({
      json: {
        incident_id: INCIDENT_ID,
        title: "Checkout latency spike",
        severity: "critical",
        status: "triggered",
        dedup_key: "checkout-latency",
        triggered_at: "2026-11-13T18:50:00Z",
        acknowledged_at: null,
        acknowledged_by: null,
        resolved_at: null,
        resolved_by: null,
        triggered_by_rule_id: "rule-1",
        runbook_url: "https://runbooks.example.com/checkout",
        rule_name: "High P95",
        timeline: [],
        impacted_service: "checkout",
      },
    }),
  );
  await page.route("**/v1/topology**", (route) => route.fulfill({ json: { edges: [] } }));
}

test("obs-20-01 primary navigation exposes plain operational entry points", async ({ page }) => {
  await mockShellApi(page);
  await page.goto("/");
  const sidebar = page.getByLabel("Application sidebar");
  for (const label of [
    "Service Health",
    "Incidents",
    "Investigation",
    "Customer Experience",
    "Reporting",
  ]) {
    await expect(sidebar.getByRole("link", { name: label, exact: true })).toBeVisible();
  }
});

test("obs-20-02 search resolves service name and incident id while hiding unauthorized incidents", async ({
  page,
}) => {
  await mockShellApi(page);
  await page.goto("/");
  await page.keyboard.press("Control+k");
  const dialog = page.getByRole("dialog", { name: "Command palette" });
  await expect(dialog).toBeVisible();

  const input = dialog.getByRole("combobox");
  await input.fill("Checkout");
  await expect(dialog.getByText(/Service: Checkout/)).toBeVisible();

  await input.fill(INCIDENT_ID);
  await expect(dialog.getByText(/Incident: Checkout latency spike/)).toBeVisible();

  await input.fill(RESTRICTED_INCIDENT_ID);
  await expect(dialog.getByText("No results")).toBeVisible();
  await expect(dialog.getByText(RESTRICTED_INCIDENT_ID)).not.toBeVisible();
});

test("obs-20-03 obs-20-04 global scope bar stays visible and preserves context across drilldown", async ({
  page,
}) => {
  await mockShellApi(page);
  await page.goto("/?preset=3h");
  const scopeBar = page.getByTestId("global-scope-bar");
  await expect(scopeBar).toContainText("Tenant: observable");
  await expect(scopeBar).toContainText("Period: Last 3 hours");
  await expect(scopeBar).toContainText("Filters: None");

  await page.getByRole("link", { name: "checkout", exact: true }).first().click();
  await expect(page).toHaveURL(/\/services\/checkout/);
  await expect(scopeBar).toContainText("Service: checkout");
  await expect(scopeBar).toContainText("Period: Last 3 hours");
});

test("obs-20-08 obs-21-01 obs-21-08 overview shows unknown critical services, opens incident, and back restores scope", async ({
  page,
}) => {
  await mockShellApi(page);
  await page.goto("/?preset=3h");

  const criticalTable = page.getByRole("table", { name: "Critical services" });
  await expect(criticalTable).toContainText("Checkout");
  await expect(criticalTable).toContainText("Degraded");
  await expect(criticalTable).toContainText("Billing");
  await expect(criticalTable).toContainText("Unknown");

  await page.getByRole("link", { name: "Checkout latency spike" }).click();
  await expect(page).toHaveURL(new RegExp(`/incidents/${INCIDENT_ID}`));
  await page.getByRole("link", { name: /Back to previous view/ }).click();
  await expect(page).toHaveURL(/\?preset=3h/);
});

test("obs-21-03 incident responder view highlights unassigned critical issues, ownership, impact and escalation", async ({
  page,
}) => {
  await mockShellApi(page);
  await page.goto("/incidents");
  const row = page.getByRole("row", { name: /Checkout latency spike/ });
  await expect(row).toContainText("critical");
  await expect(row).toContainText("Unassigned");

  await row.getByRole("link", { name: "Checkout latency spike" }).click();
  await expect(page.getByText("Unassigned")).toBeVisible();
  await expect(page.getByRole("link", { name: "checkout", exact: true })).toBeVisible();
  await expect(page.getByRole("link", { name: "https://runbooks.example.com/checkout" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Acknowledge" })).toBeVisible();
});
