import { useState, type FormEvent } from "react";
import { getRouteApi, Link, useNavigate } from "@tanstack/react-router";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { Select, SelectOption } from "../../components/ui/select";
import { catalogPageSize } from "../../runtime/catalogContract";
import {
  CatalogResult, CatalogTable, CoverageLabel, EnvironmentClassPicker, ExpectationLabel, HealthLabel, labelOf, ProviderLink, reasonOf, ServiceLink,
  useCatalogQuery,
} from "./catalogUi";
import { catalogViews, validateCatalogSearch } from "./module";

const route = getRouteApi("/catalog");

export default function CatalogPage() {
  const { view, q } = route.useSearch();
  return (
    <div className="grid min-w-0 gap-4">
      <header>
        <h1 className="m-0 text-2xl font-bold">Service catalog</h1>
        <p className="mt-1 text-sm text-[var(--muted)]">Find service owners, monitoring coverage, and provider dependencies.</p>
      </header>
      <nav aria-label="Catalog views" className="flex flex-wrap gap-x-5 gap-y-2 border-b border-[var(--border)] pb-2">
        {catalogViews.map(({ id, label }) => (
          <Link key={id} to="/catalog" search={{ view: id === "services" ? undefined : id }}
            aria-current={(view ?? "services") === id ? "page" : undefined}
            className={(view ?? "services") === id ? "font-semibold underline underline-offset-4" : "text-[var(--muted)]"}>
            {label}
          </Link>
        ))}
      </nav>
      {view === "review" && <ServiceReviewReport />}
      {view === "coverage" && <CoverageList />}
      {view === "gaps" && <Gaps />}
      {view === "providers" && <Providers />}
      {view === "assets" && <OwnerlessAssets />}
      {!view && <ServiceList key={q} />}
    </div>
  );
}

function Pagination({ total, label }: { total: number; label: string }) {
  const search = route.useSearch();
  const navigate = useNavigate();
  const offset = search.offset ?? 0;
  if (total <= catalogPageSize && offset === 0) return null;
  const setPage = (value: number) => void navigate({ to: "/catalog", search: { ...search, offset: value || undefined } });
  return (
    <nav aria-label={label} className="flex flex-wrap items-center gap-3">
      <Button variant="secondary" disabled={offset === 0} onClick={() => setPage(Math.max(0, offset - catalogPageSize))}>Previous</Button>
      <span>{offset < total ? `${offset + 1}–${Math.min(offset + catalogPageSize, total)} of ${total}` : `${total} results`}</span>
      <Button variant="secondary" disabled={offset + catalogPageSize >= total} onClick={() => setPage(offset + catalogPageSize)}>Next</Button>
    </nav>
  );
}

function ServiceList() {
  const { q, offset = 0 } = route.useSearch();
  const navigate = useNavigate();
  const [input, setInput] = useState(q ?? "");
  const [invalid, setInvalid] = useState(false);
  const services = useCatalogQuery(["services", q, offset],
    (catalog, tenant, signal) => catalog.services(tenant, offset, q ?? "", signal));

  function search(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const term = input.trim();
    const next = validateCatalogSearch({ q: term }).q;
    setInvalid(Boolean(term && !next));
    if (!term || next) void navigate({ to: "/catalog", search: { q: next } });
  }

  return (
    <div className="grid gap-3">
      <form role="search" onSubmit={search} className="grid max-w-2xl gap-2">
        <label htmlFor="catalog-search" className="font-semibold">Search services</label>
        <div className="flex gap-2">
          <Input id="catalog-search" type="search" value={input} onChange={(event) => setInput(event.target.value)}
            placeholder="Business name, operational name, domain, or owner"
            aria-invalid={invalid || undefined} aria-describedby={invalid ? "catalog-search-hint" : undefined} />
          <Button type="submit">Search</Button>
          {q && <Button variant="secondary" onClick={() => void navigate({ to: "/catalog", search: {} })}>Clear</Button>}
        </div>
        {invalid && <p id="catalog-search-hint" role="alert" className="text-sm text-[var(--bad)]">Use at least two characters and keep the search term short.</p>}
      </form>
      <CatalogResult query={services} label="services">{({ items, total }) => (
        <>
          <CatalogTable title="Catalog services" empty={q ? "No matching services" : "No services on this page"}
            headings={["Business name", "Operational name", "Domain", "Criticality", "Lifecycle"]}
            rows={items.map((service) => ({ id: service.id, cells: [
              <ServiceLink id={service.id}>{service.business_name}</ServiceLink>, service.operational_name,
              service.domain || "Unassigned", labelOf(service.criticality), labelOf(service.lifecycle),
            ] }))} />
          <Pagination total={total} label="Service pages" />
        </>
      )}</CatalogResult>
    </div>
  );
}

