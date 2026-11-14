import { test, expect, type Page } from "@playwright/test";

const TENANT_ID = "00000000-0000-0000-0000-000000000001";
const RULE_ID = "40000000-0000-0000-0000-000000000001";

async function mockRuleDetailApi(
  page: Page,
  rule: {
    runbook_url: string | null;
    runbook_reviewed_at: string | null;
    guidance_status?: "current" | "missing" | "past_review";
    guidance_flagged_to_owner?: boolean;
  },
) {
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
    route.fulfill({ json: { environments: [{ environment: "prod" }] } }),
  );
  await page.route(`**/v1/alerts/rules/${RULE_ID}`, (route) =>
    route.fulfill({
      json: {
        rule_id: RULE_ID,
        name: "Checkout 5xx critical",
        severity: "critical",
        alert_type: "threshold",
        condition: { metric_name: "http.server.errors", operator: "gt", threshold: 0.05 },
        silenced: false,
        firing: true,
        firings: [],
        owner_kind: "team",
        owner_id: "00000000-0000-0000-0000-000000000042",
        ...rule,
      },
    }),
  );
}

test.describe("Phase 5 S2: Operational guidance & runbooks (OBS-33-01)", () => {
  test("Acceptance: responder opens current runbook directly from critical alert", async ({ page }) => {
    await mockRuleDetailApi(page, {
      runbook_url: "https://runbooks.example.com/checkout-5xx",
      runbook_reviewed_at: new Date().toISOString(),
      guidance_status: "current",
      guidance_flagged_to_owner: false,
    });

    await page.goto(`/alerts/${RULE_ID}`);
    const link = page.getByRole("link", { name: "https://runbooks.example.com/checkout-5xx" });
    await expect(link).toBeVisible();
    await expect(link).toHaveAttribute("href", "https://runbooks.example.com/checkout-5xx");
    await expect(page.getByTestId("runbook-guidance-flag")).toHaveCount(0);
  });

  test("Negative: critical alert with missing or past-review guidance is flagged to owner", async ({ page }) => {
    await mockRuleDetailApi(page, {
      runbook_url: null,
      runbook_reviewed_at: null,
      guidance_status: "missing",
      guidance_flagged_to_owner: true,
    });

    await page.goto(`/alerts/${RULE_ID}`);
    await expect(page.getByTestId("runbook-guidance-flag")).toContainText("Guidance missing — flagged to owner");

    const staleDate = new Date(Date.now() - 120 * 86_400_000).toISOString();
    await mockRuleDetailApi(page, {
      runbook_url: "https://runbooks.example.com/checkout-5xx",
      runbook_reviewed_at: staleDate,
      guidance_status: "past_review",
      guidance_flagged_to_owner: true,
    });

    await page.goto(`/alerts/${RULE_ID}`);
    await expect(page.getByTestId("runbook-guidance-flag")).toContainText("Guidance past review — flagged to owner");
  });
});
