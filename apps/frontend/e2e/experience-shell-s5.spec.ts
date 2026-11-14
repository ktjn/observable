import { test, expect, type Page } from "@playwright/test";

async function mockAuthAndTenant(
  page: Page,
  options: {
    role?: string;
    elevated_access?: {
      active: boolean;
      scope?: string;
      justification?: string;
      expires_at?: string;
    } | null;
  } = {},
) {
  await page.route("**/v1/auth/me", (route) =>
    route.fulfill({
      json: {
        user_id: "00000000-0000-0000-0000-000000000001",
        email: "operator@example.com",
        tenants: [
          {
            tenant_id: "00000000-0000-0000-0000-000000000001",
            role: options.role ?? "admin",
          },
        ],
        elevated_access: options.elevated_access ?? null,
      },
    }),
  );
  await page.route("**/v1/tenants", (route) =>
    route.fulfill({
      json: {
        tenants: [{ id: "00000000-0000-0000-0000-000000000001", name: "observable" }],
      },
    }),
  );
  await page.route("**/v1/tenants/**/environments", (route) =>
    route.fulfill({ json: { environments: [{ environment: "prod" }] } }),
  );
  await page.route("**/v1/config", (route) => route.fulfill({ json: {} }));
}

test.describe("Phase 3 S5: Access-aware UI, existence hiding & elevated access", () => {
  test("OBS-25-01: mobile viewport keeps incident status, affected service, and Acknowledge usable when non-essential topology fails", async ({
    page,
  }) => {
    await page.setViewportSize({ width: 375, height: 667 });
    await mockAuthAndTenant(page, { role: "admin" });

    let status = "triggered";
    await page.route("**/v1/incidents/inc-1", (route) =>
      route.fulfill({
        json: {
          id: "inc-1",
          title: "Checkout API degradation",
          severity: "critical",
          status,
          triggered_at: "2026-11-14T07:00:00Z",
          resolved_at: null,
          acknowledged_by: status === "acknowledged" ? "operator@example.com" : null,
          impacted_service: "checkout",
          triggered_by_rule_id: null,
          timeline: [],
        },
      }),
    );
    await page.route("**/v1/incidents/inc-1/acknowledge", (route) => {
      status = "acknowledged";
      return route.fulfill({ json: { status: "acknowledged" } });
    });
    // Non-essential topology fails.
    await page.route("**/v1/topology/map**", (route) =>
      route.fulfill({ status: 500, json: { error: "topology unavailable" } }),
    );

    await page.goto("/incidents/inc-1");
    await expect(page.getByText("Checkout API degradation")).toBeVisible();
    await expect(page.getByRole("link", { name: "checkout" })).toBeVisible();
    const ackBtn = page.getByRole("button", { name: "Acknowledge" });
    await expect(ackBtn).toBeVisible();
    await ackBtn.click();
    await expect(page.getByTestId("action-feedback")).toContainText("Incident acknowledged");
  });

  test("OBS-25-02 & UX-10: shared restricted links hide existence and viewer role cannot access mutation controls", async ({
    page,
  }) => {
    await mockAuthAndTenant(page, { role: "viewer" });
    await page.route("**/v1/incidents/inc-secret", (route) =>
      route.fulfill({ status: 403, json: { error: "forbidden" } }),
    );
    await page.route("**/v1/incidents/inc-public", (route) =>
      route.fulfill({
        json: {
          id: "inc-public",
          title: "Readonly incident",
          severity: "warning",
          status: "triggered",
          triggered_at: "2026-11-14T07:00:00Z",
          resolved_at: null,
          acknowledged_by: null,
          impacted_service: "checkout",
          triggered_by_rule_id: null,
          timeline: [],
        },
      }),
    );
    await page.route("**/v1/topology/map**", (route) =>
      route.fulfill({ json: { nodes: [], edges: [] } }),
    );

    // Restricted shared link reveals neither existence nor restricted details/actions.
    await page.goto("/incidents/inc-secret");
    const withheld = page.getByTestId("incident-withheld");
    await expect(withheld).toBeVisible();
    await expect(withheld).toContainText("Incident unavailable");
    await expect(withheld).not.toContainText("inc-secret");
    await expect(page.getByRole("button", { name: "Acknowledge" })).toHaveCount(0);

    // Viewer on authorized incident cannot see mutation controls.
    await page.goto("/incidents/inc-public");
    await expect(page.getByText("Readonly incident")).toBeVisible();
    await expect(page.getByRole("button", { name: "Acknowledge" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Resolve" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "Add Note" })).toHaveCount(0);
  });

  test("OBS-25-03 & UX-10: active org context, elevated access disclosure before confirmation, and automatic expiry", async ({
    page,
  }) => {
    const expiresAt = new Date(Date.now() + 5000).toISOString();
    await mockAuthAndTenant(page, {
      role: "admin",
      elevated_access: {
        active: true,
        scope: "incident:write",
        justification: "SEV-1 mitigation",
        expires_at: expiresAt,
      },
    });
    await page.route("**/v1/incidents/inc-1", (route) =>
      route.fulfill({
        json: {
          id: "inc-1",
          title: "Checkout outage",
          severity: "critical",
          status: "acknowledged",
          triggered_at: "2026-11-14T07:00:00Z",
          resolved_at: null,
          acknowledged_by: "operator@example.com",
          impacted_service: "checkout",
          triggered_by_rule_id: null,
          timeline: [],
        },
      }),
    );
    await page.route("**/v1/incidents/inc-1/resolve", (route) =>
      route.fulfill({ json: { status: "resolved" } }),
    );
    await page.route("**/v1/topology/map**", (route) =>
      route.fulfill({ json: { nodes: [], edges: [] } }),
    );

    await page.goto("/incidents/inc-1");
    await expect(page.getByTestId("active-org-context")).toContainText("observable · admin");
    await expect(page.getByTestId("elevated-access-indicator")).toContainText(
      "ELEVATED ACCESS (incident:write)",
    );

    // Confirmation dialog visibly marks elevated access before acting.
    await page.getByRole("button", { name: "Resolve" }).click();
    await expect(page.getByTestId("elevated-action-disclosure")).toContainText(
      "Elevated access active",
    );
    await expect(page.getByTestId("elevated-action-disclosure")).toContainText(
      "SEV-1 mitigation",
    );

    await page.getByRole("button", { name: "Confirm resolve" }).click();
    await expect(page.getByTestId("action-feedback")).toContainText(
      "Incident resolved (recorded under elevated access)",
    );

    // When elevation expires, indicator is automatically withdrawn.
    await expect(page.getByTestId("elevated-access-indicator")).toHaveCount(0, { timeout: 8000 });
  });
});
