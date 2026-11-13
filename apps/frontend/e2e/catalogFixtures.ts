import { expect, type Page } from "@playwright/test";
import { createCatalogReadMock } from "../../../services/catalog-service/mocks/manage-service-catalog.v0.1.mjs";

export const catalogTenant = "00000000-0000-0000-0000-000000000001";
export const restrictedServiceId = "00000000-0000-0000-0000-000000000012";
export const checkout = {
  id: "00000000-0000-0000-0000-000000000011",
  business_name: "Checkout",
  operational_name: "checkout-api",
  domain: "Commerce",
  criticality: "critical",
  lifecycle: "live",
  live: true,
  classification: "internal",
  description: "Takes orders and authorizes payments.",
  support_route: "#checkout-on-call",
  owners: [
    { role: "business_owner", principal_kind: "team", principal_id: "team-1", team_name: "Commerce team", state: "active" },
    { role: "operational_owner", principal_kind: "team", principal_id: "team-2", team_name: "Payments team", state: "active" },
  ],
  aliases: [{ alias: "old-checkout", state: "retired" }],
  outcomes: [],
  exceptions: [],
};

type Review = {
  live?: boolean;
  lifecycle?: string;
  health?: [state: string, evidence: string];
  expectation?: [status: string, reason: string];
  objectives?: { status: string | null; evidence_status: string | null; approved: boolean }[];
};

/** A query service review (`/v1/review/services`) for one service in one class. */
export function review(environmentClass: string, service: { id: string; business_name: string }, r: Review = {}) {
  const live = r.live ?? true;
  const [state, evidence] = r.health ?? ["normal", "current"];
  const [status, reason] = r.expectation ?? ["compliant", "current"];
  return {
    service: { id: service.id, business_name: service.business_name, criticality: "critical", lifecycle: r.lifecycle ?? "live" },
    environment_class: environmentClass,
    live,
    health: live ? { state, evidence_status: evidence, evaluated_at: "2026-11-09T12:00:00Z", last_evidence_at: "2026-11-09T11:59:00Z" } : null,
    expectation: live ? {
      status, reason,
      objectives: (r.objectives ?? [{ status, evidence_status: reason, approved: true }]).map((o, i) => ({
        objective_id: `objective-${environmentClass}-${i}`, sli_type: "availability", target: 0.995, window_days: 30,
        description: `Checkout availability ${i + 1}`, attainment: null, error_budget_consumed: null, last_evidence_at: null, ...o,
      })),
    } : null,
  };
}

export async function mockCatalog(page: Page) {
  const read = createCatalogReadMock({
    records: [
      checkout,
      { ...checkout, id: restrictedServiceId, business_name: "Restricted Payments", operational_name: "checkout-restricted", classification: "restricted" },
    ],
    canRead: (record: typeof checkout) => record.classification !== "restricted",
  });
  await page.route("**/v1/auth/me", (route) => route.fulfill({
    json: { user_id: catalogTenant, email: "viewer@example.com", tenants: [{ tenant_id: catalogTenant, role: "viewer" }] },
  }));
  await page.route("**/v1/tenants", (route) => route.fulfill({
    json: { tenants: [{ id: catalogTenant, name: "observable" }] },
  }));
  await page.route("**/v1/tenants/**/environments", (route) => route.fulfill({ json: { environments: [] } }));
  // Default service review: production meets its objective, non-production has none approved.
  await page.route("**/v1/review/services**", (route) => {
    expect(route.request().headers()["x-tenant-id"]).toBe(catalogTenant);
    const url = new URL(route.request().url());
    const environmentClass = url.searchParams.get("environment_class") ?? "";
    const nonProduction = { health: ["unknown", "missing"] as [string, string], expectation: ["unknown", "no_approved_objective"] as [string, string], objectives: [] };
    const body = (environmentClass === "production" ? review(environmentClass, checkout) : review(environmentClass, checkout, nonProduction));
    if (url.pathname === "/v1/review/services") return route.fulfill({ json: { environment_class: environmentClass, items: [body], total: 1 } });
    if (url.pathname === `/v1/review/services/${checkout.id}`) return route.fulfill({ json: body });
    return route.fulfill({ status: 404, json: { error: "not_found" } });
  });
  await page.route("**/v1/catalog/**", async (route) => {
    expect(route.request().headers()["x-tenant-id"]).toBe(catalogTenant);
    const url = new URL(route.request().url());
    const path = url.pathname.slice("/v1/catalog".length);
    const result = await read(url);
    if (result) return route.fulfill({ status: result.status, json: result.body });
    if (path === "/coverage") return route.fulfill({ json: {
      items: [{ ...checkout, coverage_status: "unknown", stale: true, assessed_at: "2026-11-07T08:00:00Z" }], total: 1,
    } });
    if (path === "/gaps") return route.fulfill({ json: {
      items: [{ id: "gap-1", service_id: checkout.id, item: "objective", state: "open", remediation_owner_id: null, due_date: null, review_date: null }], total: 1,
    } });
    if (path === "/providers") return route.fulfill({ json: {
      items: [{ id: "provider-1", name: "Payment Gateway", kind: "payment", material: true }], total: 1,
    } });
    if (path === "/providers/provider-1") return route.fulfill({ json: {
      id: "provider-1", name: "Payment Gateway", kind: "payment", material: true, contract_ref: "Processing agreement",
      contract_owner: checkout.owners[0], dependents: [{ dependency_id: "dep-1", function: "Authorize payments", service: checkout }],
      dependents_withheld: false, exceptions: [],
    } });
    if (path === "/providers/gaps") return route.fulfill({ json: {
      items: [{ reason: "unregistered_provider", provider_name: "Card Tokenizer", dependent: { dependency_id: "dep-2", function: "Tokenize cards", service: checkout } }],
      total: 1,
    } });
    if (path === "/assets") {
      expect(url.searchParams.get("ownerless")).toBe("true");
      return route.fulfill({ json: {
        items: [{ kind: "dashboard", external_id: "asset-1", owning_component: "control", service_id: checkout.id, name: "Checkout errors", owner_state: "departed" }], total: 1,
      } });
    }
    if (path === `/services/${checkout.id}/coverage`) return route.fulfill({ json: {
      service: checkout, status: "uncovered", items: [{ item: "objective", demonstrated: false, evidence_ref: null, detail: "No active objective" }],
      assessed_at: "2026-11-08T08:00:00Z", bindings: [{ service_name: "checkout-api", environment: "production" }],
    } });
    return route.fulfill({ status: 404, json: { error: "not_found" } });
  });
}
