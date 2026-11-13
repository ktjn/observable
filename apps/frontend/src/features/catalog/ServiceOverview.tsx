import { useEffect, useRef, useState } from "react";
import { getRouteApi, useNavigate } from "@tanstack/react-router";
import { Button } from "../../components/ui/button";
import { DlRow } from "../../components/ui/dl-row";
import { EmptyState } from "../../components/ui/empty-state";
import { ErrorState } from "../../components/ui/error-state";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { Select, SelectOption } from "../../components/ui/select";
import { TablePanel } from "../../components/ui/table-panel";
import {
  ConfidenceNotice, DataState, dataStateOf, FreshnessNote, StateLabel, stateLabels, unfitReason, type ServiceState,
} from "../../components/ui/state";
import type { OverviewItem, ReviewFreshness, ReviewIssues } from "../../runtime/catalogContract";
import { classLabels, EnvironmentClassPicker, reasonOf, ServiceLink, useCatalogQuery } from "./catalogUi";
import type { validateCatalogSearch } from "./module";

type CatalogSearch = ReturnType<typeof validateCatalogSearch>;

/** Auto-refresh period of the overview (OBS-24-08). */
export const OVERVIEW_REFRESH_MS = 30_000;

const route = getRouteApi("/catalog");

// Worst first: a service needing attention is never sorted below a normal one.
const severity: Record<ServiceState, number> = {
  unavailable: 0, degraded: 1, unknown: 2, planned_maintenance: 3, normal: 4,
};

type Sort = "state" | "name" | "age";

export function sortItems(items: OverviewItem[], sort: Sort): OverviewItem[] {
  const rank = (i: OverviewItem) => (i.state ? severity[i.state] : 5);
  // Unknown age sorts as oldest: missing evidence is never treated as fresh.
  const age = (i: OverviewItem) => i.freshness?.age_seconds ?? Number.POSITIVE_INFINITY;
  const byName = (a: OverviewItem, b: OverviewItem) => a.service.business_name.localeCompare(b.service.business_name);
  return [...items].sort((a, b) => {
    if (sort === "name") return byName(a, b);
    if (sort === "age") return age(b) - age(a) || byName(a, b);
    return rank(a) - rank(b) || byName(a, b);
  });
}

/** Services whose state changed between two answers, by id. */
export function changedServices(before: OverviewItem[], after: OverviewItem[]): string[] {
  const previous = new Map(before.map((i) => [i.service.id, i.state]));
  return after.filter((i) => previous.has(i.service.id) && previous.get(i.service.id) !== i.state).map((i) => i.service.business_name);
}

function age(seconds: number | null | undefined) {
  if (seconds == null) return null;
  if (seconds < 60) return `${seconds} s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)} h ago`;
  return `${Math.floor(seconds / 86_400)} d ago`;
}

export function EvidenceAge({ freshness }: { freshness: ReviewFreshness | null | undefined }) {
  const kind = dataStateOf(freshness?.status);
  const when = age(freshness?.age_seconds);
  if (!kind) return <span>{when ?? "—"}</span>;
  return <span><DataState compact kind={kind} />{when && ` · last ${when}`}</span>;
}

/** Open issues; an unread list is a data state, never zero. */
export function IssueCount({ issues }: { issues: ReviewIssues | null | undefined }) {
  if (issues?.status === "current" && issues.open != null) {
    return <span>{issues.open === 0 ? "None open" : `${issues.open} open`}</span>;
  }
  return <DataState compact kind={dataStateOf(issues?.status ?? "evaluation_unavailable") ?? "unavailable"} />;
}

function Fitness({ item }: { item: Pick<OverviewItem, "fit_for_decision" | "unfit_reasons"> }) {
  if (item.fit_for_decision) return <span>Fit for decision</span>;
  return <span>Not fit: {item.unfit_reasons.map(unfitReason).join("; ") || "reason unavailable"}</span>;
}

