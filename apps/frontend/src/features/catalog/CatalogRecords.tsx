import { useMemo, useRef, useState } from "react";
import { getRouteApi, Link } from "@tanstack/react-router";
import { DlRow } from "../../components/ui/dl-row";
import { EmptyState } from "../../components/ui/empty-state";
import { Panel } from "../../components/ui/panel";
import {
  type CatalogServiceRecord,
  type EnvironmentClass,
  type ServiceReview,
} from "../../runtime/catalogContract";

type ReviewHealth = NonNullable<ServiceReview["health"]>;
import { ConfidenceNotice, FreshnessNote, StateLabel, unfitReason } from "../../components/ui/state";
import { useAuth } from "../../hooks/useAuth";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
import { useReadonlyViewSubscription } from "../../hooks/useReadonlyViewSubscription";
import { useTenantContext } from "../../hooks/useTenantContext";
import {
  deriveViewTimeRange,
  type ViewSubscriptionDefinition,
} from "../../runtime/viewSubscriptionProtocol";
import {
  CatalogResult, CatalogTable, classLabels, CoverageLabel, EnvironmentClassPicker, ExpectationLabel, labelOf, principalName,
  reasonOf, ServiceLink, useCatalogQuery,
} from "./catalogUi";
import { EvidenceAge, IssueCount } from "./ServiceOverview";

const serviceRoute = getRouteApi("/catalog/services/$serviceId");
const providerRoute = getRouteApi("/catalog/providers/$providerId");
const recordGrid = "grid grid-cols-[minmax(7rem,auto)_minmax(0,1fr)] gap-x-4 gap-y-2";

export function CatalogServicePage() {
  const { serviceId } = serviceRoute.useParams();
  const query = useCatalogQuery(["service", serviceId], (catalog, tenant, signal) => catalog.service(tenant, serviceId, signal));
  return (
    <div className="grid min-w-0 gap-4">
      <Link to="/catalog" className="w-fit underline">Back to service records</Link>
      <CatalogResult query={query} label="service">{(service) => service
        ? <ServiceRecord service={service} />
        : <EmptyState title="Service not found" />}
      </CatalogResult>
    </div>
  );
}

function ServiceRecord({ service }: { service: CatalogServiceRecord }) {
  return (
    <>
      <header>
        <h1 className="m-0 break-words text-2xl font-bold">{service.business_name}</h1>
        {service.description && <p className="mt-1 break-words text-sm text-[var(--muted)]">{service.description}</p>}
      </header>
      <div className="grid min-w-0 gap-4 lg:grid-cols-2">
        <Panel title="Service identity">
          <dl className={recordGrid}>
            <DlRow label="Operational name">{service.operational_name}</DlRow>
            <DlRow label="Domain">{service.domain || "Unassigned"}</DlRow>
            <DlRow label="Criticality">{labelOf(service.criticality)}</DlRow>
            <DlRow label="Lifecycle">{labelOf(service.lifecycle)}</DlRow>
            <DlRow label="Active aliases">{service.aliases.filter(({ state }) => state === "active").map(({ alias }) => alias).join(", ") || "None"}</DlRow>
          </dl>
        </Panel>
        <Panel title="Ownership">
          <dl className={recordGrid}>
            <DlRow label="Business owner">{principalName(service.owners.find(({ role }) => role === "business_owner"))}</DlRow>
            <DlRow label="Operational owner">{principalName(service.owners.find(({ role }) => role === "operational_owner"))}</DlRow>
            <DlRow label="Support route">{service.support_route || "Unassigned"}</DlRow>
          </dl>
        </Panel>
      </div>
      {service.exceptions.length > 0 && (
        <Panel title="Catalog exceptions">
          <ul className="list-inside list-disc">{service.exceptions.map((reason) => <li key={reason}>{labelOf(reason)}</li>)}</ul>
        </Panel>
      )}
      <Panel title="Monitoring coverage">
        {service.lifecycle === "retired"
          ? <p>Coverage assessments are unavailable for retired services.</p>
          : <ServiceAssessment serviceId={service.id} />}
      </Panel>
      <Panel title="Health and expectations">
        <ServiceReviewPanel serviceId={service.id} />
      </Panel>
      <Panel title="Business outcomes">
        <CatalogTable title="Business outcomes" headings={["Outcome", "Expectation", "Approval"]}
          rows={service.outcomes.map((outcome) => ({ id: outcome.id, cells: [
            outcome.statement, outcome.expectation, outcome.approved ? "Approved" : "Pending approval",
          ] }))} />
      </Panel>
    </>
  );
}

