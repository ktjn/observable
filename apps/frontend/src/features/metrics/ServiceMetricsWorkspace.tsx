import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { getMetricPoints, listMetrics, type MetricSeries } from "../../api/metrics";
import { createDashboard } from "../../api/dashboards";
import { Button } from "../../components/ui/button";
import { EmptyState } from "../../components/ui/empty-state";
import { LoadingState } from "../../components/ui/loading-state";
import { MetricCard } from "../../components/ui/metric-card";
import { Panel } from "../../components/ui/panel";
import { TablePanel } from "../../components/ui/table-panel";
import { TimeSeriesGraph, type TimeSeriesSeries } from "../../components/ui/time-series-graph";
import { SignalExplorer, type SaveStatus } from "../../components/shared/SignalExplorer";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
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
  const [serviceName, setServiceName] = useState(initialService);
  const [filters, setFilters] = useState<FilterState>({
    name: "",
    type: "all",
    environment: "all",
  });
  const [selectedSeriesId, setSelectedSeriesId] = useState<string | null>(null);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");

  const { data, isLoading, error } = useQuery({
    queryKey: ["service", serviceName, "metrics"],
    queryFn: () => listMetrics({ service: serviceName || undefined }),
    enabled: true, // We want to allow listing all metrics if serviceName is empty
  });

  const series = data?.series ?? [];
  const filteredSeries = useMemo(
    () => filterSeries(series, filters),
    [series, filters],
  );

  const metricTypes = useMemo(() => uniqueValues(series.map((item) => item.metric_type)), [series]);
  const environments = useMemo(
    () => uniqueValues(series.map((item) => item.environment || "default")),
    [series],
  );

  const selectedSeries = useMemo(
    () => series.find((s) => s.metric_series_id === selectedSeriesId) ?? null,
    [series, selectedSeriesId],
  );

  const handlePromote = async () => {
    setSaveStatus("saving");
    try {
      await createDashboard({
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
  if (error) return <div className="signal-empty">Metrics could not be loaded.</div>;

  return (
    <div className="space-y-4">
      {showHeader && (
        <div className="page-header">
          <div>
            <div className="text-xs font-bold uppercase text-[var(--muted)]">Explorer</div>
            <h1>Metrics</h1>
          </div>
        </div>
      )}
      {series.length > 0 && (
        <div
          className="grid grid-cols-[repeat(4,minmax(140px,1fr))] gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
          aria-label="Service metrics summary"
        >
          <MetricCard label="Metric Series" value={`${series.length} ${plural(series.length, "series", "series")}`} tone="info" />
          <MetricCard label="Metric Types" value={`${metricTypes.length} ${plural(metricTypes.length, "type")}`} tone="good" />
          <MetricCard label="Environments" value={`${environments.length} ${plural(environments.length, "env")}`} tone="info" />
          <MetricCard label="Filtered" value={`${filteredSeries.length} ${plural(filteredSeries.length, "series", "series")}`} tone="info" />
        </div>
      )}

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
        selectedId={selectedSeriesId}
        onSelect={setSelectedSeriesId}
        histogram={
          <MetricGraphContainer
            selectedSeries={selectedSeries}
            fromMs={fromMs}
            toMs={toMs}
            onRangeSelect={setCustomRange}
          />
        }
        renderTable={(selectedId, onSelect) => (
          <div className="flex flex-col gap-4 w-full">
            <Panel eyebrow="Browse" title="Metric Series">
              <div className="mb-4">
                <QueryFilterInput
                  baseIr={METRICS_BASE_IR}
                  serviceName={serviceName}
                  placeholder='Filter metric series, e.g. "histogram latency metrics in prod" or raw NLQ IR JSON'
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
                    setSelectedSeriesId(null);
                  }}
                />

              </div>
              {filteredSeries.length > 0 ? (
                <MetricSeriesTable
                  series={filteredSeries}
                  selectedSeriesId={selectedId}
                  onSelect={onSelect}
                />
              ) : (
                <EmptyState 
                  title="No metrics found" 
                  description={serviceName ? `No metrics for service ${serviceName} match filters.` : "No global metrics found matching filters."}
                />
              )}
            </Panel>
          </div>
        )}
        renderPanel={(_selectedId, onClose) => (
          selectedSeries ? (
            <MetricDetailSidebar
              series={selectedSeries}
              onClose={onClose}
            />
          ) : null
        )}
      />
    </div>
  );
}

function MetricGraphContainer({
  selectedSeries,
  fromMs,
  toMs,
  onRangeSelect,
}: {
  selectedSeries: MetricSeries | null;
  fromMs: number;
  toMs: number;
  onRangeSelect: (fromMs: number, toMs: number) => void;
}) {
  const { data, isLoading } = useQuery({
    queryKey: ["metric-points", selectedSeries?.metric_series_id, fromMs, toMs],
    queryFn: () => getMetricPoints(selectedSeries!.metric_series_id),
    enabled: Boolean(selectedSeries),
  });

  if (!selectedSeries) {
    return (
      <div className="signal-empty border border-[var(--border)] bg-[var(--surface)] h-[168px] flex items-center justify-center text-sm text-[var(--muted)]">
        Select a metric series below to visualize data.
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className="border border-[var(--border)] bg-[var(--surface)] h-[168px] animate-pulse" />
    );
  }

  const points = data?.points ?? [];
  const seriesData: TimeSeriesSeries[] = [
    {
      key: selectedSeries.metric_series_id,
      label: selectedSeries.metric_name,
      color: "var(--brand)",
      points: points.map((p) => ({
        timestampMs: Number(p.time_unix_nano) / 1_000_000,
        value: p.value_double ?? p.value_int ?? 0,
      })),
      formatY: (v) => `${v}${selectedSeries.unit ? ` ${selectedSeries.unit}` : ""}`,
    },
  ];

  return (
    <TimeSeriesGraph
      series={seriesData}
      rangeStartMs={fromMs}
      rangeEndMs={toMs}
      height={140}
      onRangeSelect={onRangeSelect}
      ariaLabel={`Graph for ${selectedSeries.metric_name}`}
    />
  );
}

function MetricSeriesTable({
  series,
  selectedSeriesId,
  onSelect,
}: {
  series: MetricSeries[];
  selectedSeriesId: string | null;
  onSelect: (seriesId: string | null) => void;
}) {
  return (
    <TablePanel>
      <table aria-label="Service metrics">
        <thead>
          <tr>
            <th>Metric</th>
            <th>Type</th>
            <th>Unit</th>
            <th>Environment</th>
            <th>Labels</th>
          </tr>
        </thead>
        <tbody>
          {series.map((item) => (
            <tr
              key={item.metric_series_id}
              role="button"
              aria-label={`Select ${item.metric_name}`}
              className={selectedSeriesId === item.metric_series_id ? "selected" : "hoverable"}
              onClick={() => onSelect(item.metric_series_id)}
              style={{ cursor: "pointer" }}
            >
              <td>
                <div className="font-semibold text-[var(--text-strong)]">{item.metric_name}</div>
                <div className="text-xs text-[var(--muted)]">{shortId(item.metric_series_id)}</div>
              </td>
              <td>{item.metric_type}</td>
              <td>{item.unit || "none"}</td>
              <td>{item.environment || "default"}</td>
              <td>{labelSummary(item)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </TablePanel>
  );
}

function MetricDetailSidebar({
  series,
  onClose,
}: {
  series: MetricSeries;
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
          <h2 className="m-0 text-base font-bold text-[var(--text-strong)]">{series.metric_name}</h2>
        </div>
        <Button variant="secondary" className="min-h-8 px-2 text-xs" onClick={onClose}>
          Close
        </Button>
      </div>

      <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">series_id</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{series.metric_series_id}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">type</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{series.metric_type}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">unit</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{series.unit || "none"}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">service</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{series.service_name}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">environment</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{series.environment || "default"}</dd>
        </div>
      </dl>

      <div className="mt-4">
        <div className="text-[10px] font-bold uppercase text-[var(--muted)] mb-2">Attributes</div>
        <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-1 text-xs">
          {Object.entries({ ...series.attributes, ...series.resource_attributes })
            .sort(([a], [b]) => a.localeCompare(b))
            .map(([key, value]) => (
              <div key={key} className="contents">
                <dt className="break-all font-medium text-[var(--muted)]">{key}</dt>
                <dd className="m-0 min-w-0 break-all text-[var(--text)]">{String(value)}</dd>
              </div>
            ))}
        </dl>
      </div>
    </aside>
  );
}

function filterSeries(series: MetricSeries[], filters: FilterState): MetricSeries[] {
  const name = filters.name.trim().toLowerCase();
  return series.filter((item) => {
    const environment = item.environment || "default";
    const matchesName = !name || item.metric_name.toLowerCase().includes(name);
    const matchesType = filters.type === "all" || item.metric_type === filters.type;
    const matchesEnvironment = filters.environment === "all" || environment === filters.environment;
    return matchesName && matchesType && matchesEnvironment;
  });
}

function uniqueValues(values: string[]): string[] {
  return Array.from(new Set(values.filter(Boolean))).sort((a, b) => a.localeCompare(b));
}

function labelSummary(series: MetricSeries): string {
  const attributes = Object.keys(series.attributes ?? {});
  const resourceAttributes = Object.keys(series.resource_attributes ?? {});
  const labels = [...attributes, ...resourceAttributes].slice(0, 3);
  if (!labels.length) return "none";
  const suffix = attributes.length + resourceAttributes.length > labels.length ? " +" : "";
  return `${labels.join(", ")}${suffix}`;
}

function shortId(value: string): string {
  return value.slice(0, 8);
}

function plural(count: number, singular: string, pluralValue = `${singular}s`): string {
  return count === 1 ? singular : pluralValue;
}