export function ServiceOverview() {
  const search: CatalogSearch = route.useSearch();
  const navigate = useNavigate();
  const environmentClass = search.env ?? "production";
  const sort: Sort = search.sort ?? "state";
  const offset = search.offset ?? 0;
  const query = useCatalogQuery(["overview", environmentClass, offset],
    (catalog, tenant, signal) => catalog.overview(tenant, environmentClass, offset, signal), OVERVIEW_REFRESH_MS);
  const update = (next: Partial<CatalogSearch>) =>
    void navigate({ to: "/catalog", search: { ...search, ...next }, replace: true });

  // Change notice: which services changed state on the last refresh of the same scope.
  const previous = useRef<{ key: string; items: OverviewItem[] } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const scopeKey = `${environmentClass}:${offset}`;
  useEffect(() => {
    const items = query.data?.items;
    if (!items || query.isPlaceholderData) return;
    const before = previous.current;
    if (before && before.key === scopeKey) {
      const changed = changedServices(before.items, items);
      const at = new Date(query.dataUpdatedAt).toLocaleTimeString();
      setNotice(changed.length ? `Updated ${at}: ${changed.join(", ")} changed state.` : `Updated ${at}: no state changes.`);
    } else {
      setNotice(null);
    }
    previous.current = { key: scopeKey, items };
  }, [query.data, query.dataUpdatedAt, query.isPlaceholderData, scopeKey]);

  const data = query.data;
  const visible = data ? sortItems(data.items.filter((i) => !search.state || i.state === search.state), sort) : [];
  const selected = data?.items.find((i) => i.service.id === search.selected);

  return (
    <div className="grid gap-3">
      <div className="flex flex-wrap items-end gap-4">
        <EnvironmentClassPicker value={environmentClass}
          onChange={(value) => update({ env: value === "production" ? undefined : value, offset: undefined, selected: undefined })} />
        <label className="grid gap-1 text-sm">
          <span className="font-semibold">Sort by</span>
          <Select value={sort} onChange={(event) => {
            const value = event.target.value;
            update({ sort: value === "name" || value === "age" ? value : undefined });
          }}>
            <SelectOption value="state">Worst state first</SelectOption>
            <SelectOption value="name">Service name</SelectOption>
            <SelectOption value="age">Oldest evidence first</SelectOption>
          </Select>
        </label>
        <label className="grid gap-1 text-sm">
          <span className="font-semibold">State</span>
          <Select value={search.state ?? ""} onChange={(event) => {
            const value = event.target.value as ServiceState | "";
            update({ state: value || undefined });
          }}>
            <SelectOption value="">All states</SelectOption>
            {(Object.keys(stateLabels) as ServiceState[]).map((state) => (
              <SelectOption key={state} value={state}>{stateLabels[state]}</SelectOption>
            ))}
          </Select>
        </label>
        <Button variant="secondary" onClick={() => void query.refetch()} disabled={query.isFetching}>
          {query.isFetching && data ? "Refreshing…" : "Refresh now"}
        </Button>
      </div>
      <div aria-live="polite" className="min-h-0">
        {notice && <p role="status" className="m-0 text-sm font-semibold">{notice}</p>}
      </div>
      {!data && query.isPending && <div role="status"><LoadingState>Loading service overview…</LoadingState></div>}
      {!data && query.isError && (
        <ErrorState title="Could not load service overview" description="Health, issues and confidence are unavailable. Nothing here is shown as normal."
          actions={<Button variant="secondary" onClick={() => void query.refetch()}>Retry</Button>} />
      )}
      {data && (
        <>
          {query.isError && <DataState kind="unavailable">The last refresh failed; the view below is from {new Date(query.dataUpdatedAt).toLocaleTimeString()}.</DataState>}
          <FreshnessNote
            scope={`${classLabels[data.scope.environment_class]} · Business-critical services`}
            freshnessSeconds={data.period.freshness_seconds}
            updatedAt={query.dataUpdatedAt}
            refreshSeconds={OVERVIEW_REFRESH_MS / 1000} />
          <ConfidenceNotice confidence={data.confidence} />
          {visible.length === 0
            ? <EmptyState compact title={search.state ? `No ${stateLabels[search.state].toLowerCase()} services` : "No business-critical services"} />
            : (
              <TablePanel>
                <table aria-label="Service overview" className="w-full text-left">
                  <thead><tr>
                    {["Service", "State", "Last evidence", "Open issues", "Decision"].map((h) => <th key={h} scope="col">{h}</th>)}
                    <th scope="col"><span className="sr-only">Inspect</span></th>
                  </tr></thead>
                  <tbody>{visible.map((item) => (
                    <tr key={item.service.id} aria-selected={item.service.id === search.selected || undefined}
                      className={item.service.id === search.selected ? "bg-[var(--surface-subtle)]" : undefined}>
                      <td><ServiceLink id={item.service.id}>{item.service.business_name}</ServiceLink></td>
                      <td>{item.live && item.state
                        ? <StateLabel state={item.state} reason={reasonOf(item.freshness?.status)} />
                        : "Not live"}</td>
                      <td><EvidenceAge freshness={item.freshness} /></td>
                      <td>{item.live ? <IssueCount issues={item.issues} /> : "—"}</td>
                      <td><Fitness item={item} /></td>
                      <td>
                        <Button variant="secondary" aria-pressed={item.service.id === search.selected}
                          aria-label={`Inspect ${item.service.business_name}`}
                          onClick={() => update({ selected: item.service.id === search.selected ? undefined : item.service.id })}>
                          Inspect
                        </Button>
                      </td>
                    </tr>
                  ))}</tbody>
                </table>
              </TablePanel>
            )}
          {search.selected && (
            <Panel title="Inspection">
              {selected ? <Inspection item={selected} /> : <p>This service is not in the current view.</p>}
            </Panel>
          )}
        </>
      )}
    </div>
  );
}

function Inspection({ item }: { item: OverviewItem }) {
  const grid = "grid grid-cols-[minmax(7rem,auto)_minmax(0,1fr)] gap-x-4 gap-y-2";
  return (
    <dl className={grid} aria-label={`Inspection of ${item.service.business_name}`}>
      <DlRow label="Service"><ServiceLink id={item.service.id}>{item.service.business_name}</ServiceLink></DlRow>
      <DlRow label="State">{item.live && item.state ? <StateLabel state={item.state} reason={reasonOf(item.freshness?.status)} /> : "Not live"}</DlRow>
      <DlRow label="Evidence">{reasonOf(item.freshness?.status)}</DlRow>
      <DlRow label="Last evidence">{item.freshness?.last_evidence_at ? new Date(item.freshness.last_evidence_at).toLocaleString() : "None"}</DlRow>
      <DlRow label="Open issues"><IssueCount issues={item.issues} /></DlRow>
      <DlRow label="Decision"><Fitness item={item} /></DlRow>
    </dl>
  );
}
