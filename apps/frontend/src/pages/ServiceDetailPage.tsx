import { useQuery } from "@tanstack/react-query";
import { Link, useLocation, useParams } from "@tanstack/react-router";
import { listChangeEvents } from "../api/changeEvents";
import { listDeployments } from "../api/deployments";
import {
  getServiceResponseTimeHistory,
  getServiceSummary,
  ServiceSummary,
} from "../api/services";
import { EmptyState } from "../components/ui/empty-state";
import { LoadingState } from "../components/ui/loading-state";
import { MetricCard } from "../components/ui/metric-card";
import { Panel } from "../components/ui/panel";
import {
  TimeSeriesGraph,
  TimeSeriesSeries,
} from "../components/ui/time-series-graph";
import { ServiceMetricsWorkspace } from "../features/metrics/ServiceMetricsWorkspace";
import { ServiceInfraPanel } from "../components/ServiceInfraPanel";
import { ServiceDeploymentsTab } from "../features/services/ServiceDeploymentsTab";
import { ServiceAlertsTab } from "../features/services/ServiceAlertsTab";
import { ServiceReliabilityTab } from "../features/services/ServiceReliabilityTab";
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";
import { useTenantContext } from "../hooks/useTenantContext";
import { liveViewQueryOptions } from "../hooks/useLiveRefresh";
import { LogExplorer } from "./LogSearch";
import { TraceExplorer } from "./TraceSearch";

