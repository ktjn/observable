import { getRouteApi, Link } from "@tanstack/react-router";
import { DlRow } from "../../components/ui/dl-row";
import { EmptyState } from "../../components/ui/empty-state";
import { Panel } from "../../components/ui/panel";
import type { CatalogServiceRecord } from "../../runtime/catalogContract";
import { CatalogResult, CatalogTable, CoverageLabel, labelOf, principalName, ServiceLink, useCatalogQuery } from "./catalogUi";

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
