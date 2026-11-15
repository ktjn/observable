import {
  catalogPageSize, type CatalogRuntime, type CatalogServiceRecord, type CatalogProviderRecord,
  type EnvironmentClass, type PlatformConfidence, type ServiceReview,
} from "./catalogContract";

const SERVICES: CatalogServiceRecord[] = [
  {
    id: "c4b1e8a2-7f3d-4c91-9b14-8a2e5f1d3c01",
    business_name: "Web Storefront",
    operational_name: "web",
    domain: "Storefront",
    criticality: "critical",
    lifecycle: "live",
    description: "Server-rendered storefront and customer shopping experience.",
    support_route: "#storefront-on-call",
    aliases: [{ alias: "storefront", state: "active" }],
    owners: [
      { role: "business_owner", principal_kind: "team", principal_id: "storefront", team_name: "Storefront", state: "active" },
      { role: "operational_owner", principal_kind: "team", principal_id: "web-platform", team_name: "Web Platform", state: "active" },
    ],
    outcomes: [{ id: "storefront-browse", statement: "Shoppers can browse and load pages quickly.", expectation: "P95 page render stays under 300ms.", approved: true }],
    exceptions: [],
  },
  {
    id: "d8e2f4b6-3a9c-4e12-8c45-7b3d9e2a4f02",
    business_name: "API Gateway",
    operational_name: "api-gateway",
    domain: "Platform Edge",
    criticality: "critical",
    lifecycle: "live",
    description: "Edge ingress, auth verification, rate limiting, and upstream routing.",
    support_route: "#edge-on-call",
    aliases: [{ alias: "edge-gateway", state: "active" }],
    owners: [
      { role: "business_owner", principal_kind: "team", principal_id: "platform-edge", team_name: "Platform Edge", state: "active" },
      { role: "operational_owner", principal_kind: "team", principal_id: "edge-ops", team_name: "Edge Operations", state: "active" },
    ],
    outcomes: [{ id: "edge-routing", statement: "API traffic is authenticated and routed reliably.", expectation: "99.95% availability across edge routes.", approved: true }],
    exceptions: [],
  },
  {
    id: "a3f7c1d9-5b2e-4a83-9e61-4c8d2f7b5a03",
    business_name: "Checkout",
    operational_name: "checkout",
    domain: "Commerce",
    criticality: "critical",
    lifecycle: "live",
    description: "Accepts customer orders, validates carts, and coordinates payment capture.",
    support_route: "#checkout-on-call",
    aliases: [{ alias: "orders", state: "active" }],
    owners: [
      { role: "business_owner", principal_kind: "team", principal_id: "commerce", team_name: "Commerce", state: "active" },
      { role: "operational_owner", principal_kind: "team", principal_id: "checkout", team_name: "Checkout Operations", state: "active" },
    ],
    outcomes: [{ id: "orders", statement: "Customers can place orders.", expectation: "Orders are confirmed after payment.", approved: true }],
    exceptions: [],
  },
  {
    id: "e9b4a7c2-1d6f-4b54-8a32-9f1e6c4d8b04",
    business_name: "Payment Service",
    operational_name: "payment",
    domain: "Payments",
    criticality: "critical",
    lifecycle: "live",
    description: "Authorizes, captures, and settles card and wallet transactions.",
    support_route: "#payments-on-call",
    aliases: [{ alias: "billing-payments", state: "active" }],
    owners: [
      { role: "business_owner", principal_kind: "team", principal_id: "payments", team_name: "Payments", state: "active" },
      { role: "operational_owner", principal_kind: "team", principal_id: "payments-sre", team_name: "Payments SRE", state: "active" },
    ],
    outcomes: [{ id: "payment-auth", statement: "Valid payment methods authorize without false declines.", expectation: "Authorization success >= 99.5%.", approved: true }],
    exceptions: [],
  },
  {
    id: "f2c6d8e1-9a4b-4c75-b103-5d8e2a7c9f05",
    business_name: "Inventory",
    operational_name: "inventory",
    domain: "Fulfillment",
    criticality: "high",
    lifecycle: "live",
    description: "Tracks SKU stock levels and holds warehouse reservations during checkout.",
    support_route: "#fulfillment-on-call",
    aliases: [{ alias: "stock-service", state: "active" }],
    owners: [
      { role: "business_owner", principal_kind: "team", principal_id: "fulfillment", team_name: "Fulfillment", state: "active" },
      { role: "operational_owner", principal_kind: "team", principal_id: "inventory-ops", team_name: "Inventory Ops", state: "active" },
    ],
    outcomes: [{ id: "stock-reservation", statement: "Available stock is reserved atomically.", expectation: "Reservation latency P95 < 150ms.", approved: false }],
    exceptions: [],
  },
  {
    id: "1b7e9d34-6c2a-4f86-9d42-3a5b8e1c7d06",
    business_name: "Product Catalog",
    operational_name: "catalog",
    domain: "Commerce",
    criticality: "high",
    lifecycle: "live",
    description: "Serves product metadata, variants, and regional pricing tiers.",
    support_route: "#catalog-on-call",
    aliases: [{ alias: "products", state: "active" }],
    owners: [
      { role: "business_owner", principal_kind: "team", principal_id: "commerce", team_name: "Commerce", state: "active" },
      { role: "operational_owner", principal_kind: "team", principal_id: "catalog-eng", team_name: "Catalog Engineering", state: "active" },
    ],
    outcomes: [{ id: "catalog-reads", statement: "Product pages display accurate pricing and stock.", expectation: "99.9% read availability.", approved: true }],
    exceptions: [],
  },
  {
    id: "2c8f0e45-7d3b-4a97-ae53-4b6c9f2d8e07",
    business_name: "Search Engine",
    operational_name: "search",
    domain: "Storefront",
    criticality: "medium",
    lifecycle: "live",
    description: "Hybrid vector and full-text product search and autocomplete.",
    support_route: "#search-on-call",
    aliases: [{ alias: "discovery-search", state: "active" }],
    owners: [
      { role: "business_owner", principal_kind: "team", principal_id: "storefront", team_name: "Storefront", state: "active" },
      { role: "operational_owner", principal_kind: "team", principal_id: "search-relevance", team_name: "Search Relevance", state: "active" },
    ],
    outcomes: [{ id: "search-results", statement: "Queries return ranked product matches.", expectation: "P95 search latency < 120ms.", approved: true }],
    exceptions: [],
  },
  {
    id: "3d9a1f56-8e4c-4b08-bf64-5c7d0a3e9f08",
    business_name: "Orders Database",
    operational_name: "database",
    domain: "Core Data",
    criticality: "critical",
    lifecycle: "live",
    description: "Primary PostgreSQL cluster storing orders, payments, and ledger entries.",
    support_route: "#data-platform-on-call",
    aliases: [{ alias: "orders-pg", state: "active" }],
    owners: [
      { role: "business_owner", principal_kind: "team", principal_id: "data-platform", team_name: "Data Platform", state: "active" },
      { role: "operational_owner", principal_kind: "team", principal_id: "dba-oncall", team_name: "Database Reliability", state: "active" },
    ],
    outcomes: [{ id: "db-durability", statement: "Order writes are durable and low-latency.", expectation: "99.99% primary availability.", approved: true }],
    exceptions: [],
  },
];

