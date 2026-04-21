import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import { getServiceSummary, ServiceSummary } from "../api/services";

export default function ServiceDetailPage() {
  const { serviceId } = useParams({ from: "/services/$serviceId" });
  const serviceName = decodeURIComponent(serviceId);
  const { data, isLoading, isError } = useQuery({
    queryKey: ["service-summary", serviceName],
    queryFn: () => getServiceSummary(serviceName),
  });

  if (isLoading) {
    return <div className="loading-state">Loading service overview...</div>;
  }

  if (isError || !data) {
    return (
      <section className="page-stack">
        <Link to="/services" className="secondary-link">Back to services</Link>
        <div className="empty-panel">
          <div className="empty-title">Service not found</div>
          <div className="empty-metrics">
            <span>{serviceName}</span>
          </div>
        </div>
      </section>
    );
  }

  return <ServiceOverview service={data.service} />;
}

function ServiceOverview({ service }: { service: ServiceSummary }) {
  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="field-label">Service Overview</div>
          <h1>{service.service_name}</h1>
        </div>
        <Link to="/services" className="secondary-link">Back to services</Link>
      </div>

      <div className="metric-grid" aria-label="Service performance summary">
        <MetricTile label="Request Rate" value={`${service.request_rate.toFixed(2)} rps`} tone="info" />
        <MetricTile
          label="Error Rate"
          value={`${(service.error_rate * 100).toFixed(2)}%`}
          tone={service.health_state === "breach" ? "bad" : service.health_state === "watch" ? "warn" : "good"}
        />
        <MetricTile label="P95 Latency" value={`${Math.round(service.p95_latency_ms)}ms`} tone="good" />
        <MetricTile label="Active Alerts" value={String(service.active_alert_count)} tone={service.active_alert_count > 0 ? "warn" : "good"} />
      </div>

      <div className="detail-grid">
        <section className="detail-panel">
          <div className="detail-panel-header">
            <div>
              <div className="field-label">Health</div>
              <h2>Current State</h2>
            </div>
            <HealthStatus healthState={service.health_state} />
          </div>
          <dl className="definition-grid">
            <div>
              <dt>SLO / health state</dt>
              <dd>{healthLabel(service.health_state)}</dd>
            </div>
            <div>
              <dt>Latest deployment</dt>
              <dd>{service.latest_deployment ?? "No deployment marker"}</dd>
            </div>
            <div>
              <dt>Lookback</dt>
              <dd>Last 1h</dd>
            </div>
          </dl>
        </section>

        <section className="detail-panel">
          <div className="detail-panel-header">
            <div>
              <div className="field-label">Investigate</div>
              <h2>Signal Entry Points</h2>
            </div>
          </div>
          <div className="entry-link-grid" aria-label="Signal entry points">
            <a href={`/traces?service=${encodeURIComponent(service.service_name)}`} className="entry-link">
              Traces
            </a>
            <a href={`/logs?service=${encodeURIComponent(service.service_name)}`} className="entry-link">
              Logs
            </a>
            <a href={`/metrics?service=${encodeURIComponent(service.service_name)}`} className="entry-link">
              Metrics
            </a>
            <a href={`/infrastructure?service=${encodeURIComponent(service.service_name)}`} className="entry-link">
              Infrastructure
            </a>
          </div>
        </section>
      </div>
    </section>
  );
}

function HealthStatus({ healthState }: { healthState: ServiceSummary["health_state"] }) {
  if (healthState === "breach") return <span className="status bad">Breach</span>;
  if (healthState === "watch") return <span className="status warn">Watch</span>;
  return <span className="status good">Healthy</span>;
}

function healthLabel(healthState: ServiceSummary["health_state"]) {
  if (healthState === "breach") return "Breach";
  if (healthState === "watch") return "Watch";
  return "Healthy";
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
