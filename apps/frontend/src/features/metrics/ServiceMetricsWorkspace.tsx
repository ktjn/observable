import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { getMetricGroupPoints, listMetrics, type MetricCatalogEntry } from "../../api/metrics";
import { createDashboard } from "../../api/dashboards";
import { Button } from "../../components/ui/button";
import { EmptyState } from "../../components/ui/empty-state";
import { ErrorState } from "../../components/ui/error-state";
import { LoadingState } from "../../components/ui/loading-state";
import { MetricCard } from "../../components/ui/metric-card";
import { PillFilter } from "../../components/ui/pill-filter";
import { TablePanel } from "../../components/ui/table-panel";
import { TimeSeriesGraph, type TimeSeriesSeries } from "../../components/ui/time-series-graph";
import { SignalExplorer, type SaveStatus } from "../../components/shared/SignalExplorer";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
import { useTenantContext } from "../../hooks/useTenantContext";
import { liveViewQueryOptions } from "../../hooks/useLiveRefresh";
import { QueryFilterInput } from "../nlq/QueryFilterInput";
import { deriveViewFiltersFromIr, type NlqIrLike } from "../nlq/queryFilters";

const METRICS_BASE_IR: NlqIrLike = {
  operation: "catalog",
  signals: ["metrics"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};

type FilterState = {
  name: string;
  type: string;
  environment: string;
};

export function ServiceMetricsWorkspace({ 
  initialService = "",
  lockedService = false,
  showHeader = true,
}: { 
  initialService?: string;
  lockedService?: boolean;
  showHeader?: boolean;
}) {
  const { fromMs, toMs, setCustomRange } = useGlobalDateRange();
  const { tenantId } = useTenantContext();
  const [serviceName, setServiceName] = useState(initialService);
  const [filters, setFilters] = useState<FilterState>({
    name: "",
    type: "all",
    environment: "all",
  });
  const [selectedMetricId, setSelectedMetricId] = useState<string | null>(null);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");

  const { data, isLoading, error } = useQuery({
    queryKey: ["service", tenantId, serviceName, "metrics"],
    queryFn: () => listMetrics(tenantId, { service: serviceName || undefined }),
    enabled: true, // We want to allow listing all metrics if serviceName is empty
    ...liveViewQueryOptions,
  });

  const metrics = data?.metrics ?? [];

  // Apply name and environment filters first, then compute type counts,
  // then apply type filter. This follows the design-guide §12.1 count
  // semantics: type pills show counts that respect all other active filters.
  const baseMetrics = useMemo(() => {
    const name = filters.name.trim().toLowerCase();
    return metrics.filter((item) => {
      const environment = item.environment || "default";
      const matchesName = !name || item.metric_name.toLowerCase().includes(name);
      const matchesEnvironment = filters.environment === "all" || environment === filters.environment;
      return matchesName && matchesEnvironment;
    });
  }, [metrics, filters.name, filters.environment]);

  const typeCounts = useMemo(() => {
    const counts: Record<string, number> = { all: baseMetrics.length };
    for (const m of baseMetrics) {
      counts[m.metric_type] = (counts[m.metric_type] || 0) + 1;
    }
    return counts;
  }, [baseMetrics]);

  const filteredMetrics = useMemo(
    () => (filters.type === "all" ? baseMetrics : baseMetrics.filter((m) => m.metric_type === filters.type)),
    [baseMetrics, filters.type],
  );

  const metricTypes = useMemo(() => uniqueValues(metrics.map((item) => item.metric_type)), [metrics]);
  const environments = useMemo(
    () => uniqueValues(metrics.map((item) => item.environment || "default")),
    [metrics],
  );
  const backingSeriesCount = useMemo(
    () => metrics.reduce((total, item) => total + item.series_count, 0),
    [metrics],
  );

  const selectedMetric = useMemo(
    () => metrics.find((metric) => metricIdentity(metric) === selectedMetricId) ?? null,
    [metrics, selectedMetricId],
  );

  const handlePromote = async () => {
    setSaveStatus("saving");
    try {
      await createDashboard(tenantId, {
        name: serviceName ? `Metrics for ${serviceName}` : "Global Metrics",
        panels: [
          {
            title: serviceName ? `Metrics for ${serviceName}` : "Global Metrics",
            query_kind: "metrics",
            service: serviceName || undefined,
            preset: null,
            filters: { ...filters },
          },
        ],
      });
      setSaveStatus("saved");
    } catch {
      setSaveStatus("error");
    }
  };

  if (isLoading) return <LoadingState>Loading service metrics...</LoadingState>;
  if (error) return <ErrorState title="Failed to load metrics" description={String(error)} />;

  return (
    <div className="flex flex-col h-full min-h-0 gap-4">
      {showHeader && (
        <div className="shrink-0 page-header">
          <div>
            <div className="text-xs font-bold uppercase text-[var(--muted)]">Explorer</div>
            <h1>Metrics</h1>
          </div>
        </div>
      )}
      {metrics.length > 0 && (
        <div
          className="shrink-0 grid grid-cols-[repeat(4,minmax(140px,1fr))] gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
          aria-label="Service metrics summary"
        >
          <MetricCard label="Metrics" value={`${metrics.length} ${plural(metrics.length, "metric")}`} tone="info" />
          <MetricCard label="Backing Series" value={`${backingSeriesCount} ${plural(backingSeriesCount, "series", "series")}`} tone="info" />
          <MetricCard label="Metric Types" value={`${metricTypes.length} ${plural(metricTypes.length, "type")}`} tone="good" />
          <MetricCard label="Environments" value={`${environments.length} ${plural(environments.length, "env")}`} tone="info" />
        </div>
      )}

      <div className="flex-1 min-h-0">
        <SignalExplorer
        title="Metrics"
        service={serviceName}
        onServiceChange={setServiceName}
        lockedService={lockedService}
        showHeader={false}
        showPromote={true}
        querySurface="metrics"
        saveStatus={saveStatus}
        onPromote={handlePromote}
        selectedId={selectedMetricId}
        onSelect={setSelectedMetricId}
        histogram={
          <MetricGraphContainer
            selectedMetric={selectedMetric}
            fromMs={fromMs}
            toMs={toMs}
            onRangeSelect={setCustomRange}
          />
        }
        renderTable={(selectedId, onSelect) => (
          <div className="flex flex-col flex-1 min-h-0 gap-4">
            <div>
              <QueryFilterInput
                baseIr={METRICS_BASE_IR}
                serviceName={serviceName}
                placeholder='Filter metric series, e.g. "histogram latency metrics in prod"'
                onSubmit={(_rawText) => {
                  // Need to derive IR and apply filters based on rawText here?
                  // Actually, the previous implementation used `onIr`.
                  // Let's keep `onIr` and add `baseIr` as required.
                }}

                onIr={(ir) => {
                  const next = deriveViewFiltersFromIr(ir, "metrics");
                  setFilters({
                    name: next.metricName ?? "",
                    type: next.metricType ?? "all",
                    environment: next.environment ?? "all",
                  });
                  if (next.service && !lockedService) {
                    setServiceName(next.service);
                  }
                  setSelectedMetricId(null);
                }}
              />
            </div>

            {/* Type filter pills + name search */}
            <div className="flex flex-wrap items-center gap-2">
              <PillFilter
                pills={["all", ...metricTypes].map((type) => ({
                  key: type,
                  label: type === "all" ? "All types" : type,
                  count: typeCounts[type] ?? 0,
                }))}
                activeKey={filters.type}
                onSelect={(key) => setFilters((f) => ({ ...f, type: key }))}
                ariaLabel="Filter by metric type"
              />
              <input
                type="search"
                value={filters.name}
                onChange={(e) => setFilters((f) => ({ ...f, name: e.target.value }))}
                placeholder="Search metric names…"
                aria-label="Search metric names"
                className="min-w-[180px] flex-1 border border-[var(--border)] bg-[var(--surface)] px-2.5 py-1 text-xs text-[var(--text)] placeholder:text-[var(--muted)] focus:border-[var(--brand)] focus:outline-none"
              />
            </div>

            <MetricCatalogTable
              metrics={filteredMetrics}
              selectedSeriesId={selectedId}
              onSelect={onSelect}
              filters={filters}
              serviceName={serviceName}
            />
          </div>
        )}
        renderPanel={(_selectedId, onClose) => (
          selectedMetric ? (
            <MetricDetailSidebar
              metric={selectedMetric}
              onClose={onClose}
            />
          ) : null
        )}
      />
    </div>
    </div>
  );
}

function MetricGraphContainer({
  selectedMetric,
  fromMs,
  toMs,
  onRangeSelect,
}: {
  selectedMetric: MetricCatalogEntry | null;
  fromMs: number;
  toMs: number;
  onRangeSelect: (fromMs: number, toMs: number) => void;
}) {
  const { tenantId } = useTenantContext();
  const { data, isLoading } = useQuery({
    queryKey: ["metric-group-points", tenantId, selectedMetric ? metricIdentity(selectedMetric) : null, fromMs, toMs],
    queryFn: () => getMetricGroupPoints(tenantId, selectedMetric!),
    enabled: Boolean(selectedMetric),
    ...liveViewQueryOptions,
  });

  const content = !selectedMetric ? (
    <EmptyState
      compact
      className="h-full"
      title="No metric selected"
      description="Select a metric below to visualize data."
    />
  ) : isLoading ? (
    <LoadingState variant="skeleton" className="h-full" />
  ) : (() => {
    const points = data?.points ?? [];
    const seriesData: TimeSeriesSeries[] = [
      {
        key: metricIdentity(selectedMetric),
        label: selectedMetric.metric_name,
        color: "var(--brand)",
        points: points.map((p) => ({
          timestampMs: Number(p.time_unix_nano) / 1_000_000,
          value: p.value_double ?? p.value_int ?? 0,
        })),
        formatY: (v) => `${v}${selectedMetric.unit ? ` ${selectedMetric.unit}` : ""}`,
      },
    ];

    return (
      <TimeSeriesGraph
        series={seriesData}
        rangeStartMs={fromMs}
        rangeEndMs={toMs}
        height={140}
        onRangeSelect={onRangeSelect}
        ariaLabel={`Graph for ${selectedMetric.metric_name}`}
      />
    );
  })();

  return <div className="h-[192px]">{content}</div>;
}

function MetricCatalogTable({
  metrics,
  selectedSeriesId,
  onSelect,
  filters,
  serviceName,
}: {
  metrics: MetricCatalogEntry[];
  selectedSeriesId: string | null;
  onSelect: (seriesId: string | null) => void;
  filters: FilterState;
  serviceName: string;
}) {
  return (
    <TablePanel className="flex-1 min-h-0 flex flex-col">
        <div className="flex-1 overflow-y-auto min-h-0">
        {metrics.length > 0 ? (
          <table aria-label="Service metrics">
            <thead className="sticky top-0 z-10 bg-[var(--surface)]">
              <tr>
                <th>Metric</th>
                <th>Type</th>
                <th>Unit</th>
                <th>Environment</th>
                <th>Series</th>
              </tr>
            </thead>
            <tbody>
              {metrics.map((item) => {
                const id = metricIdentity(item);
                return (
                <tr
                  key={id}
                  role="button"
                  aria-label={`Select ${item.metric_name}`}
                  className={selectedSeriesId === id ? "selected" : "hoverable"}
                  onClick={() => onSelect(id)}
                  style={{ cursor: "pointer" }}
                >
                  <td>
                    <div className="font-semibold text-[var(--text-strong)]">{item.metric_name}</div>
                    <div className="text-xs text-[var(--muted)]">{item.service_name}</div>
                  </td>
                  <td>{item.metric_type}</td>
                  <td>{item.unit || "none"}</td>
                  <td>{item.environment || "default"}</td>
                  <td>{`${item.series_count} ${plural(item.series_count, "series", "series")}`}</td>
                </tr>
                );
              })}
            </tbody>
          </table>
        ) : (
          <EmptyState
            title="No metrics found"
            description={
              filters.name || filters.type !== "all" || filters.environment !== "all"
                ? "No metrics match the current filters. Try clearing the search or selecting a different type."
                : serviceName
                  ? `No metrics for service ${serviceName}.`
                  : "No metrics available."
            }
          />
        )}
      </div>
    </TablePanel>
  );
}

function MetricDetailSidebar({
  metric,
  onClose,
}: {
  metric: MetricCatalogEntry;
  onClose: () => void;
}) {
  return (
    <aside
      aria-label="Selected metric details"
      className="w-full border border-[var(--border)] bg-[var(--surface)] p-4"
    >
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Selected Metric</div>
          <h2 className="m-0 text-base font-bold text-[var(--text-strong)]">{metric.metric_name}</h2>
        </div>
        <Button variant="secondary" className="min-h-8 px-2 text-xs" onClick={onClose}>
          Close
        </Button>
      </div>

      <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">series_count</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{metric.series_count}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">type</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{metric.metric_type}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">unit</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{metric.unit || "none"}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">service</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{metric.service_name}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">environment</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{metric.environment || "default"}</dd>
        </div>
      </dl>
    </aside>
  );
}

function uniqueValues(values: string[]): string[] {
  return Array.from(new Set(values.filter(Boolean))).sort((a, b) => a.localeCompare(b));
}

function metricIdentity(metric: MetricCatalogEntry): string {
  return [
    metric.metric_name,
    metric.metric_type,
    metric.unit || "",
    metric.service_name,
    metric.environment || "default",
  ].join("|");
}

function plural(count: number, singular: string, pluralValue = `${singular}s`): string {
  return count === 1 ? singular : pluralValue;
}
