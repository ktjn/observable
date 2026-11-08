import { catalogPageSize, type CatalogRuntime, type CatalogServiceRecord, type CatalogProviderRecord } from "./catalogContract";

const checkout: CatalogServiceRecord = {
  id: "00000000-0000-0000-0000-000000000011",
  business_name: "Checkout",
  operational_name: "checkout",
  domain: "Commerce",
  criticality: "critical",
  lifecycle: "live",
  description: "Accepts orders and authorizes payments.",
  support_route: "#checkout-on-call",
  aliases: [{ alias: "orders", state: "active" }],
  owners: [
    { role: "business_owner", principal_kind: "team", principal_id: "commerce", team_name: "Commerce", state: "active" },
    { role: "operational_owner", principal_kind: "team", principal_id: "checkout", team_name: "Checkout operations", state: "active" },
  ],
  outcomes: [{ id: "orders", statement: "Customers can place orders.", expectation: "Orders are confirmed after payment.", approved: true }],
  exceptions: [],
};

const payment: CatalogProviderRecord = {
  id: "00000000-0000-0000-0000-000000000021",
  name: "Payment Gateway",
  kind: "payment",
  material: true,
  contract_ref: "Payment processing agreement",
  contract_owner: checkout.owners[0],
  dependents: [{ dependency_id: "payment", function: "Authorize payments", service: checkout }],
  dependents_withheld: false,
  exceptions: [],
};

const assessedAt = new Date().toISOString();

function page<T>(items: T[], offset = 0) {
  return { items: items.slice(offset, offset + catalogPageSize), total: items.length };
}

export const playgroundCatalog: CatalogRuntime = {
  async services(_tenant, offset, query) {
    const term = query.toLowerCase();
    const matched = [checkout.business_name, checkout.operational_name, "orders"].some((name) =>
      name.toLowerCase().startsWith(term))
      || checkout.domain.toLowerCase() === term
      || checkout.owners.some((owner) => owner.team_name?.toLowerCase() === term);
    return page(matched ? [checkout] : [], offset);
  },
  async service(_tenant, id) {
    return id === checkout.id ? checkout : null;
  },
  async coverage() {
    return page([{ ...checkout, coverage_status: "partial", assessed_at: assessedAt, stale: false }]);
  },
  async assessment(_tenant, id) {
    if (id !== checkout.id) return null;
    return {
      status: "partial",
      assessed_at: assessedAt,
      bindings: [{ service_name: "checkout", environment: "demo" }],
      items: [
        { item: "traces", demonstrated: true, evidence_ref: "demo telemetry", detail: "Recent traces observed" },
        { item: "objective", demonstrated: false, evidence_ref: null, detail: "No active objective" },
      ],
    };
  },
  async gaps() {
    return page([{
      id: "checkout-objective", service_id: checkout.id, journey_id: null, item: "objective",
      state: "open", remediation_owner_id: null, due_date: null, review_date: null, acceptance_rationale: null,
    }]);
  },
  async providers(_tenant, offset) {
    return page([payment], offset);
  },
  async provider(_tenant, id) {
    return id === payment.id ? payment : null;
  },
  async providerGaps() {
    return page([]);
  },
  async ownerlessAssets() {
    return page([{
      kind: "dashboard", external_id: "checkout-errors", owning_component: "control",
      service_id: checkout.id, name: "Checkout errors", owner_state: "none",
    }]);
  },
};