function ServiceAssessment({ serviceId }: { serviceId: string }) {
  // Coverage is requested only after the service lookup succeeds.
  const query = useCatalogQuery(["assessment", serviceId], (catalog, tenant, signal) => catalog.assessment(tenant, serviceId, signal));
  return <CatalogResult query={query} label="coverage">{(assessment) => assessment
    ? (
      <div className="grid gap-3">
        <dl className={recordGrid}>
          <DlRow label="Coverage"><CoverageLabel status={assessment.status} /></DlRow>
          <DlRow label="Assessed at">{new Date(assessment.assessed_at).toLocaleString()}</DlRow>
          <DlRow label="Telemetry bindings">{assessment.bindings.map((binding) =>
            binding.environment ? `${binding.service_name} (${binding.environment})` : binding.service_name).join(", ") || "None"}</DlRow>
        </dl>
        {assessment.status === "unknown" && <p>Coverage evidence could not be evaluated.</p>}
        <CatalogTable title="Required coverage" headings={["Required item", "Evidence status", "Details"]}
          rows={assessment.items.map((item) => ({ id: item.item, cells: [
            labelOf(item.item), item.demonstrated ? "Demonstrated" : "Unmet",
            <>{item.detail}{item.evidence_ref && <div className="text-[var(--muted)]">{item.evidence_ref}</div>}</>,
          ] }))} />
      </div>
    )
    : <p>Coverage is unavailable.</p>}
  </CatalogResult>;
}

function isHealthPatch(value: unknown): value is Partial<ReviewHealth> {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    typeof (value as { state?: unknown }).state === "string"
  );
}

function mergeHealthIntoReview(
  review: ServiceReview | null | undefined,
  healthPatch: Partial<ReviewHealth> | null,
): ServiceReview | null | undefined {
  if (!review || !review.live || !healthPatch) return review;
  return {
    ...review,
    health: {
      ...(review.health ?? { state: "unknown", evidence_status: "missing" }),
      ...healthPatch,
    } as ReviewHealth,
  };
}

function ServiceReviewPanel({ serviceId }: { serviceId: string }) {
  // The review is requested only after the service lookup succeeds, one class at a time.
  const [environmentClass, setEnvironmentClass] = useState<EnvironmentClass>("production");
  const { tenantId } = useTenantContext();
  const { data: user } = useAuth();
  const dateRange = useGlobalDateRange();
  const pushedHealthRef = useRef<Partial<ReviewHealth> | null>(null);

  const query = useCatalogQuery(["review", serviceId, environmentClass],
    (catalog, tenant, signal) => catalog.review(tenant, serviceId, environmentClass, signal));

  const reviewQueryKey = useMemo(
    () => ["catalog", user?.user_id, tenantId, "review", serviceId, environmentClass] as const,
    [user?.user_id, tenantId, serviceId, environmentClass],
  );

  const viewTimeRange = useMemo(
    () =>
      deriveViewTimeRange({
        preset: dateRange.preset,
        fromMs: dateRange.fromMs,
        toMs: dateRange.toMs,
      }),
    [dateRange.preset, dateRange.fromMs, dateRange.toMs],
  );

  const subscriptionViews = useMemo<readonly ViewSubscriptionDefinition[]>(
    () => [
      {
        viewId: "service-health",
        query: {
          kind: "service-health",
          serviceId,
          environmentClass,
        },
        timeRange: viewTimeRange,
        queryKey: reviewQueryKey,
        applyData: (current, data) => {
          if (!isHealthPatch(data)) return current;
          pushedHealthRef.current = data;
          return mergeHealthIntoReview(current as ServiceReview | null | undefined, data);
        },
      },
    ],
    [serviceId, environmentClass, viewTimeRange, reviewQueryKey],
  );

  const subscription = useReadonlyViewSubscription({
    tenantId,
    views: subscriptionViews,
    enabled: Boolean(user),
  });

  return (
    <div className="grid gap-3">
      <EnvironmentClassPicker value={environmentClass} onChange={(next) => {
        pushedHealthRef.current = null;
        setEnvironmentClass(next);
      }} />
      <CatalogResult query={query} label="health and expectations">{(review) => {
        const merged = mergeHealthIntoReview(review, pushedHealthRef.current);
        return merged
          ? (
            <ReviewDetail
              review={merged}
              updatedAt={Math.max(query.dataUpdatedAt, subscription.lastUpdatedAt ?? 0)}
              subscriptionStatus={subscription.status}
            />
          )
          : <p>Health and expectations are unavailable.</p>;
      }}
      </CatalogResult>
    </div>
  );
}

