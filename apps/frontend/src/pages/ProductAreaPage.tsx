import { useState, useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { listServiceSummaries, listEnvironments, type ServiceSummary } from "../api/services";
import { Badge } from "../components/ui/badge";
import { EmptyState } from "../components/ui/empty-state";
import { Input } from "../components/ui/input";
import { MetricCard } from "../components/ui/metric-card";
import { Panel } from "../components/ui/panel";
import { Select, SelectOption } from "../components/ui/select";
import { Toolbar } from "../components/ui/toolbar";

type ProductArea =
  | "services"
  | "dashboards"
  | "alerts"
  | "admin";

const pageCopy: Record<ProductArea, { title: string; kicker: string }> = {
  services: { title: "Services", kicker: "Catalog" },
  dashboards: { title: "Dashboards", kicker: "Saved Views" },
  alerts: { title: "Alerts & SLOs", kicker: "Reliability" },
  admin: { title: "Admin / Fleet / Billing", kicker: "Operations" },
};

export function ProductAreaPage({ area }: { area: ProductArea }) {
  const copy = pageCopy[area];
  
  // State for filters
  const [environment, setEnvironment] = useState<string>("all");
  const [search, setSearch] = useState("");
  const [healthFilter, setHealthFilter] = useState("all");
  const [sortBy, setSortBy] = useState<keyof ServiceSummary | "health">("service_name");
  const [sortOrder, setSortOrder] = useState<"asc" | "desc">("asc");

  // Fetch data
  const { data: envsData } = useQuery({
    queryKey: ["environments"],
    queryFn: () => listEnvironments(),
  });

  const { data: servicesData, isLoading } = useQuery({
    queryKey: ["services", environment],
    queryFn: () => listServiceSummaries({ 
      environment: environment === "all" ? undefined : environment 
    }),
    enabled: area === "services",
  });

  const filteredAndSortedServices = useMemo(() => {
    if (!servicesData?.items) return [];

    let items = [...servicesData.items];

    // Search filter
    if (search) {
      const lowSearch = search.toLowerCase();
      items = items.filter(s => s.service_name.toLowerCase().includes(lowSearch));
    }

    // Health filter (derived from error rate for now)
    if (healthFilter !== "all") {
      items = items.filter(s => {
        const health = s.error_rate > 0.05 ? "breach" : s.error_rate > 0.01 ? "watch" : "healthy";
        return health === healthFilter;
      });
    }

    // Sort
    items.sort((a, b) => {
      let valA: string | number = a[sortBy as keyof ServiceSummary] ?? "";
      let valB: string | number = b[sortBy as keyof ServiceSummary] ?? "";
      
      if (sortBy === "health") {
        valA = a.error_rate; // use error rate as proxy for health sorting
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
      return { count: 0, avgP95: 0, avgError: 0 };
    }
    const count = servicesData.items.length;
    const avgP95 = servicesData.items.reduce((acc, s) => acc + s.p95_latency_ms, 0) / count;
    const avgError = servicesData.items.reduce((acc, s) => acc + s.error_rate, 0) / count;
    return { count, avgP95, avgError };
  }, [servicesData]);

  if (area === "services") {
    return (
      <section className="page-stack">
        <PageHeader kicker={copy.kicker} title={copy.title} />
        <Toolbar aria-label="Service filters">
          <Input
            className="max-w-[360px]"
            placeholder="Search services"
            aria-label="Search services"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <Select
            aria-label="Environment filter"
            value={environment}
            onChange={(e) => setEnvironment(e.target.value)}
          >
            <SelectOption value="all">All environments</SelectOption>
            {envsData?.items.map(env => (
              <SelectOption key={env} value={env}>{env}</SelectOption>
            ))}
          </Select>
          <Select
            aria-label="Health filter"
            value={healthFilter}
            onChange={(e) => setHealthFilter(e.target.value)}
          >
            <SelectOption value="all">All health</SelectOption>
            <SelectOption value="healthy">Healthy</SelectOption>
            <SelectOption value="watch">Watch</SelectOption>
            <SelectOption value="breach">Breach</SelectOption>
          </Select>
        </Toolbar>
        
        <div className="metric-grid" aria-label="Service summary">
          <MetricCard label="Services" value={String(stats.count)} tone="info" />
          <MetricCard
            label="Active Alerts"
            value={String(servicesData?.items.reduce((acc, s) => acc + s.active_alert_count, 0) ?? 0)}
            tone="warn"
          />
          <MetricCard label="Avg P95" value={`${Math.round(stats.avgP95)}ms`} tone="good" />
          <MetricCard
            label="Avg Error Rate"
            value={(stats.avgError * 100).toFixed(2) + "%"}
            tone={stats.avgError > 0.01 ? "warn" : "good"}
          />
        </div>

        <Panel title="Service catalog" eyebrow="Health and performance">
          {isLoading ? (
            <div className="loading-state">Loading services...</div>
          ) : (
            <table>
              <thead>
                <tr>
                  <th onClick={() => handleSort("service_name")} className="sortable">Service</th>
                  <th>RPS</th>
                  <th onClick={() => handleSort("health")} className="sortable">Health</th>
                  <th onClick={() => handleSort("error_rate")} className="sortable">Error Rate</th>
                  <th onClick={() => handleSort("p95_latency_ms")} className="sortable">P95</th>
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
      <PageHeader kicker={copy.kicker} title={copy.title} />
      <div className="metric-grid" aria-label={`${copy.title} summary`}>
        <MetricCard label="Entities" value={area === "admin" ? "12" : "48"} tone="info" />
        <MetricCard label="Healthy" value="94%" tone="good" />
        <MetricCard label="Watch" value="5" tone="warn" />
        <MetricCard label="Breach" value="1" tone="bad" />
      </div>
      <EmptyState
        title={copy.title}
        description="This workspace will use the same dense operational layout as the service catalog."
        metadata={["Tenant: local-dev", `Environment: ${environment}`, "Range: Last 1h"]}
      />
    </section>
  );
}

function HealthStatus({ healthState }: { healthState: ServiceSummary["health_state"] }) {
  if (healthState === "breach") return <Badge tone="bad">Breach</Badge>;
  if (healthState === "watch") return <Badge tone="warn">Watch</Badge>;
  return <Badge tone="good">Healthy</Badge>;
}

function PageHeader({ kicker, title }: { kicker: string; title: string }) {
  return (
    <div className="page-header">
      <div>
        <div className="field-label">{kicker}</div>
        <h1>{title}</h1>
      </div>
    </div>
  );
}
