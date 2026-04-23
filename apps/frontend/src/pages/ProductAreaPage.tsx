import { useState, useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { listServiceSummaries, listEnvironments, ServiceSummary } from "../api/services";

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
        <div className="toolbar-row">
          <input 
            className="search-input" 
            placeholder="Search services" 
            aria-label="Search services" 
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <select 
            className="select-input" 
            aria-label="Environment filter" 
            value={environment}
            onChange={(e) => setEnvironment(e.target.value)}
          >
            <option value="all">All environments</option>
            {envsData?.items.map(env => (
              <option key={env} value={env}>{env}</option>
            ))}
          </select>
          <select 
            className="select-input" 
            aria-label="Health filter" 
            value={healthFilter}
            onChange={(e) => setHealthFilter(e.target.value)}
          >
            <option value="all">All health</option>
            <option value="healthy">Healthy</option>
            <option value="watch">Watch</option>
            <option value="breach">Breach</option>
          </select>
        </div>
        
        <div className="metric-grid" aria-label="Service summary">
          <MetricTile label="Services" value={String(stats.count)} tone="info" />
          <MetricTile
            label="Active Alerts"
            value={String(servicesData?.items.reduce((acc, s) => acc + s.active_alert_count, 0) ?? 0)}
            tone="warn"
          />
          <MetricTile label="Avg P95" value={`${Math.round(stats.avgP95)}ms`} tone="good" />
          <MetricTile label="Avg Error Rate" value={(stats.avgError * 100).toFixed(2) + "%"} tone={stats.avgError > 0.01 ? "warn" : "good"} />
        </div>

        <div className="table-panel">
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
                  <tr key={row.service_name}>
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
        </div>
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
        <MetricTile label="Entities" value={area === "admin" ? "12" : "48"} tone="info" />
        <MetricTile label="Healthy" value="94%" tone="good" />
        <MetricTile label="Watch" value="5" tone="warn" />
        <MetricTile label="Breach" value="1" tone="bad" />
      </div>
      <div className="empty-panel">
        <div className="empty-title">{copy.title}</div>
        <div className="empty-metrics">
          <span>Tenant: local-dev</span>
          <span>Environment: {environment}</span>
          <span>Range: Last 1h</span>
        </div>
      </div>
    </section>
  );
}

function HealthStatus({ healthState }: { healthState: ServiceSummary["health_state"] }) {
  if (healthState === "breach") return <span className="status bad">Breach</span>;
  if (healthState === "watch") return <span className="status warn">Watch</span>;
  return <span className="status good">Healthy</span>;
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

function MetricTile({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: "good" | "warn" | "bad" | "info";
}) {
  return (
    <div className={`metric-tile ${tone}`}>
      <div className="metric-label">{label}</div>
      <div className="metric-value">{value}</div>
    </div>
  );
}
