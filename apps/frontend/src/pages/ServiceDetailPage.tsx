import { useQuery } from "@tanstack/react-query";
import { Link, useLocation, useParams, useSearch } from "@tanstack/react-router";
import { getServiceSummary, ServiceSummary } from "../api/services";
import { ServiceInfraPanel } from "../components/ServiceInfraPanel";
import { listDeployments } from "../api/deployments";
import { DeploymentTimeline } from "../components/DeploymentTimeline";
import { Badge } from "../components/ui/badge";
import { EmptyState } from "../components/ui/empty-state";
import { LoadingState } from "../components/ui/loading-state";
import { MetricCard } from "../components/ui/metric-card";
import { Panel } from "../components/ui/panel";
import { NlqPanel } from "../features/nlq/NlqPanel";
import { ServiceMetricsWorkspace } from "../features/metrics/ServiceMetricsWorkspace";
import { LogExplorer } from "./LogSearch";
import { TraceExplorer } from "./TraceSearch";

export default function ServiceDetailPage() {
  const { serviceId } = useParams({ strict: false });
  if (!serviceId) {
    return <LoadingState>Loading service overview…</LoadingState>;
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
    return <LoadingState>Loading service overview…</LoadingState>;
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
    <ServiceDetailView
      service={data.service}
      activeTab={activeTab}
      lookbackMinutes={lookbackMinutes}
    />
  );
}

function ServiceDetailView({
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
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Service Overview</div>
          <h1>{service.service_name}</h1>
        </div>
        <Link to="/services" className="secondary-link">Back to services</Link>
      </div>

      <div
        className="grid grid-cols-[repeat(4,minmax(140px,1fr))] gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
        aria-label="Service performance summary"
      >
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
          <div className="grid grid-cols-2 gap-2.5" aria-label="Signal entry points">
            <a
              href={`/traces?service=${encodeURIComponent(service.service_name)}`}
              className="min-h-[54px] border border-[var(--border)] grid place-items-center text-[var(--text)] no-underline font-bold hover:border-[var(--brand)] hover:text-[var(--brand-strong)]"
            >
              Traces
            </a>
            <a
              href={`/logs?service=${encodeURIComponent(service.service_name)}`}
              className="min-h-[54px] border border-[var(--border)] grid place-items-center text-[var(--text)] no-underline font-bold hover:border-[var(--brand)] hover:text-[var(--brand-strong)]"
            >
              Logs
            </a>
            <a
              href={`/services/${encodeURIComponent(service.service_name)}/metrics?lookback_minutes=60`}
              className="min-h-[54px] border border-[var(--border)] grid place-items-center text-[var(--text)] no-underline font-bold hover:border-[var(--brand)] hover:text-[var(--brand-strong)]"
            >
              Metrics
            </a>
            <a
              href={`/infrastructure?service=${encodeURIComponent(service.service_name)}`}
              className="min-h-[54px] border border-[var(--border)] grid place-items-center text-[var(--text)] no-underline font-bold hover:border-[var(--brand)] hover:text-[var(--brand-strong)]"
            >
              Infrastructure
            </a>
          </div>
        </Panel>
      </div>

      <ServiceInfraPanel serviceName={service.service_name} />

      <Panel eyebrow="Ask" title="Natural Language Query">
        <NlqPanel
          serviceName={service.service_name}
          placeholder={`Ask about ${service.service_name}… e.g. "p99 latency over the last hour"`}
        />
      </Panel>

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
      <nav className="modern-signal-tabs" aria-label="Service signals">
        {tabLinks.map((link) => (
          <Link
            key={link.tab}
            to={link.to}
            params={{ serviceId: encodedService }}
            search={preservedSearch}
            className={activeTab === link.tab ? "modern-signal-tab active" : "modern-signal-tab"}
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
      {activeTab === "metrics" && <ServiceMetricsWorkspace serviceName={serviceName} />}
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
  return (
    <LogExplorer
      initialService={serviceName}
      lockedService
      initialLookbackMinutes={lookbackMinutes}
      showHeader={false}
      showServiceColumn={false}
      showPromote={false}
      tableAriaLabel="Service logs"
    />
  );
}

function ServiceTracesTab({
  serviceName,
  lookbackMinutes,
}: {
  serviceName: string;
  lookbackMinutes: number;
}) {
  return (
    <TraceExplorer
      initialService={serviceName}
      lockedService
      initialLookbackMinutes={lookbackMinutes}
      showHeader={false}
      showServiceColumn={false}
      showPromote={false}
      showFacets={false}
      tableAriaLabel="Service traces"
    />
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
