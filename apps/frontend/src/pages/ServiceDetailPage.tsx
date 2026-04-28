import { useQuery } from "@tanstack/react-query";
import { Link, useLocation, useParams, useSearch } from "@tanstack/react-router";
import { searchLogs } from "../api/logs";
import { listMetrics } from "../api/metrics";
import { getServiceSummary, ServiceSummary } from "../api/services";
import { searchTraces } from "../api/traces";
import { ServiceInfraPanel } from "../components/ServiceInfraPanel";
import { listDeployments } from "../api/deployments";
import { DeploymentTimeline } from "../components/DeploymentTimeline";
import { Badge } from "../components/ui/badge";
import { EmptyState } from "../components/ui/empty-state";
import { MetricCard } from "../components/ui/metric-card";
import { Panel } from "../components/ui/panel";

export default function ServiceDetailPage() {
  const { serviceId } = useParams({ strict: false });
  if (!serviceId) {
    return <div className="loading-state">Loading service overview...</div>;
  }
  const serviceName = decodeURIComponent(serviceId);
  const location = useLocation();
  const activeTab = signalTabFromPath(location.pathname);
  const search = useSearch({ strict: false }) as ServiceDetailSearch;
  const lookbackMinutes = readLookbackMinutes(search);
  const { data, isLoading, isError } = useQuery({
    queryKey: ["service-summary", serviceName, lookbackMinutes],
    queryFn: () => getServiceSummary(serviceName, { lookback_minutes: lookbackMinutes }),
  });

  if (isLoading) {
    return <div className="loading-state">Loading service overview...</div>;
  }

  if (isError || !data) {
    return (
      <section className="page-stack">
        <Link to="/services" className="secondary-link">Back to services</Link>
        <EmptyState title="Service not found" metadata={[serviceName]} />
      </section>
    );
  }

  return (
    <ServiceOverview
      service={data.service}
      activeTab={activeTab}
      lookbackMinutes={lookbackMinutes}
    />
  );
}

function ServiceOverview({
  service,
  activeTab,
  lookbackMinutes,
}: {
  service: ServiceSummary;
  activeTab: ServiceSignalTab;
  lookbackMinutes: number;
}) {
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
        <MetricCard label="Request Rate" value={`${service.request_rate.toFixed(2)} rps`} tone="info" />
        <MetricCard
          label="Error Rate"
          value={`${(service.error_rate * 100).toFixed(2)}%`}
          tone={service.health_state === "breach" ? "bad" : service.health_state === "watch" ? "warn" : "good"}
        />
        <MetricCard label="P95 Latency" value={`${Math.round(service.p95_latency_ms)}ms`} tone="good" />
        <MetricCard label="Active Alerts" value={String(service.active_alert_count)} tone={service.active_alert_count > 0 ? "warn" : "good"} />
      </div>

      <DeploymentTimelineSection
        serviceName={service.service_name}
        lookbackMinutes={lookbackMinutes}
      />

      <div className="detail-grid">
        <Panel
          eyebrow="Health"
          title="Current State"
          actions={<HealthStatus healthState={service.health_state} />}
        >
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
        </Panel>

        <Panel eyebrow="Investigate" title="Signal Entry Points">
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
        </Panel>
      </div>

      <ServiceInfraPanel serviceName={service.service_name} />
      <ServiceSignalTabs
        serviceName={service.service_name}
        activeTab={activeTab}
        lookbackMinutes={lookbackMinutes}
      />
    </section>
  );
}

type ServiceSignalTab = "overview" | "logs" | "metrics" | "traces";
type ServiceDetailSearch = {
  lookback_minutes?: number | string;
};

function readLookbackMinutes(search: ServiceDetailSearch): number {
  const raw = search.lookback_minutes;
  const parsed = typeof raw === "number" ? raw : Number(raw ?? 60);
  if (!Number.isFinite(parsed) || parsed <= 0) return 60;
  return Math.floor(parsed);
}

function signalTabFromPath(pathname: string): ServiceSignalTab {
  if (pathname.endsWith("/logs")) return "logs";
  if (pathname.endsWith("/metrics")) return "metrics";
  if (pathname.endsWith("/traces")) return "traces";
  return "overview";
}

function ServiceSignalTabs({
  serviceName,
  activeTab,
  lookbackMinutes,
}: {
  serviceName: string;
  activeTab: ServiceSignalTab;
  lookbackMinutes: number;
}) {
  const encodedService = encodeURIComponent(serviceName);
  const tabLinks = [
    { tab: "overview", label: "Overview", to: "/services/$serviceId" },
    { tab: "logs", label: "Logs", to: "/services/$serviceId/logs" },
    { tab: "metrics", label: "Metrics", to: "/services/$serviceId/metrics" },
    { tab: "traces", label: "Traces", to: "/services/$serviceId/traces" },
  ] as const;
  const preservedSearch = { lookback_minutes: lookbackMinutes };

  return (
    <Panel className="overflow-hidden">
      <nav className="modern-tab-list" aria-label="Service signals">
        {tabLinks.map((link) => (
          <Link
            key={link.tab}
            to={link.to}
            params={{ serviceId: encodedService }}
            search={preservedSearch}
            className={activeTab === link.tab ? "modern-tab-link active" : "modern-tab-link"}
            aria-current={activeTab === link.tab ? "page" : undefined}
          >
            {link.label}
          </Link>
        ))}
      </nav>
      {activeTab === "overview" && (
        <div className="signal-empty">
          {serviceName} · Last {lookbackMinutes}m
        </div>
      )}
      {activeTab === "logs" && (
        <ServiceLogsTab serviceName={serviceName} lookbackMinutes={lookbackMinutes} />
      )}
      {activeTab === "metrics" && <ServiceMetricsTab serviceName={serviceName} />}
      {activeTab === "traces" && (
        <ServiceTracesTab serviceName={serviceName} lookbackMinutes={lookbackMinutes} />
      )}
    </Panel>
  );
}