const byOp = (op: string) => SERVICES.find((s) => s.operational_name === op)!;

const PROVIDERS: CatalogProviderRecord[] = [
  {
    id: "7a1c5e90-4b2d-4f18-8c31-6e9a2d4f7b11",
    name: "Stripe Payment Gateway",
    kind: "payment",
    material: true,
    contract_ref: "MSA-2026-STRIPE-01",
    contract_owner: byOp("payment").owners[0],
    dependents: [
      { dependency_id: "stripe-auth", function: "Authorize and capture card payments", service: byOp("payment") },
      { dependency_id: "stripe-checkout", function: "Validate payment intents", service: byOp("checkout") },
    ],
    dependents_withheld: false,
    exceptions: [],
  },
  {
    id: "8b2d6f01-5c3e-4a29-9d42-7f0b3e5a8c12",
    name: "AWS RDS PostgreSQL",
    kind: "database",
    material: true,
    contract_ref: "INFRA-RDS-PROD-01",
    contract_owner: byOp("database").owners[0],
    dependents: [
      { dependency_id: "rds-orders", function: "Persist order and ledger state", service: byOp("database") },
      { dependency_id: "rds-inventory", function: "Coordinate stock locks", service: byOp("inventory") },
    ],
    dependents_withheld: false,
    exceptions: [],
  },
  {
    id: "9c3e7a12-6d4f-4b3a-ae53-8a1c4f6b9d13",
    name: "Managed OpenSearch",
    kind: "search",
    material: false,
    contract_ref: "INFRA-SEARCH-PROD-02",
    contract_owner: byOp("search").owners[0],
    dependents: [
      { dependency_id: "opensearch-query", function: "Execute product catalog queries", service: byOp("search") },
    ],
    dependents_withheld: false,
    exceptions: [],
  },
];

