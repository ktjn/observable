import { useQuery } from "@tanstack/react-query";
import { Link, useLocation, useParams } from "@tanstack/react-router";
import type { ServiceSummary } from "../api/services";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/empty-state";
import { ErrorState } from "../components/ui/error-state";
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
import { useRuntime } from "../hooks/useRuntime";
import { liveViewQueryOptions } from "../hooks/useLiveRefresh";
import { LogExplorer } from "./LogSearch";
import { TraceExplorer } from "./TraceSearch";

export default function ServiceDetailPage() {
  const { serviceId } = useParams({ strict: false });
  const { tenantId, environment } = useTenantContext();
  const runtime = useRuntime();
  if (!serviceId) {
    return <LoadingState>Loading service overview…</LoadingState>;
  }
  const serviceName = decodeURIComponent(serviceId);
  const location = useLocation();
  const activeTab = signalTabFromPath(location.pathname);
  const { fromMs, toMs } = useGlobalDateRange();

  const { data, isLoading, isError } = useQuery({
    queryKey: ["service-summary", tenantId, serviceName, environment ?? "all", fromMs, toMs],
    queryFn: () =>
      runtime.services.summary(tenantId, serviceName, {
        environment: environment ?? undefined,
        from: fromMs,
        to: toMs,
      }),
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
  const runtime = useRuntime();

  const { data: historyData } = useQuery({
    queryKey: [
      "service-response-time",
      tenantId,
      service.service_name,
      fromMs,
      toMs,
    ],
    queryFn: () =>
      runtime.services.responseTimeHistory(tenantId, service.service_name, {
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
          tone={service.error_rate > 0 ? "warn" : "info"}
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
}: {
  serviceName: string;
  activeTab: ServiceSignalTab;
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
        <ServiceReliabilityTab serviceName={serviceName} />
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
  const { preset, compareRange, setCustomRange, clearCustomRange } = useGlobalDateRange();
  const { tenantId, environment } = useTenantContext();
  const runtime = useRuntime();
  const isPinned = preset === null;

  const {
    data: historyData,
    isError: historyError,
    error: historyErr,
    refetch,
    isFetching,
  } = useQuery({
    queryKey: ["service-response-time", tenantId, serviceName, fromMs, toMs],
    queryFn: () =>
      runtime.services.responseTimeHistory(tenantId, serviceName, {
        from: fromMs,
        to: toMs,
        buckets: 60,
      }),
    ...liveViewQueryOptions,
    retry: 1,
    refetchInterval: isPinned ? false : liveViewQueryOptions.refetchInterval,
  });

  const { data: baselineData, isError: baselineError } = useQuery({
    queryKey: [
      "service-response-time-baseline",
      tenantId,
      serviceName,
      compareRange?.fromMs ?? null,
      compareRange?.toMs ?? null,
    ],
    queryFn: () =>
      runtime.services.responseTimeHistory(tenantId, serviceName, {
        from: compareRange!.fromMs,
        to: compareRange!.toMs,
        buckets: 60,
      }),
    enabled: Boolean(compareRange),
  });

  const { data: deploymentData } = useQuery({
    queryKey: ["deployments", tenantId, serviceName, fromMs, toMs],
    queryFn: () =>
      runtime.deployments.list(tenantId, {
        service_name: serviceName,
        start_time: new Date(fromMs).toISOString(),
        end_time: new Date(toMs).toISOString(),
        limit: 20,
      }),
    ...liveViewQueryOptions,
    retry: 1,
    refetchInterval: isPinned ? false : liveViewQueryOptions.refetchInterval,
  });

  const { data: changeEventData } = useQuery({
    queryKey: ["change-events", tenantId, serviceName, fromMs, toMs],
    queryFn: () =>
      runtime.changeEvents.list(tenantId, {
        service_name: serviceName,
        start_time: new Date(fromMs).toISOString(),
        end_time: new Date(toMs).toISOString(),
        limit: 20,
      }),
    ...liveViewQueryOptions,
    retry: 1,
    refetchInterval: isPinned ? false : liveViewQueryOptions.refetchInterval,
  });

  if (historyError) {
    return (
      <ErrorState
        title="Failed to load response time history"
        description="Latency and request-rate series could not be retrieved for this period."
        error={historyErr instanceof Error ? historyErr.message : String(historyErr)}
        onRetry={() => refetch()}
      />
    );
  }

  if (!historyData?.buckets?.length) return null;

  const p95Series: TimeSeriesSeries = {
    key: "p95",
    label: "P95",
    color: "#818cf8",
    aggregation: "p95",
    unit: "ms",
    formatY: (v) => `${Math.round(v)}ms`,
    points: historyData.buckets.map((b) => ({ timestampMs: b.start_ms, value: b.p95_ms })),
  };

  const p50Series: TimeSeriesSeries = {
    key: "p50",
    label: "P50",
    color: "#34d399",
    aggregation: "p50",
    unit: "ms",
    formatY: (v) => `${Math.round(v)}ms`,
    points: historyData.buckets.map((b) => ({ timestampMs: b.start_ms, value: b.p50_ms })),
  };

  const rateSeries: TimeSeriesSeries = {
    key: "request_rate",
    label: "Req/s",
    color: "#fb923c",
    dashed: true,
    aggregation: "rate",
    unit: "rps",
    formatY: (v) => `${v.toFixed(1)} rps`,
    points: historyData.buckets.map((b) => ({ timestampMs: b.start_ms, value: b.request_rate })),
  };

  const series: TimeSeriesSeries[] = [p95Series, p50Series, rateSeries];
  if (compareRange && baselineData?.buckets?.length) {
    const offsetMs = fromMs - compareRange.fromMs;
    series.push({
      key: "p95_baseline",
      label: "P95 Baseline",
      color: "#94a3b8",
      dashed: true,
      aggregation: "p95",
      unit: "ms",
      formatY: (v) => `${Math.round(v)}ms`,
      points: baselineData.buckets.map((b) => ({
        timestampMs: b.start_ms + offsetMs,
        value: b.p95_ms,
      })),
    });
  }

  const observedBuckets = historyData.buckets.filter((b) => b.request_rate > 0).length;
  const totalBuckets = historyData.buckets.length;
  const partialCoverage = observedBuckets < totalBuckets;

  const currentSpan = toMs - fromMs;
  const baselineSpan = compareRange ? compareRange.toMs - compareRange.fromMs : currentSpan;
  const lengthMismatch = Boolean(compareRange && Math.abs(baselineSpan - currentSpan) > 1000);
  const baselineObserved = baselineData?.buckets?.filter((b) => b.request_rate > 0).length ?? 0;
  const baselineTotal = baselineData?.buckets?.length ?? 0;
  const baselineMissingData = Boolean(
    compareRange && (baselineError || baselineTotal === 0 || baselineObserved < baselineTotal),
  );

  return (
    <div className="grid gap-1.5">
      <div
        data-testid="service-histogram-disclosure"
        className="flex flex-wrap items-center justify-between gap-2 text-xs text-[var(--muted)]"
      >
        <span>
          Evaluated period: <strong className="text-[var(--text)]">{new Date(fromMs).toISOString().slice(0, 16).replace("T", " ")}</strong>
          {" – "}
          <strong className="text-[var(--text)]">{new Date(toMs).toISOString().slice(0, 16).replace("T", " ")} UTC</strong>
          {" · "}
          Environment: <strong className="text-[var(--text)]">{environment ?? "All environments"}</strong>
        </span>
        <div className="flex items-center gap-2">
          <span className={partialCoverage ? "font-semibold text-[var(--warn)]" : undefined}>
            {partialCoverage
              ? `Partial coverage: ${observedBuckets} of ${totalBuckets} intervals contain observed traffic`
              : `Full interval coverage (${totalBuckets} intervals)`}
          </span>
          {isPinned && (
            <span role="status" className="inline-flex items-center gap-2">
              <span>Inspection pinned</span>
              <Button variant="secondary" onClick={() => void refetch()} disabled={isFetching}>
                {isFetching ? "Refreshing…" : "Refresh now"}
              </Button>
            </span>
          )}
        </div>
      </div>
      {compareRange && (
        <div
          data-testid="baseline-comparison-banner"
          className="flex flex-wrap items-center justify-between gap-2 rounded border border-[var(--border)] bg-[var(--surface-1)] px-3 py-1.5 text-xs text-[var(--muted)]"
        >
          <span>
            Baseline ({compareRange.mode}):{" "}
            <strong className="text-[var(--text)]">
              {new Date(compareRange.fromMs).toISOString().slice(0, 16).replace("T", " ")}–
              {new Date(compareRange.toMs).toISOString().slice(0, 16).replace("T", " ")} UTC
            </strong>
          </span>
          <div className="flex flex-wrap items-center gap-2">
            {lengthMismatch && (
              <span className="font-semibold text-[var(--warn)]">
                Period length differs ({Math.round(currentSpan / 60_000)}m vs {Math.round(baselineSpan / 60_000)}m)
              </span>
            )}
            {baselineMissingData && (
              <span className="font-semibold text-[var(--warn)]">
                {baselineError
                  ? "Baseline data unavailable"
                  : baselineTotal === 0
                    ? "Baseline has missing data (no intervals)"
                    : `Baseline has missing data (${baselineObserved} of ${baselineTotal} intervals observed)`}
              </span>
            )}
          </div>
        </div>
      )}
      <TimeSeriesGraph
        series={series}
        deploymentMarkers={deploymentData?.items ?? []}
        changeEvents={changeEventData?.items ?? []}
        rangeStartMs={fromMs}
        rangeEndMs={toMs}
        eyebrow="Performance"
        title="Response Time & Throughput"
        ariaLabel="Service response time and throughput graph"
        onRangeSelect={setCustomRange}
        isZoomed={isPinned}
        onResetZoom={clearCustomRange}
      />
    </div>
  );
}
