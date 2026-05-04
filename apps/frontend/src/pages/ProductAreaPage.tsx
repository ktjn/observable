import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { listServiceSummaries, type ServiceSummary } from "../api/services";
import { Badge } from "../components/ui/badge";
import { LoadingState } from "../components/ui/loading-state";
import { MetricCard } from "../components/ui/metric-card";
import { Panel } from "../components/ui/panel";
import { Toolbar } from "../components/ui/toolbar";
import { QueryFilterInput } from "../features/nlq/QueryFilterInput";
import { deriveViewFiltersFromIr, type NlqIrLike } from "../features/nlq/queryFilters";
import { useTenantContext } from "../hooks/useTenantContext";

const SERVICES_BASE_IR: NlqIrLike = {
  operation: "catalog",
  signals: ["metrics"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};

export function ProductAreaPage() {
  const [environment, setEnvironment] = useState<string>("all");
  const [search, setSearch] = useState("");
  const [healthFilter, setHealthFilter] = useState("all");
  const [sortBy, setSortBy] = useState<keyof ServiceSummary | "health">("service_name");
  const [sortOrder, setSortOrder] = useState<"asc" | "desc">("asc");
  const { tenantId } = useTenantContext();

  const { data: servicesData, isLoading } = useQuery({
    queryKey: ["services", tenantId, environment],
    queryFn: () =>
      listServiceSummaries(tenantId, {
        environment: environment === "all" ? undefined : environment,
      }),
  });

  const filteredAndSortedServices = useMemo(() => {
    if (!servicesData?.items) return [];

    let items = [...servicesData.items];
    if (search) {
      const lowSearch = search.toLowerCase();
      items = items.filter((service) =>
        service.service_name.toLowerCase().includes(lowSearch),
      );
    }

    if (healthFilter !== "all") {
      items = items.filter((service) => service.health_state === healthFilter);
    }

    items.sort((a, b) => {
      let valA: string | number = a[sortBy as keyof ServiceSummary] ?? "";
      let valB: string | number = b[sortBy as keyof ServiceSummary] ?? "";

      if (sortBy === "health") {
        valA = a.error_rate;
        valB = b.error_rate;
      }

      if (valA < valB) return sortOrder === "asc" ? -1 : 1;
      if (valA > valB) return sortOrder === "asc" ? 1 : -1;
      return 0;
    });

    return items;
  }, [servicesData, search, healthFilter, sortBy, sortOrder]);

  const stats = useMemo(() => {
    if (!servicesData?.items || servicesData.items.length === 0) {
      return { count: 0, avgP95: 0, avgError: 0, alertCount: 0 };
    }

    const count = servicesData.items.length;
    const avgP95 =
      servicesData.items.reduce((acc, service) => acc + service.p95_latency_ms, 0) / count;
    const avgError =
      servicesData.items.reduce((acc, service) => acc + service.error_rate, 0) / count;
    const alertCount = servicesData.items.reduce(
      (acc, service) => acc + service.active_alert_count,
      0,
    );
    return { count, avgP95, avgError, alertCount };
  }, [servicesData]);

  function handleSort(column: keyof ServiceSummary | "health") {
    if (sortBy === column) {
      setSortOrder(sortOrder === "asc" ? "desc" : "asc");
    } else {
      setSortBy(column);
      setSortOrder("asc");
    }
  }

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Catalog</div>
          <h1>Services</h1>
        </div>
      </div>

      <Toolbar aria-label="Service filters">
        <QueryFilterInput
          baseIr={SERVICES_BASE_IR}
          placeholder='Filter services, e.g. "prod checkout services in watch" or raw NLQ IR JSON'
          onIr={(ir) => {
            const filters = deriveViewFiltersFromIr(ir, "services");
            setSearch(filters.text ?? "");
            setEnvironment(filters.environment ?? "all");
            setHealthFilter(filters.health ?? "all");
          }}
        />
      </Toolbar>

      <div
        className="grid grid-cols-[repeat(4,minmax(140px,1fr))] gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
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

      <Panel title="Service catalog" eyebrow="Health and performance">
        {isLoading ? (
          <LoadingState>Loading services…</LoadingState>
        ) : (
          <table>
            <thead>
              <tr>
                <th onClick={() => handleSort("service_name")} className="sortable">
                  Service
                </th>
                <th>RPS</th>
                <th onClick={() => handleSort("health")} className="sortable">
                  Health
                </th>
                <th onClick={() => handleSort("error_rate")} className="sortable">
                  Error Rate
                </th>
                <th onClick={() => handleSort("p95_latency_ms")} className="sortable">
                  P95
                </th>
                <th>Alerts</th>
              </tr>
            </thead>
            <tbody>
              {filteredAndSortedServices.map((row) => (
                <tr key={row.service_name} className="modern-table-row">
                  <td className="strong-cell">
                    <Link to="/services/$serviceId" params={{ serviceId: row.service_name }}>
                      {row.service_name}
                    </Link>
                  </td>
                  <td>{row.request_rate.toFixed(2)}</td>
                  <td>
                    <HealthStatus healthState={row.health_state} />
                  </td>
                  <td>{(row.error_rate * 100).toFixed(2)}%</td>
                  <td>{Math.round(row.p95_latency_ms)}ms</td>
                  <td>{row.active_alert_count}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>
    </section>
  );
}

function HealthStatus({ healthState }: { healthState: ServiceSummary["health_state"] }) {
  if (healthState === "breach") return <Badge tone="bad">Breach</Badge>;
  if (healthState === "watch") return <Badge tone="warn">Watch</Badge>;
  return <Badge tone="good">Healthy</Badge>;
}