function ServiceLogsTab({
  serviceName,
  lookbackMinutes,
}: {
  serviceName: string;
  lookbackMinutes: number;
}) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["service", serviceName, "logs", lookbackMinutes],
    queryFn: () => searchLogs({ service: serviceName, lookback_minutes: lookbackMinutes, limit: 50 }),
  });

  if (isLoading) return <div className="loading-state">Loading service logs...</div>;
  if (error) return <div className="signal-empty">Logs could not be loaded.</div>;
  if (!data?.logs.length) return <div className="signal-empty">No logs found for {serviceName}.</div>;

  return (
    <div className="table-panel">
      <table aria-label="Service logs">
        <thead>
          <tr>
            <th>Timestamp</th>
            <th>Level</th>
            <th>Body</th>
            <th>Trace</th>
          </tr>
        </thead>
        <tbody>
          {data.logs.map((log) => (
            <tr key={log.log_id}>
              <td>{log.timestamp_unix_nano}</td>
              <td>{log.severity_text || log.severity_number}</td>
              <td>{typeof log.body === "string" ? log.body : JSON.stringify(log.body)}</td>
              <td>{log.trace_id ? log.trace_id.substring(0, 16) : "none"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ServiceMetricsTab({ serviceName }: { serviceName: string }) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["service", serviceName, "metrics"],
    queryFn: () => listMetrics({ service: serviceName }),
  });

  if (isLoading) return <div className="loading-state">Loading service metrics...</div>;
  if (error) return <div className="signal-empty">Metrics could not be loaded.</div>;
  if (!data?.series.length) {
    return <div className="signal-empty">No metric series found for {serviceName}.</div>;
  }

  return (
    <div className="table-panel">
      <table aria-label="Service metrics">
        <thead>
          <tr>
            <th>Metric</th>
            <th>Type</th>
            <th>Unit</th>
            <th>Environment</th>
          </tr>
        </thead>
        <tbody>
          {data.series.map((series) => (
            <tr key={series.metric_series_id}>
              <td>{series.metric_name}</td>
              <td>{series.metric_type}</td>
              <td>{series.unit || "none"}</td>
              <td>{series.environment || "default"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ServiceTracesTab({
  serviceName,
  lookbackMinutes,
}: {
  serviceName: string;
  lookbackMinutes: number;
}) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["service", serviceName, "traces", lookbackMinutes],
    queryFn: () =>
      searchTraces({ service: serviceName, lookback_minutes: lookbackMinutes, limit: 50 }),
  });

  if (isLoading) return <div className="loading-state">Loading service traces...</div>;
  if (error) return <div className="signal-empty">Traces could not be loaded.</div>;
  if (!data?.traces.length) return <div className="signal-empty">No traces found for {serviceName}.</div>;

  return (
    <div className="table-panel">
      <table aria-label="Service traces">
        <thead>
          <tr>
            <th>Trace ID</th>
            <th>Operation</th>
            <th>Duration</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          {data.traces.map((trace) => {
            const root = trace.spans[0];
            if (!root) return null;
            return (
              <tr key={trace.trace_id}>
                <td>
                  <Link to="/traces/$traceId" params={{ traceId: trace.trace_id }}>
                    {trace.trace_id.substring(0, 16)}
                  </Link>
                </td>
                <td>{root.operation_name}</td>
                <td>{(root.duration_ns / 1e6).toFixed(2)}ms</td>
                <td>{root.status_code}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function HealthStatus({ healthState }: { healthState: ServiceSummary["health_state"] }) {
  if (healthState === "breach") return <Badge tone="bad">Breach</Badge>;
  if (healthState === "watch") return <Badge tone="warn">Watch</Badge>;
  return <Badge tone="good">Healthy</Badge>;
}

function healthLabel(healthState: ServiceSummary["health_state"]) {
  if (healthState === "breach") return "Breach";
  if (healthState === "watch") return "Watch";
  return "Healthy";
}

function DeploymentTimelineSection({
  serviceName,
  lookbackMinutes,
}: {
  serviceName: string;
  lookbackMinutes: number;
}) {
  const nowMs = Date.now();
  const startMs = nowMs - lookbackMinutes * 60 * 1000;

  const { data } = useQuery({
    queryKey: ["deployments", serviceName, lookbackMinutes],
    queryFn: () =>
      listDeployments({
        service_name: serviceName,
        start_time: new Date(startMs).toISOString(),
        end_time: new Date(nowMs).toISOString(),
        limit: 20,
      }),
  });

  if (!data?.items.length) return null;

  return (
    <DeploymentTimeline
      markers={data.items}
      rangeStartMs={startMs}
      rangeEndMs={nowMs}
    />
  );
}
