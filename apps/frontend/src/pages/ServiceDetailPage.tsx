import { useQuery } from "@tanstack/react-query";
import { Link, useLocation, useParams } from "@tanstack/react-router";
import { useState } from "react";
import { listDeployments } from "../api/deployments";
import {
  getServiceResponseTimeHistory,
  getServiceSummary,
  ServiceSummary,
} from "../api/services";
import { Badge } from "../components/ui/badge";
import { EmptyState } from "../components/ui/empty-state";
import { LoadingState } from "../components/ui/loading-state";
import { MetricCard } from "../components/ui/metric-card";
import { Panel } from "../components/ui/panel";
import {
  TimeSeriesGraph,
  TimeSeriesSeries,
} from "../components/ui/time-series-graph";
import { NlqPanel } from "../features/nlq/NlqPanel";
import type { VisualizationFrame } from "../api/nlq";
import { VisualizationPanel } from "../features/nlq/VisualizationPanel";
import { ServiceMetricsWorkspace } from "../features/metrics/ServiceMetricsWorkspace";
import { ServiceInfraPanel } from "../components/ServiceInfraPanel";
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";
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
  const { fromMs, toMs } = useGlobalDateRange();

  const { data, isLoading, isError } = useQuery({
    queryKey: ["service-summary", serviceName, fromMs, toMs],
    queryFn: () => getServiceSummary(serviceName, { from: fromMs, to: toMs }),
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
      fromMs={fromMs}
      toMs={toMs}
    />
  );
}

function ServiceDetailView({
  service,
  activeTab,
  fromMs,
  toMs,
}: {
  service: ServiceSummary;
  activeTab: ServiceSignalTab;
  fromMs: number;
  toMs: number;
}) {
  const [nlqFrame, setNlqFrame] = useState<VisualizationFrame | null>(null);
  const [nlqTab, setNlqTab] = useState<ServiceSignalTab | null>(null);
  const displayedTab = nlqTab ?? activeTab;

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

      <ResponseTimeGraphSection
        serviceName={service.service_name}
        fromMs={fromMs}
        toMs={toMs}
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
              <dt>Time window</dt>
              <dd>{describeRange(fromMs, toMs)}</dd>
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
          suppressFrameResult
          onFrameResult={(frame) => {
            const tab = signalTabFromFrame(frame);
            setNlqFrame(frame);
            setNlqTab(tab);
          }}
        />
      </Panel>

      <ServiceSignalTabs
        serviceName={service.service_name}
        activeTab={displayedTab}
        nlqFrame={nlqFrame}
      />
    </section>
  );
}

type ServiceSignalTab = "logs" | "metrics" | "traces";

function signalTabFromPath(pathname: string): ServiceSignalTab {
  if (pathname.endsWith("/metrics")) return "metrics";
  if (pathname.endsWith("/traces")) return "traces";
  return "logs";
}

function describeRange(fromMs: number, toMs: number): string {
  const minutes = Math.round((toMs - fromMs) / 60_000);
  if (minutes < 60) return `Last ${minutes}m`;
  const hours = Math.round(minutes / 60);
  return `Last ${hours}h`;
}

function ServiceSignalTabs({
  serviceName,
  activeTab,
  nlqFrame,
}: {
  serviceName: string;
  activeTab: ServiceSignalTab;
  nlqFrame: VisualizationFrame | null;
}) {
  const encodedService = encodeURIComponent(serviceName);
  const tabLinks = [
    { tab: "logs" as const,    label: "Logs",    to: "/services/$serviceId/logs" },
    { tab: "metrics" as const, label: "Metrics", to: "/services/$serviceId/metrics" },
    { tab: "traces" as const,  label: "Traces",  to: "/services/$serviceId/traces" },
  ];

  return (
    <Panel className="overflow-hidden">
      <nav className="modern-signal-tabs" aria-label="Service signals">
        {tabLinks.map((link) => (
          <Link
            key={link.tab}
            to={link.to}
            params={{ serviceId: encodedService }}
            className={activeTab === link.tab ? "modern-signal-tab active" : "modern-signal-tab"}
            aria-current={activeTab === link.tab ? "page" : undefined}
          >
            {link.label}
          </Link>
        ))}
      </nav>
      {activeTab === "logs" && (
        nlqFrame && signalTabFromFrame(nlqFrame) === "logs" ? (
          <NlqTabFrame frame={nlqFrame} />
        ) : (
          <ServiceLogsTab serviceName={serviceName} />
        )
      )}
      {activeTab === "metrics" && (
        nlqFrame && signalTabFromFrame(nlqFrame) === "metrics" ? (
          <NlqTabFrame frame={nlqFrame} />
        ) : (
          <ServiceMetricsWorkspace serviceName={serviceName} />
        )
      )}
      {activeTab === "traces" && (
        nlqFrame && signalTabFromFrame(nlqFrame) === "traces" ? (
          <NlqTabFrame frame={nlqFrame} />
        ) : (
          <ServiceTracesTab serviceName={serviceName} />
        )
      )}
    </Panel>
  );
}

