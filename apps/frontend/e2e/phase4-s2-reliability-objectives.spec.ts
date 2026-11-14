import { test, expect, type Page } from "@playwright/test";

const TENANT_ID = "00000000-0000-0000-0000-000000000001";
const APPROVER_ID = "00000000-0000-0000-0000-000000000099";

async function mockSloGovernanceApi(page: Page, slos: Array<Record<string, unknown>>) {
  await page.route("**/v1/auth/me", (route) =>
    route.fulfill({
      json: {
        user_id: APPROVER_ID,
        email: "sre-lead@example.com",
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
  await page.route("**/v1/alerts**", (route) => route.fulfill({ json: { items: [] } }));
  await page.route("**/v1/notifications/channels**", (route) => route.fulfill({ json: [] }));
  await page.route("**/v1/slos", (route) => {
    if (route.request().method() === "POST") {
      const payload = route.request().postDataJSON() as Record<string, unknown>;
      return route.fulfill({
        status: 201,
        json: {
          slo_id: "30000000-0000-0000-0000-000000000009",
          sli_type: "availability",
          firing: false,
          last_fired_at: null,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          approved_by: payload.approval_status === "approved" ? APPROVER_ID : null,
          approved_at: payload.approval_status === "approved" ? new Date().toISOString() : null,
          ...payload,
        },
      });
    }
    return route.fulfill({ json: { items: slos } });
  });
}

test.describe("Phase 4 S2: Reliability objectives (SLIs, SLO approval, tiers, and error budget)", () => {
  test("OBS-06-01 & OBS-06-02: SLI outcome/scope and approver/period gating", async ({ page }) => {
    const now = new Date().toISOString();
    await mockSloGovernanceApi(page, [
      {
        slo_id: "slo-approved",
        service_name: "checkout",
        environment: "prod",
        sli_type: "availability",
        target: 0.999,
        window_days: 30,
        burn_rate_fast_threshold: 14.4,
        burn_rate_slow_threshold: 1.0,
        description: "Checkout availability objective",
        firing: false,
        last_fired_at: null,
        created_at: now,
        updated_at: now,
        sli_outcome: "HTTP requests succeeding without 5xx",
        sli_scope: "ingress HTTP spans excluding /healthz",
        commitment_tier: "internal_slo",
        approval_status: "approved",
        approved_by: APPROVER_ID,
        approved_at: now,
      },
      {
        slo_id: "slo-unapproved",
        service_name: "billing",
        environment: "prod",
        sli_type: "availability",
        target: 0.995,
        window_days: 30,
        burn_rate_fast_threshold: 14.4,
        burn_rate_slow_threshold: 1.0,
        description: "Billing draft SLO",
        firing: false,
        last_fired_at: null,
        created_at: now,
        updated_at: now,
        sli_outcome: "Invoice API 2xx responses",
        sli_scope: "billing-api ingress",
        commitment_tier: "internal_slo",
        approval_status: "approved",
        approved_by: null,
        approved_at: null,
      },
    ]);

    await page.goto("/alerts");
    await page.getByRole("tab", { name: "SLOs" }).click();

    const cards = page.locator("article").filter({ has: page.getByTestId("slo-commitment-tier") });
    await expect(cards).toHaveCount(2);

    // Approved SLO shows SLI outcome/scope and Approved badge
    await expect(cards.nth(0).getByTestId("slo-sli-definition")).toContainText("HTTP requests succeeding without 5xx");
    await expect(cards.nth(0).getByTestId("slo-sli-definition")).toContainText("ingress HTTP spans excluding /healthz");
    await expect(cards.nth(0).getByTestId("slo-approval-status")).toHaveText("Approved");

    // SLO lacking an approver is NOT reported as approved
    await expect(cards.nth(1).getByTestId("slo-approval-status")).toHaveText("Unapproved (Draft)");

    // Attempting to submit an approved SLO without SLI outcome/scope is rejected
    await page.getByRole("button", { name: "New SLO" }).click();
    await page.locator("#slo-service").fill("search");
    await page.locator("#slo-environment").fill("prod");
    await page.locator("#slo-approval-status").selectOption("approved");
    await page.getByRole("button", { name: "Create SLO" }).click();
    await expect(page.getByRole("alert")).toContainText(
      "An SLI without a defined outcome or scope cannot be used for an approved objective",
    );
  });

  test("OBS-06-03 & OBS-06-04: internal SLO vs contractual SLA distinction, error budget, and missing evidence disclosure", async ({ page }) => {
    const now = new Date().toISOString();
    await mockSloGovernanceApi(page, [
      {
        slo_id: "slo-internal-breaching",
        service_name: "checkout",
        environment: "prod",
        sli_type: "availability",
        target: 0.99,
        window_days: 30,
        burn_rate_fast_threshold: 14.4,
        burn_rate_slow_threshold: 1.0,
        description: "Checkout Internal SLO",
        firing: true,
        last_fired_at: now,
        created_at: now,
        updated_at: now,
        sli_outcome: "HTTP 2xx/3xx responses",
        sli_scope: "checkout ingress",
        commitment_tier: "internal_slo",
        approval_status: "approved",
        approved_by: APPROVER_ID,
        approved_at: now,
        total_events: 1000,
        good_events: 980,
        expected_intervals: 10,
        observed_intervals: 10,
      },
      {
        slo_id: "sla-contractual-missing",
        service_name: "payments",
        environment: "prod",
        sli_type: "availability",
        target: 0.999,
        window_days: 30,
        burn_rate_fast_threshold: 14.4,
        burn_rate_slow_threshold: 1.0,
        description: "Payments Contractual SLA",
        firing: false,
        last_fired_at: null,
        created_at: now,
        updated_at: now,
        sli_outcome: "Payment capture success",
        sli_scope: "payments-api",
        commitment_tier: "contractual_sla",
        approval_status: "approved",
        approved_by: APPROVER_ID,
        approved_at: now,
        total_events: 0,
        good_events: 0,
        expected_intervals: 10,
        observed_intervals: 4,
      },
    ]);

    await page.goto("/alerts");
    await page.getByRole("tab", { name: "SLOs" }).click();

    const cards = page.locator("article").filter({ has: page.getByTestId("slo-commitment-tier") });
    await expect(cards).toHaveCount(2);

    // OBS-06-03: Internal SLO is labeled Internal SLO, never Contractual SLA
    await expect(cards.nth(0).getByTestId("slo-commitment-tier")).toHaveText("Internal SLO");
    await expect(cards.nth(0)).not.toContainText("Contractual SLA");
    await expect(cards.nth(1).getByTestId("slo-commitment-tier")).toHaveText("Contractual SLA");

    // OBS-06-04: Known period of failures updates attainment and remaining budget
    await expect(cards.nth(0).getByTestId("slo-attainment")).toHaveText("98.00%");
    await expect(cards.nth(0).getByTestId("slo-remaining-budget")).toHaveText("-100.0%");

    // OBS-06-04: Missing evidence is disclosed and NOT counted as good events
    await expect(cards.nth(1).getByTestId("slo-attainment")).toHaveText("Unassessed");
    await expect(cards.nth(1).getByTestId("slo-evidence-disclosure")).toContainText(
      "Missing evidence for 6 of 10 evaluation intervals; missing intervals are not counted as good events.",
    );
  });
});