function ReviewDetail({
  review,
  updatedAt,
  subscriptionStatus,
}: {
  review: ServiceReview;
  updatedAt: number;
  subscriptionStatus?: string;
}) {
  const { health, expectation } = review;
  if (!review.live || !health || !expectation) {
    return <p>{labelOf(review.service.lifecycle)} services are not live and have no current health or expectation.</p>;
  }
  return (
    <div className="grid gap-3">
      <FreshnessNote scope={classLabels[review.environment_class]} freshnessSeconds={review.period?.freshness_seconds ?? review.freshness?.bound_seconds}
        updatedAt={updatedAt} subscriptionStatus={subscriptionStatus} />
      <ConfidenceNotice confidence={review.confidence} />
      <dl className={recordGrid}>
        <DlRow label="Health"><StateLabel state={health.state} reason={reasonOf(health.evidence_status)} /> {reasonOf(health.evidence_status)}</DlRow>
        <DlRow label="Last evidence"><>{health.last_evidence_at ? new Date(health.last_evidence_at).toLocaleString() : "None"}{" "}
          {review.freshness && <EvidenceAge freshness={review.freshness} />}</></DlRow>
        <DlRow label="Open issues"><IssueCount issues={review.issues} /></DlRow>
        <DlRow label="Decision">{review.fit_for_decision
          ? "Fit for decision"
          : `Not fit: ${(review.unfit_reasons ?? []).map(unfitReason).join("; ") || "fitness unavailable"}`}</DlRow>
        <DlRow label="Expectation"><ExpectationLabel status={expectation.status} /> {reasonOf(expectation.reason)}</DlRow>
      </dl>
      <CatalogTable title="Objectives" empty={`No objectives for ${classLabels[review.environment_class].toLowerCase()}`}
        headings={["Objective", "Target", "Window", "Approval", "Compliance", "Evidence"]}
        rows={expectation.objectives.map((objective) => ({ id: objective.objective_id, cells: [
          objective.description || labelOf(objective.sli_type),
          `${(objective.target * 100).toFixed(2)}%`,
          `${objective.window_days} days`,
          objective.approved ? "Approved" : "Pending approval",
          objective.approved && objective.status ? <ExpectationLabel status={objective.status} /> : "No result",
          objective.approved ? reasonOf(objective.evidence_status) : "—",
        ] }))} />
    </div>
  );
}

export function CatalogProviderPage() {
  const { providerId } = providerRoute.useParams();
  const query = useCatalogQuery(["provider", providerId], (catalog, tenant, signal) => catalog.provider(tenant, providerId, signal));
  return (
    <div className="grid min-w-0 gap-4">
      <Link to="/catalog" search={{ view: "providers" }} className="w-fit underline">Back to providers</Link>
      <CatalogResult query={query} label="provider">{(provider) => provider ? (
        <>
          <h1 className="m-0 break-words text-2xl font-bold">{provider.name}</h1>
          <Panel title="Provider record">
            <dl className={recordGrid}>
              <DlRow label="Type">{labelOf(provider.kind)}</DlRow>
              <DlRow label="Materiality">{provider.material ? "Material" : "Non-material"}</DlRow>
              <DlRow label="Contract reference">{provider.contract_ref || "Unassigned"}</DlRow>
              <DlRow label="Contract owner">{principalName(provider.contract_owner)}</DlRow>
              {provider.exceptions.length > 0 && <DlRow label="Gaps">{provider.exceptions.map(labelOf).join(", ")}</DlRow>}
            </dl>
          </Panel>
          <Panel title="Dependent functions">
            {provider.dependents_withheld && <p>Some dependent functions are unavailable in this context.</p>}
            <CatalogTable title="Dependent functions" headings={["Function", "Service or journey"]}
              rows={provider.dependents.map(({ dependency_id, function: name, service, journey }) => ({
                id: dependency_id, cells: [name, service ? <ServiceLink id={service.id}>{service.business_name}</ServiceLink> : journey?.name],
              }))} />
          </Panel>
        </>
      ) : <EmptyState title="Provider not found" />}</CatalogResult>
    </div>
  );
}