function signalTabFromFrame(frame: VisualizationFrame): ServiceSignalTab {
  const signal = frame.signal_types[0];
  if (signal === "metrics") return "metrics";
  if (signal === "traces") return "traces";
  return "logs";
}

function NlqTabFrame({ frame }: { frame: VisualizationFrame }) {
  return (
    <div className="space-y-3 p-4" data-testid="service-nlq-tab-result">
      <VisualizationPanel frame={frame} />
      <p className="m-0 text-xs italic text-[var(--muted)]">
        {frame.approximation_statement}
      </p>
    </div>
  );
}

function ServiceLogsTab({ serviceName }: { serviceName: string }) {
  return (
    <LogExplorer
      initialService={serviceName}
      serviceName={serviceName}
      lockedService
      showHeader={false}
      showServiceColumn={false}
      showPromote={false}
      tableAriaLabel="Service logs"
    />
  );
}

function ServiceTracesTab({ serviceName }: { serviceName: string }) {
  return (
    <TraceExplorer
      initialService={serviceName}
      serviceName={serviceName}
      lockedService
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

function ResponseTimeGraphSection({
  serviceName,
  fromMs,
  toMs,
}: {
  serviceName: string;
  fromMs: number;
  toMs: number;
}) {
  const { setCustomRange } = useGlobalDateRange();

  const { data: historyData } = useQuery({
    queryKey: ["service-response-time", serviceName, fromMs, toMs],
    queryFn: () =>
      getServiceResponseTimeHistory(serviceName, {
        from: fromMs,
        to: toMs,
        buckets: 60,
      }),
  });

  const { data: deploymentData } = useQuery({
    queryKey: ["deployments", serviceName, fromMs, toMs],
    queryFn: () =>
      listDeployments({
        service_name: serviceName,
        start_time: new Date(fromMs).toISOString(),
        end_time: new Date(toMs).toISOString(),
        limit: 20,
      }),
  });

  if (!historyData?.buckets?.length) return null;

  const p95Series: TimeSeriesSeries = {
    key: "p95",
    label: "P95",
    color: "#818cf8",
    formatY: (v) => `${Math.round(v)}ms`,
    points: historyData.buckets.map((b) => ({ timestampMs: b.start_ms, value: b.p95_ms })),
  };

  const p50Series: TimeSeriesSeries = {
    key: "p50",
    label: "P50",
    color: "#34d399",
    formatY: (v) => `${Math.round(v)}ms`,
    points: historyData.buckets.map((b) => ({ timestampMs: b.start_ms, value: b.p50_ms })),
  };

  const rateSeries: TimeSeriesSeries = {
    key: "request_rate",
    label: "Req/s",
    color: "#fb923c",
    dashed: true,
    formatY: (v) => `${v.toFixed(1)} rps`,
    points: historyData.buckets.map((b) => ({ timestampMs: b.start_ms, value: b.request_rate })),
  };

  return (
    <TimeSeriesGraph
      series={[p95Series, p50Series, rateSeries]}
      deploymentMarkers={deploymentData?.items ?? []}
      rangeStartMs={fromMs}
      rangeEndMs={toMs}
      eyebrow="Performance"
      title="Response Time & Throughput"
      ariaLabel="Service response time and throughput graph"
      onRangeSelect={setCustomRange}
    />
  );
}