function ServiceReviewReport() {
  const search = route.useSearch();
  const navigate = useNavigate();
  const environmentClass = search.env ?? "production";
  const scope = search.scope ?? "business_critical";
  const offset = search.offset ?? 0;
  const query = useCatalogQuery(["reviews", environmentClass, scope, offset],
    (catalog, tenant, signal) => catalog.reviews(tenant, environmentClass, scope, offset, signal));
  const update = (next: Partial<typeof search>) =>
    void navigate({ to: "/catalog", search: { ...search, offset: undefined, ...next } });
  return (
    <div className="grid gap-3">
      <div className="flex flex-wrap items-end gap-4">
        <EnvironmentClassPicker value={environmentClass}
          onChange={(value) => update({ env: value === "production" ? undefined : value })} />
        <label className="grid gap-1 text-sm">
          <span className="font-semibold">Services</span>
          <Select value={scope} onChange={(event) => update({ scope: event.target.value === "all" ? "all" : undefined })}>
            <SelectOption value="business_critical">Business-critical</SelectOption>
            <SelectOption value="all">All services</SelectOption>
          </Select>
        </label>
      </div>
      <CatalogResult query={query} label="service review">{({ items, total }) => (
        <>
          <CatalogTable title="Service review" empty="No services in this review"
            headings={["Service", "Lifecycle", "Health", "Health evidence", "Expectation", "Expectation basis"]}
            rows={items.map((review) => ({ id: review.service.id, cells: review.live && review.health && review.expectation
              ? [
                <ServiceLink id={review.service.id}>{review.service.business_name}</ServiceLink>, labelOf(review.service.lifecycle),
                <HealthLabel state={review.health.state} />, reasonOf(review.health.evidence_status),
                <ExpectationLabel status={review.expectation.status} />, reasonOf(review.expectation.reason),
              ]
              : [
                <ServiceLink id={review.service.id}>{review.service.business_name}</ServiceLink>, labelOf(review.service.lifecycle),
                "Not live", "—", "Not live", "—",
              ] }))} />
          <Pagination total={total} label="Review pages" />
        </>
      )}</CatalogResult>
    </div>
  );
}

function CoverageList() {
  const query = useCatalogQuery(["coverage"], (catalog, tenant, signal) => catalog.coverage(tenant, signal));
  return <CatalogResult query={query} label="coverage">{({ items }) => (
    <CatalogTable title="Monitoring coverage" empty="No coverage records"
      headings={["Service", "Coverage", "Last assessment"]}
      rows={items.map((service) => ({ id: service.id, cells: [
        <ServiceLink id={service.id}>{service.business_name}</ServiceLink>,
        <CoverageLabel status={service.stale ? "unknown" : service.coverage_status} />,
        service.assessed_at ? <>{new Date(service.assessed_at).toLocaleString()}{service.stale && " (Stale)"}</> : "Never assessed",
      ] }))} />
  )}</CatalogResult>;
}

function Gaps() {
  const query = useCatalogQuery(["gaps"], (catalog, tenant, signal) => catalog.gaps(tenant, signal));
  return <CatalogResult query={query} label="coverage gaps">{({ items }) => (
    <CatalogTable title="Coverage gaps" headings={["Required item", "Affected record", "State", "Remediation owner", "Due or review date", "Risk rationale"]}
      rows={items.map((gap) => ({ id: gap.id, cells: [
        labelOf(gap.item), gap.service_id ? <ServiceLink id={gap.service_id}>Service {gap.service_id}</ServiceLink> : `Journey ${gap.journey_id}`,
        labelOf(gap.state), gap.remediation_owner_id ?? "Unassigned", gap.review_date ?? gap.due_date ?? "Unscheduled",
        gap.acceptance_rationale ?? "—",
      ] }))} />
  )}</CatalogResult>;
}

function Providers() {
  const { offset = 0 } = route.useSearch();
  const providers = useCatalogQuery(["providers", offset], (catalog, tenant, signal) => catalog.providers(tenant, offset, signal));
  const gaps = useCatalogQuery(["provider-gaps"], (catalog, tenant, signal) => catalog.providerGaps(tenant, signal));
  return (
    <div className="grid gap-4">
      <CatalogResult query={providers} label="providers">{({ items, total }) => (
        <>
          <CatalogTable title="Providers" headings={["Provider", "Type", "Materiality"]}
            rows={items.map((provider) => ({ id: provider.id, cells: [
              <ProviderLink id={provider.id}>{provider.name}</ProviderLink>, labelOf(provider.kind), provider.material ? "Material" : "Non-material",
            ] }))} />
          <Pagination total={total} label="Provider pages" />
        </>
      )}</CatalogResult>
      <h2 className="text-lg font-semibold">Provider gaps</h2>
      <CatalogResult query={gaps} label="provider gaps">{({ items, total }) => (
        <>
          <CatalogTable title="Provider gaps" headings={["Provider", "Gap", "Affected function"]}
            rows={items.map(({ provider, provider_name, reason, dependent }) => ({
              id: `${provider?.id ?? dependent?.dependency_id}:${reason}`,
              cells: [provider ? <ProviderLink id={provider.id}>{provider.name}</ProviderLink> : provider_name, labelOf(reason), dependent?.function ?? "—"],
            }))} />
          {items.length < total && <p>Showing the first {items.length} of {total} provider gaps.</p>}
        </>
      )}</CatalogResult>
    </div>
  );
}

function OwnerlessAssets() {
  const query = useCatalogQuery(["ownerless-assets"], (catalog, tenant, signal) => catalog.ownerlessAssets(tenant, signal));
  return <CatalogResult query={query} label="ownerless assets">{({ items }) => (
    <CatalogTable title="Ownerless assets" headings={["Asset", "Type", "Component", "Service", "Owner state"]}
      rows={items.map((asset) => ({ id: `${asset.kind}:${asset.external_id}`, cells: [
        asset.name || asset.external_id, labelOf(asset.kind), asset.owning_component,
        <ServiceLink id={asset.service_id}>{asset.service_id}</ServiceLink>,
        asset.owner_state === "none" ? "Unassigned" : labelOf(asset.owner_state),
      ] }))} />
  )}</CatalogResult>;
}
