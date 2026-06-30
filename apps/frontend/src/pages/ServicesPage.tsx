import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import {
  getTopology,
  listServiceSummaries,
  listServices,
  type ServiceSummary,
  type TopologyEdge,
} from "../api/services";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { DataFreshness } from "../components/ui/data-freshness";
import { EmptyState } from "../components/ui/empty-state";
import { ErrorState } from "../components/ui/error-state";
import { LoadingState } from "../components/ui/loading-state";
import { MetricCard } from "../components/ui/metric-card";
import { PillFilter } from "../components/ui/pill-filter";
import { ErrorRateCell, LatencyCell } from "../components/ui/metric-cells";
import { TablePanel } from "../components/ui/table-panel";
import { TopologyMap } from "../components/topology/TopologyMap";
import { QueryFilterInput } from "../features/nlq/QueryFilterInput";
import { deriveViewFiltersFromIr, type NlqIrLike } from "../features/nlq/queryFilters";
import { useTenantContext } from "../hooks/useTenantContext";
import { LogExplorer } from "./LogSearch";

const SERVICES_BASE_IR: NlqIrLike = {
  operation: "catalog",
  signals: ["metrics"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};

type ViewMode = "list" | "topology";

// ── Services page ─────────────────────────────────────────────────────────────

export default function ServicesPage() {
  const [view, setView] = useState<ViewMode>("list");
  const [environment, setEnvironment] = useState<string>("all");
  const [search, setSearch] = useState("");
  const [healthFilter, setHealthFilter] = useState("all");
  const [sortBy, setSortBy] = useState<keyof ServiceSummary | "health">("service_name");
  const [sortOrder, setSortOrder] = useState<"asc" | "desc">("asc");
  const { tenantId } = useTenantContext();

  const { data: servicesData, isLoading, dataUpdatedAt } = useQuery({
    queryKey: ["services-summary", tenantId, environment],
    queryFn: () =>
      listServiceSummaries(tenantId, {
        environment: environment === "all" ? undefined : environment,
      }),
  });

  const items = servicesData?.items ?? [];

  // Health pill counts (ignore health filter so each pill shows how many match)
  const healthCounts = useMemo(() => {
    const base = search
      ? items.filter((s) => s.service_name.toLowerCase().includes(search.trim().toLowerCase()))
      : items;
    return base.reduce(
      (acc, s) => {
        if (s.health_state === "healthy") acc.healthy += 1;
        if (s.health_state === "watch") acc.watch += 1;
        if (s.health_state === "breach") acc.breach += 1;
        return acc;
      },
      { healthy: 0, watch: 0, breach: 0 },
    );
  }, [items, search]);

  const filteredAndSorted = useMemo(() => {
    let list = [...items];
    if (search) {
      const sv = search.trim().toLowerCase();
      list = list.filter((s) => s.service_name.toLowerCase().includes(sv));
    }
    if (healthFilter !== "all") {
      list = list.filter((s) => s.health_state === healthFilter);
    }
    list.sort((a, b) => {
      let valA: string | number = sortBy === "health" ? a.error_rate : (a[sortBy as keyof ServiceSummary] ?? "");
      let valB: string | number = sortBy === "health" ? b.error_rate : (b[sortBy as keyof ServiceSummary] ?? "");
      if (valA < valB) return sortOrder === "asc" ? -1 : 1;
      if (valA > valB) return sortOrder === "asc" ? 1 : -1;
      return 0;
    });
    return list;
  }, [items, search, healthFilter, sortBy, sortOrder]);

  const stats = useMemo(() => {
    if (items.length === 0) return { count: 0, avgP95: 0, avgError: 0, alertCount: 0 };
    const count = items.length;
    const avgP95 = items.reduce((acc, s) => acc + s.p95_latency_ms, 0) / count;
    const avgError = items.reduce((acc, s) => acc + s.error_rate, 0) / count;
    const alertCount = items.reduce((acc, s) => acc + s.active_alert_count, 0);
    return { count, avgP95, avgError, alertCount };
  }, [items]);

  function handleSort(column: keyof ServiceSummary | "health") {
    if (sortBy === column) {
      setSortOrder(sortOrder === "asc" ? "desc" : "asc");
    } else {
      setSortBy(column);
      setSortOrder("asc");
    }
  }

  function SortIndicator({ column }: { column: keyof ServiceSummary | "health" }) {
    if (sortBy !== column) return null;
    return <span aria-hidden="true"> {sortOrder === "asc" ? "▲" : "▼"}</span>;
  }

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Catalog</div>
          <h1>Services</h1>
        </div>
        <div className="flex items-center gap-3">
          <DataFreshness dataUpdatedAt={dataUpdatedAt} />
        <div className="flex items-center gap-1 border border-[var(--border)] rounded p-0.5">
          <button
            type="button"
            onClick={() => setView("list")}
            className={[
              "px-3 py-1 text-xs font-bold rounded transition-colors",
              view === "list"
                ? "bg-[var(--brand)] text-white"
                : "text-[var(--muted)] hover:text-[var(--text)]",
            ].join(" ")}
          >
            List
          </button>
          <button
            type="button"
            onClick={() => setView("topology")}
            className={[
              "px-3 py-1 text-xs font-bold rounded transition-colors",
              view === "topology"
                ? "bg-[var(--brand)] text-white"
                : "text-[var(--muted)] hover:text-[var(--text)]",
            ].join(" ")}
          >
            Topology
          </button>
        </div>
        </div>
      </div>

      <div className="toolbar-row">
        <QueryFilterInput
          baseIr={SERVICES_BASE_IR}
          placeholder='Filter services, e.g. "prod checkout services in watch"'
          onIr={(ir) => {
            const filters = deriveViewFiltersFromIr(ir, "services");
            setSearch(filters.text ?? "");
            setEnvironment(filters.environment ?? "all");
            setHealthFilter(filters.health ?? "all");
          }}
        />
      </div>

      {view === "list" && (
        <>
          <div className="toolbar-row flex-wrap gap-y-2">
            <PillFilter
              pills={(["all", "healthy", "watch", "breach"] as const).map((health) => {
                const count =
                  health === "all"
                    ? healthCounts.healthy + healthCounts.watch + healthCounts.breach
                    : healthCounts[health];
                const activeColor =
                  health === "breach" ? "var(--bad)" : health === "watch" ? "var(--warn)" : health === "healthy" ? "var(--good)" : "var(--brand)";
                return { key: health, label: health === "all" ? "All health" : health, count, activeColor };
              })}
              activeKey={healthFilter}
              onSelect={(key) => setHealthFilter(key)}
              rounded
              ariaLabel="Filter by health"
            />

            <input
              type="search"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search services…"
              className="ml-auto min-w-[180px] px-2.5 py-1 text-xs border border-[var(--border)] bg-transparent text-[var(--text)] placeholder:text-[var(--muted)] rounded focus:outline-none focus:border-[var(--brand)]"
            />
          </div>

          <div
            className="grid gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
            style={{ gridTemplateColumns: "repeat(4, minmax(140px, 1fr))" }}
            aria-label="Service summary"
          >
            <MetricCard label="Services" value={String(stats.count)} tone="info" />
            <MetricCard label="Active Alerts" value={String(stats.alertCount)} tone="warn" />
            <MetricCard label="Avg P95" value={`${Math.round(stats.avgP95)}ms`} tone="good" />
            <MetricCard
              label="Avg Error Rate"
              value={`${(stats.avgError * 100).toFixed(2)}%`}
              tone={stats.avgError > 0.01 ? "warn" : "good"}
            />
          </div>

          <TablePanel>
            {isLoading ? (
              <LoadingState>Loading services…</LoadingState>
            ) : filteredAndSorted.length === 0 ? (
              <EmptyState title="No services found" description="No services matched the current filters." />
            ) : (
              <table aria-label="Service catalog">
                <thead>
                  <tr>
                    <th onClick={() => handleSort("service_name")} className="sortable">
                      Service<SortIndicator column="service_name" />
                    </th>
                    <th onClick={() => handleSort("request_rate")} className="sortable">
                      RPS<SortIndicator column="request_rate" />
                    </th>
                    <th>Health</th>
                    <th onClick={() => handleSort("error_rate")} className="sortable">
                      Error Rate<SortIndicator column="error_rate" />
                    </th>
                    <th onClick={() => handleSort("p95_latency_ms")} className="sortable">
                      P95<SortIndicator column="p95_latency_ms" />
                    </th>
                    <th>Alerts</th>
                    <th>Latest Deploy</th>
                  </tr>
                </thead>
                <tbody>
                  {filteredAndSorted.map((row) => (
                    <ServiceRow key={row.service_name} row={row} />
                  ))}
                </tbody>
              </table>
            )}
          </TablePanel>
        </>
      )}

      {view === "topology" && (
        <TopologyView tenantId={tenantId} environment={environment} />
      )}
    </section>
  );
}

