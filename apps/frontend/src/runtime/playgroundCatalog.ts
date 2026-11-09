import {
  catalogPageSize, type CatalogRuntime, type CatalogServiceRecord, type CatalogProviderRecord,
  type EnvironmentClass, type ServiceReview,
} from "./catalogContract";

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

/** Production meets its approved objective; non-production has no approved objective yet. */
function reviewOf(environmentClass: EnvironmentClass): ServiceReview {
  const production = environmentClass === "production";
  return {
    service: { id: checkout.id, business_name: checkout.business_name, criticality: checkout.criticality, lifecycle: checkout.lifecycle },
    environment_class: environmentClass,
    live: true,
    health: production
      ? { state: "normal", evidence_status: "current", evaluated_at: assessedAt, last_evidence_at: assessedAt }
      : { state: "unknown", evidence_status: "missing", evaluated_at: assessedAt, last_evidence_at: null },
    expectation: production
      ? {
        status: "compliant", reason: "current", objectives: [{
          objective_id: "checkout-availability", sli_type: "availability", target: 0.995, window_days: 30,
          description: "Orders are confirmed after payment.", approved: true, status: "compliant",
          evidence_status: "current", attainment: 0.998, error_budget_consumed: 0.4, last_evidence_at: assessedAt,
        }],
      }
      : { status: "unknown", reason: "no_approved_objective", objectives: [] },
  };
}

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
  async review(_tenant, id, environmentClass) {
    return id === checkout.id ? reviewOf(environmentClass) : null;
  },
  async reviews(_tenant, environmentClass, _scope, offset) {
    return page([reviewOf(environmentClass)], offset);
  },
  async ownerlessAssets() {
    return page([{
      kind: "dashboard", external_id: "checkout-errors", owning_component: "control",
      service_id: checkout.id, name: "Checkout errors", owner_state: "none",
    }]);
  },
};