export default function ServiceDetailPage() {
  const { serviceId } = useParams({ strict: false });
  const { tenantId } = useTenantContext();
  if (!serviceId) {
    return <LoadingState>Loading service overview…</LoadingState>;
  }
  const serviceName = decodeURIComponent(serviceId);
  const location = useLocation();
  const activeTab = signalTabFromPath(location.pathname);
  const { fromMs, toMs } = useGlobalDateRange();

  const { data, isLoading, isError } = useQuery({
    queryKey: ["service-summary", tenantId, serviceName, fromMs, toMs],
    queryFn: () => getServiceSummary(tenantId, serviceName, { from: fromMs, to: toMs }),
    ...liveViewQueryOptions,
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

function computeDelta<T>(
  buckets: T[] | undefined,
  getValue: (b: T) => number,
): number | undefined {
  if (!buckets || buckets.length < 4) return undefined;
  const half = Math.floor(buckets.length / 2);
  const prevAvg =
    buckets.slice(0, half).reduce((s, b) => s + getValue(b), 0) / half;
  const currAvg =
    buckets.slice(half).reduce((s, b) => s + getValue(b), 0) /
    (buckets.length - half);
  if (prevAvg === 0) return undefined;
  return (currAvg - prevAvg) / prevAvg;
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
  const { tenantId } = useTenantContext();

  const { data: historyData } = useQuery({
    queryKey: [
      "service-response-time",
      tenantId,
      service.service_name,
      fromMs,
      toMs,
    ],
    queryFn: () =>
      getServiceResponseTimeHistory(tenantId, service.service_name, {
        from: fromMs,
        to: toMs,
        buckets: 60,
      }),
    ...liveViewQueryOptions,
  });

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Service Overview</div>
          <h1>{service.service_name}</h1>
        </div>
        <div className="flex items-center gap-3">
          <Link to="/workbench" className="secondary-link">Ask in Workbench →</Link>
          <Link to="/services" className="secondary-link">Back to services</Link>
        </div>
      </div>

      <div
        className="grid grid-cols-[repeat(4,minmax(140px,1fr))] gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
        aria-label="Service performance summary"
      >
        <MetricCard
          label="Request Rate"
          value={`${service.request_rate.toFixed(2)} rps`}
          tone="info"
          sparkline={historyData?.buckets?.map((b) => b.request_rate)}
          delta={computeDelta(historyData?.buckets, (b) => b.request_rate)}
          deltaPositiveTone="good"
        />
        <MetricCard
          label="Error Rate"
          value={`${(service.error_rate * 100).toFixed(2)}%`}
          tone={service.health_state === "breach" ? "bad" : service.health_state === "watch" ? "warn" : "good"}
        />
        <MetricCard
          label="P95 Latency"
          value={`${Math.round(service.p95_latency_ms)}ms`}
          tone={service.p95_latency_ms >= 500 ? "bad" : service.p95_latency_ms >= 100 ? "warn" : "good"}
          sparkline={historyData?.buckets?.map((b) => b.p95_ms)}
          delta={computeDelta(historyData?.buckets, (b) => b.p95_ms)}
          deltaPositiveTone="bad"
        />
        <MetricCard label="Active Alerts" value={String(service.active_alert_count)} tone={service.active_alert_count > 0 ? "warn" : "good"} />
      </div>

      <ResponseTimeGraphSection
        serviceName={service.service_name}
        fromMs={fromMs}
        toMs={toMs}
      />

      <ServiceSignalTabs
        serviceName={service.service_name}
        activeTab={activeTab}
        healthState={service.health_state}
      />
    </section>
  );
}

type ServiceSignalTab = "reliability" | "logs" | "metrics" | "traces" | "infrastructure" | "deployments" | "alerts";

function signalTabFromPath(pathname: string): ServiceSignalTab {
  if (pathname.endsWith("/reliability")) return "reliability";
  if (pathname.endsWith("/metrics")) return "metrics";
  if (pathname.endsWith("/traces")) return "traces";
  if (pathname.endsWith("/infrastructure")) return "infrastructure";
  if (pathname.endsWith("/deployments")) return "deployments";
  if (pathname.endsWith("/alerts")) return "alerts";
  return "logs";
}

function ServiceSignalTabs({
  serviceName,
  activeTab,
  healthState,
}: {
  serviceName: string;
  activeTab: ServiceSignalTab;
  healthState: ServiceSummary["health_state"];
}) {
  const encodedService = encodeURIComponent(serviceName);
  const tabLinks = [
    { tab: "reliability" as const, label: "Reliability", to: "/services/$serviceId/reliability" },
    { tab: "logs" as const,         label: "Logs",        to: "/services/$serviceId/logs" },
    { tab: "metrics" as const,      label: "Metrics",     to: "/services/$serviceId/metrics" },
    { tab: "traces" as const,       label: "Traces",      to: "/services/$serviceId/traces" },
    { tab: "infrastructure" as const, label: "Infrastructure", to: "/services/$serviceId/infrastructure" },
    { tab: "deployments" as const,  label: "Deployments", to: "/services/$serviceId/deployments" },
    { tab: "alerts" as const,       label: "Alerts",      to: "/services/$serviceId/alerts" },
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
      {activeTab === "logs" && <ServiceLogsTab serviceName={serviceName} />}
      {activeTab === "metrics" && <ServiceMetricsWorkspace initialService={serviceName} />}
      {activeTab === "traces" && <ServiceTracesTab serviceName={serviceName} />}
      {activeTab === "infrastructure" && <ServiceInfraPanel serviceName={serviceName} />}
      {activeTab === "deployments" && <ServiceDeploymentsTab serviceName={serviceName} />}
      {activeTab === "reliability" && (
        <ServiceReliabilityTab serviceName={serviceName} healthState={healthState} />
      )}
      {activeTab === "alerts" && <ServiceAlertsTab />}
    </Panel>
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

function ResponseTimeGraphSection({
  serviceName,
  fromMs,
  toMs,
}: {
  serviceName: string;
  fromMs: number;
  toMs: number;
}) {
  const { preset, setCustomRange, clearCustomRange } = useGlobalDateRange();
  const { tenantId } = useTenantContext();

  const { data: historyData } = useQuery({
    queryKey: ["service-response-time", tenantId, serviceName, fromMs, toMs],
    queryFn: () =>
      getServiceResponseTimeHistory(tenantId, serviceName, {
        from: fromMs,
        to: toMs,
        buckets: 60,
      }),
    ...liveViewQueryOptions,
  });

  const { data: deploymentData } = useQuery({
    queryKey: ["deployments", tenantId, serviceName, fromMs, toMs],
    queryFn: () =>
      listDeployments(tenantId, {
        service_name: serviceName,
        start_time: new Date(fromMs).toISOString(),
        end_time: new Date(toMs).toISOString(),
        limit: 20,
      }),
    ...liveViewQueryOptions,
  });

  const { data: changeEventData } = useQuery({
    queryKey: ["change-events", tenantId, serviceName, fromMs, toMs],
    queryFn: () =>
      listChangeEvents(tenantId, {
        service_name: serviceName,
        start_time: new Date(fromMs).toISOString(),
        end_time: new Date(toMs).toISOString(),
        limit: 20,
      }),
    ...liveViewQueryOptions,
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
      changeEvents={changeEventData?.items ?? []}
      rangeStartMs={fromMs}
      rangeEndMs={toMs}
      eyebrow="Performance"
      title="Response Time & Throughput"
      ariaLabel="Service response time and throughput graph"
      onRangeSelect={setCustomRange}
      isZoomed={preset === null}
      onResetZoom={clearCustomRange}
    />
  );
}