// ── Service list row ──────────────────────────────────────────────────────────

function ServiceRow({ row }: { row: ServiceSummary }) {
  const rowBorderClass =
    row.health_state === "breach"
      ? "border-l-2 border-l-[var(--bad)]"
      : row.health_state === "watch"
        ? "border-l-2 border-l-[var(--warn)]"
        : "";

  return (
    <tr className={rowBorderClass}>
      <td className="strong-cell">
        <Link to="/services/$serviceId" params={{ serviceId: row.service_name }}>
          {row.service_name}
        </Link>
      </td>
      <td>{row.request_rate.toFixed(2)}</td>
      <td>
        <HealthStatus healthState={row.health_state} />
      </td>
      <td>
        <ErrorRateCell value={row.error_rate} />
      </td>
      <td>
        <LatencyCell valueMs={row.p95_latency_ms} />
      </td>
      <td>
        {row.active_alert_count > 0 ? (
          <span style={{ color: "var(--warn)", fontWeight: 700 }}>{row.active_alert_count}</span>
        ) : (
          row.active_alert_count
        )}
      </td>
      <td>
        {row.latest_deployment ?? <span className="text-[var(--muted)]">--</span>}
      </td>
    </tr>
  );
}


function HealthStatus({ healthState }: { healthState: ServiceSummary["health_state"] }) {
  const tone = healthState === "breach" ? "bad" : healthState === "watch" ? "warn" : "good";
  const label = healthState === "breach" ? "Breach" : healthState === "watch" ? "Watch" : "Healthy";
  return <Badge tone={tone}>{label}</Badge>;
}

