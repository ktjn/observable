type ProductArea =
  | "services"
  | "infrastructure"
  | "service-overview"
  | "dashboards"
  | "alerts"
  | "admin";

const pageCopy: Record<ProductArea, { title: string; kicker: string }> = {
  services: { title: "Services", kicker: "Catalog" },
  infrastructure: { title: "Infrastructure", kicker: "Inventory" },
  "service-overview": { title: "Service Overview", kicker: "Topology" },
  dashboards: { title: "Dashboards", kicker: "Saved Views" },
  alerts: { title: "Alerts & SLOs", kicker: "Reliability" },
  admin: { title: "Admin / Fleet / Billing", kicker: "Operations" },
};

const serviceRows = [
  { name: "checkout-api", owner: "Payments", health: "Healthy", errorRate: "0.08%", p95: "142ms", alerts: 0 },
  { name: "orders-worker", owner: "Commerce", health: "Watch", errorRate: "0.42%", p95: "318ms", alerts: 1 },
  { name: "identity", owner: "Platform", health: "Healthy", errorRate: "0.01%", p95: "88ms", alerts: 0 },
];

export function ProductAreaPage({ area }: { area: ProductArea }) {
  const copy = pageCopy[area];

  if (area === "services") {
    return (
      <section className="page-stack">
        <PageHeader kicker={copy.kicker} title={copy.title} />
        <div className="toolbar-row">
          <input className="search-input" placeholder="Search services" aria-label="Search services" />
          <select className="select-input" aria-label="Environment filter" defaultValue="prod">
            <option value="prod">prod</option>
            <option value="staging">staging</option>
            <option value="dev">dev</option>
          </select>
          <select className="select-input" aria-label="Health filter" defaultValue="all">
            <option value="all">All health</option>
            <option value="healthy">Healthy</option>
            <option value="watch">Watch</option>
            <option value="breach">Breach</option>
          </select>
        </div>
        <div className="metric-grid" aria-label="Service summary">
          <MetricTile label="Services" value="128" tone="info" />
          <MetricTile label="Active Alerts" value="7" tone="warn" />
          <MetricTile label="P95 Latency" value="184ms" tone="good" />
          <MetricTile label="Error Rate" value="0.18%" tone="good" />
        </div>
        <div className="table-panel">
          <table>
            <thead>
              <tr>
                <th>Service</th>
                <th>Owner</th>
                <th>Health</th>
                <th>Error Rate</th>
                <th>P95</th>
                <th>Alerts</th>
              </tr>
            </thead>
            <tbody>
              {serviceRows.map((row) => (
                <tr key={row.name}>
                  <td className="strong-cell">{row.name}</td>
                  <td>{row.owner}</td>
                  <td>
                    <span className={row.health === "Healthy" ? "status good" : "status warn"}>
                      {row.health}
                    </span>
                  </td>
                  <td>{row.errorRate}</td>
                  <td>{row.p95}</td>
                  <td>{row.alerts}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    );
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
          <span>Environment: dev</span>
          <span>Range: Last 1h</span>
        </div>
      </div>
    </section>
  );
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