function reviewOf(service: CatalogServiceRecord, environmentClass: EnvironmentClass): ServiceReview {
  const now = new Date().toISOString();
  const production = environmentClass === "production";
  const isPayment = service.operational_name === "payment";
  const isInventory = service.operational_name === "inventory";

  if (!production) {
    return {
      service: { id: service.id, business_name: service.business_name, criticality: service.criticality, lifecycle: service.lifecycle },
      environment_class: environmentClass,
      live: true,
      health: { state: "unknown", evidence_status: "missing", evaluated_at: now, last_evidence_at: null },
      expectation: { status: "unknown", reason: "no_approved_objective", objectives: [] },
    };
  }

  if (isInventory) {
    return {
      service: { id: service.id, business_name: service.business_name, criticality: service.criticality, lifecycle: service.lifecycle },
      environment_class: environmentClass,
      live: true,
      health: { state: "normal", evidence_status: "current", evaluated_at: now, last_evidence_at: now },
      expectation: { status: "unknown", reason: "no_approved_objective", objectives: [] },
    };
  }

  return {
    service: { id: service.id, business_name: service.business_name, criticality: service.criticality, lifecycle: service.lifecycle },
    environment_class: environmentClass,
    live: true,
    health: {
      state: isPayment ? "degraded" : "normal",
      evidence_status: "current",
      evaluated_at: now,
      last_evidence_at: now,
    },
    expectation: {
      status: isPayment ? "breached" : "compliant",
      reason: "current",
      objectives: [{
        objective_id: `${service.operational_name}-availability`,
        sli_type: "availability",
        target: 0.995,
        window_days: 30,
        description: service.outcomes[0]?.expectation ?? "Service meets availability target.",
        approved: true,
        status: isPayment ? "breached" : "compliant",
        evidence_status: "current",
        attainment: isPayment ? 0.986 : 0.998,
        error_budget_consumed: isPayment ? 1.42 : 0.35,
        last_evidence_at: now,
      }],
    },
  };
}

/** The playground evaluates its own seeded telemetry, so every function is current. */
function confidence(): PlatformConfidence {
  const at = new Date().toISOString();
  return {
    confidence: "full",
    affected: [],
    functions: (["assess", "detect", "ingest"] as const).map((name) => ({ function: name, state: "current", last_success_at: at })),
    checked_at: at,
  };
}

function judged(service: CatalogServiceRecord, environmentClass: EnvironmentClass): ServiceReview {
  const review = reviewOf(service, environmentClass);
  const current = review.health?.evidence_status === "current";
  const openIssues = service.operational_name === "payment" && environmentClass === "production" ? 1 : 0;
  return {
    ...review,
    freshness: review.health && {
      status: review.health.evidence_status,
      last_evidence_at: review.health.last_evidence_at,
      age_seconds: review.health.last_evidence_at
        ? Math.max(0, Math.round((Date.now() - Date.parse(review.health.last_evidence_at)) / 1000)) : null,
      bound_seconds: 300,
    },
    issues: { status: "current", open: openIssues },
    fit_for_decision: current,
    unfit_reasons: current ? [] : [`evidence_${review.health?.evidence_status}`],
  };
}

