export interface CatalogPage<T> {
  items: T[];
  total: number;
}

export interface CatalogService {
  id: string;
  business_name: string;
  operational_name: string;
  domain: string;
  criticality: string;
  lifecycle: string;
}

export interface CatalogPrincipal {
  principal_kind: string;
  principal_id: string;
  team_name?: string;
  state: string;
}

export interface CatalogServiceRecord extends CatalogService {
  description: string;
  support_route: string;
  aliases: { alias: string; state: string }[];
  owners: (CatalogPrincipal & { role: string })[];
  outcomes: { id: string; statement: string; expectation: string; approved: boolean }[];
  exceptions: string[];
}

export type CoverageStatus = "covered" | "partial" | "uncovered" | "unknown";

export interface CatalogCoverage extends CatalogService {
  coverage_status: CoverageStatus;
  assessed_at: string | null;
  stale: boolean;
}

export interface CatalogAssessment {
  status: CoverageStatus;
  assessed_at: string;
  bindings: { service_name: string; environment: string }[];
  items: { item: string; demonstrated: boolean; evidence_ref: string | null; detail: string }[];
}

export interface CatalogGap {
  id: string;
  service_id: string | null;
  journey_id: string | null;
  item: string;
  state: string;
  remediation_owner_id: string | null;
  due_date: string | null;
  review_date: string | null;
  acceptance_rationale: string | null;
}

export interface CatalogProvider {
  id: string;
  name: string;
  kind: string;
  material: boolean;
}

interface CatalogDependent {
  dependency_id: string;
  function: string;
  service?: CatalogService;
  journey?: { name: string };
}

export interface CatalogProviderRecord extends CatalogProvider {
  contract_ref: string;
  contract_owner: CatalogPrincipal | null;
  dependents: CatalogDependent[];
  dependents_withheld: boolean;
  exceptions: string[];
}

export interface CatalogProviderGap {
  reason: string;
  provider?: CatalogProvider;
  provider_name?: string;
  dependent?: CatalogDependent;
}

export interface CatalogAsset {
  kind: string;
  external_id: string;
  owning_component: string;
  service_id: string;
  name: string;
  owner_state: string;
}

export const catalogPageSize = 50;

/** An explicit environment class; reviews never mix production and non-production. */
export type EnvironmentClass = "production" | "non_production";
export const environmentClasses: readonly EnvironmentClass[] = ["production", "non_production"];

/** Services a review report covers: business-critical live services, or every record. */
export type ReviewScope = "business_critical" | "all";

export type HealthState = "normal" | "degraded" | "unavailable" | "planned_maintenance" | "unknown";
export type ExpectationStatus = "compliant" | "at_risk" | "breached" | "unknown";

export interface ReviewObjective {
  objective_id: string;
  sli_type: string;
  target: number;
  window_days: number;
  description: string;
  approved: boolean;
  status: ExpectationStatus | null;
  evidence_status: string | null;
  attainment: number | null;
  error_budget_consumed: number | null;
  last_evidence_at: string | null;
}

/** `GET /v1/review/services/{id}?environment_class=` from observable-query (phase 1 C4). */
export interface ServiceReview {
  service: Pick<CatalogService, "id" | "business_name" | "criticality" | "lifecycle">;
  environment_class: EnvironmentClass;
  /** Retired and planned services are not live and carry no health or expectation. */
  live: boolean;
  health: {
    state: HealthState;
    evidence_status: string;
    evaluated_at: string | null;
    last_evidence_at: string | null;
  } | null;
  expectation: {
    status: ExpectationStatus;
    reason: string;
    objectives: ReviewObjective[];
  } | null;
}

export interface CatalogRuntime {
  services(tenant: string, offset: number, query: string, signal?: AbortSignal): Promise<CatalogPage<CatalogService>>;
  service(tenant: string, id: string, signal?: AbortSignal): Promise<CatalogServiceRecord | null>;
  coverage(tenant: string, signal?: AbortSignal): Promise<CatalogPage<CatalogCoverage>>;
  assessment(tenant: string, id: string, signal?: AbortSignal): Promise<CatalogAssessment | null>;
  gaps(tenant: string, signal?: AbortSignal): Promise<CatalogPage<CatalogGap>>;
  providers(tenant: string, offset: number, signal?: AbortSignal): Promise<CatalogPage<CatalogProvider>>;
  provider(tenant: string, id: string, signal?: AbortSignal): Promise<CatalogProviderRecord | null>;
  providerGaps(tenant: string, signal?: AbortSignal): Promise<CatalogPage<CatalogProviderGap>>;
  ownerlessAssets(tenant: string, signal?: AbortSignal): Promise<CatalogPage<CatalogAsset>>;
  review(tenant: string, id: string, environmentClass: EnvironmentClass, signal?: AbortSignal): Promise<ServiceReview | null>;
  reviews(tenant: string, environmentClass: EnvironmentClass, scope: ReviewScope, offset: number, signal?: AbortSignal): Promise<CatalogPage<ServiceReview>>;
}