// ── Topology view ─────────────────────────────────────────────────────────────

function TopologyView({ tenantId, environment }: { tenantId: string; environment: string }) {
  const [focusedService, setFocusedService] = useState<string | null>(null);
  const [edgePopover, setEdgePopover] = useState<{
    edge: TopologyEdge;
    x: number;
    y: number;
  } | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["topology", tenantId, environment],
    queryFn: () =>
      getTopology(tenantId, { environment: environment === "all" ? undefined : environment }),
  });

  const { data: servicesData } = useQuery({
    queryKey: ["services", tenantId],
    queryFn: () => listServices(tenantId),
  });

  const allServiceNames = (servicesData?.items ?? []).filter((s) => s !== "");

  return (
    <>
      {focusedService && (
        <div className="flex gap-4 items-center py-2">
          <Button variant="secondary" onClick={() => setFocusedService(null)}>
            ← All services
          </Button>
          <span>Viewing: {focusedService}</span>
          <Link to="/services/$serviceId" params={{ serviceId: focusedService }}>
            → Service detail
          </Link>
        </div>
      )}

      <TablePanel className="overflow-hidden relative bg-[var(--surface-inset)] h-[calc(100vh-12rem)]">
        {isLoading ? (
          <LoadingState>Loading topology…</LoadingState>
        ) : error ? (
          <ErrorState title="Failed to load topology" description={String(error)} />
        ) : (
          <div className="relative h-full w-full flex flex-col">
            {edgePopover && (
              <div
                style={{
                  position: "absolute",
                  left: edgePopover.x,
                  top: edgePopover.y,
                  zIndex: 10,
                  background: "var(--surface)",
                  border: "1px solid var(--border)",
                  borderRadius: "4px",
                  padding: "0.5rem",
                  display: "flex",
                  flexDirection: "column",
                  gap: "0.25rem",
                }}
              >
                <a href={`/traces?caller=${encodeURIComponent(edgePopover.edge.caller)}&callee=${encodeURIComponent(edgePopover.edge.callee)}`}>
                  View Traces
                </a>
                <a href={`/logs?service=${encodeURIComponent(edgePopover.edge.caller)}`}>
                  View Logs
                </a>
              </div>
            )}
            {allServiceNames.length === 0 ? (
              <EmptyState title="No services found" description="No services found in the selected time range." />
            ) : (
              <div className="flex flex-col flex-1 gap-2 min-h-0">
                {(!data || data.edges.length === 0) && (
                  <p className="text-xs text-[var(--muted)] shrink-0">
                    No observed call relationships yet — services shown as standalone nodes.
                  </p>
                )}
                <div className="flex-1 min-h-0">
                  <TopologyMap
                    edges={data?.edges ?? []}
                    allServices={allServiceNames}
                    focusedService={focusedService}
                    onNodeClick={(svc) => {
                      setEdgePopover(null);
                      setFocusedService((prev) => (prev === svc ? null : svc));
                    }}
                    onEdgeClick={(edge, x, y) => setEdgePopover({ edge, x, y })}
                    onBackgroundClick={() => setEdgePopover(null)}
                  />
                </div>
              </div>
            )}
          </div>
        )}
      </TablePanel>

      {focusedService && (
        <section aria-label="Focused service logs">
          <LogExplorer
            key={focusedService}
            initialService={focusedService}
            lockedService
            showHeader={false}
            showServiceColumn={false}
            showPromote={false}
            tableAriaLabel="Focused service logs"
          />
        </section>
      )}
    </>
  );
}