function period() {
  return { as_of: new Date().toISOString(), freshness_seconds: 300 };
}

function page<T>(items: T[], offset = 0) {
  return { items: items.slice(offset, offset + catalogPageSize), total: items.length };
}

export const playgroundCatalog: CatalogRuntime = {
  async services(_tenant, offset, query) {
    const term = query.trim().toLowerCase();
    const matched = term.length === 0
      ? SERVICES
      : SERVICES.filter((svc) =>
          [svc.business_name, svc.operational_name, ...svc.aliases.map((a) => a.alias)].some((name) =>
            name.toLowerCase().includes(term))
          || svc.domain.toLowerCase().includes(term)
          || svc.owners.some((owner) => owner.team_name?.toLowerCase().includes(term)));
    return page(matched, offset);
  },
  async service(_tenant, id) {
    return SERVICES.find((s) => s.id === id) ?? null;
  },
  async coverage() {
    const now = new Date().toISOString();
    return page(
      SERVICES.map((svc) => ({
        ...svc,
        coverage_status: svc.operational_name === "inventory" ? "partial" : "covered",
        assessed_at: now,
        stale: false,
      })),
    );
  },
  async assessment(_tenant, id) {
    const svc = SERVICES.find((s) => s.id === id);
    if (!svc) return null;
    const now = new Date().toISOString();
    const hasObjective = svc.operational_name !== "inventory";
    return {
      status: hasObjective ? "covered" : "partial",
      assessed_at: now,
      bindings: [{ service_name: svc.operational_name, environment: "production" }],
      items: [
        { item: "traces", demonstrated: true, evidence_ref: "live telemetry", detail: "Recent spans and logs observed" },
        { item: "objective", demonstrated: hasObjective, evidence_ref: hasObjective ? `${svc.operational_name}-availability` : null, detail: hasObjective ? "Approved availability objective active" : "No active objective" },
      ],
    };
  },
  async gaps() {
    const inventory = byOp("inventory");
    return page([{
      id: "6f2a9c18-3b7d-4e51-9a42-1c8e5d7f3b90",
      service_id: inventory.id,
      journey_id: null,
      item: "objective",
      state: "open",
      remediation_owner_id: null,
      due_date: null,
      review_date: null,
      acceptance_rationale: null,
    }]);
  },
  async providers(_tenant, offset) {
    return page(PROVIDERS, offset);
  },
  async provider(_tenant, id) {
    return PROVIDERS.find((p) => p.id === id) ?? null;
  },
  async providerGaps() {
    return page([]);
  },
  async ownerlessAssets() {
    return page([]);
  },
  async review(_tenant, id, environmentClass) {
    const svc = SERVICES.find((s) => s.id === id);
    return svc ? { ...judged(svc, environmentClass), confidence: confidence(), period: period() } : null;
  },
  async reviews(_tenant, environmentClass, _scope, offset, _signal, options) {
    const basePeriod = period();
    const p = options?.from && options?.to
      ? { ...basePeriod, from: options.from, to: options.to, historical: true, definitions_as_of: options.to, as_of: options.to }
      : basePeriod;
    return { ...page(SERVICES.map((s) => judged(s, environmentClass)), offset), confidence: confidence(), period: p };
  },
  async overview(_tenant, environmentClass, offset) {
    const overviewItems = SERVICES.map((s) => {
      const r = judged(s, environmentClass);
      return {
        service: r.service,
        live: r.live,
        state: r.health?.state ?? null,
        freshness: r.freshness ?? null,
        issues: r.issues ?? null,
        fit_for_decision: Boolean(r.fit_for_decision),
        unfit_reasons: r.unfit_reasons ?? [],
      };
    });
    const { items, total } = page(overviewItems, offset);
    return {
      scope: { environment_class: environmentClass, context: "business_critical" as const },
      items,
      total,
      confidence: confidence(),
      period: period(),
    };
  },
};
