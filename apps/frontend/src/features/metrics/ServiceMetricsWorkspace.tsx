import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { getMetricPoints, listMetrics, type MetricPoint, type MetricSeries } from "../../api/metrics";
import { Button } from "../../components/ui/button";
import { EmptyState } from "../../components/ui/empty-state";
import { LoadingState } from "../../components/ui/loading-state";
import { MetricCard } from "../../components/ui/metric-card";
import { Panel } from "../../components/ui/panel";
import { TablePanel } from "../../components/ui/table-panel";
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

export function ServiceMetricsWorkspace({ serviceName }: { serviceName: string }) {
  const [filters, setFilters] = useState<FilterState>({
    name: "",
    type: "all",
    environment: "all",
  });
  const [selectedSeriesId, setSelectedSeriesId] = useState<string | null>(null);

  const { data, isLoading, error } = useQuery({
    queryKey: ["service", serviceName, "metrics"],
    queryFn: () => listMetrics({ service: serviceName }),
  });

  const series = data?.series ?? [];
  const selectedSeries = series.find((item) => item.metric_series_id === selectedSeriesId) ?? null;
  const filteredSeries = useMemo(
    () => filterSeries(series, filters),
    [series, filters],
  );
  const metricTypes = useMemo(() => uniqueValues(series.map((item) => item.metric_type)), [series]);
  const environments = useMemo(
    () => uniqueValues(series.map((item) => item.environment || "default")),
    [series],
  );

  if (isLoading) return <LoadingState>Loading service metrics...</LoadingState>;
  if (error) return <div className="signal-empty">Metrics could not be loaded.</div>;
  if (!series.length) {
    return (
      <EmptyState
        title="No service metrics"
        description={`No metric series found for ${serviceName}.`}
      />
    );
  }

  return (
    <div className="space-y-4">
      <div
        className="grid grid-cols-[repeat(4,minmax(140px,1fr))] gap-3 max-[860px]:grid-cols-2 max-[560px]:grid-cols-1"
        aria-label="Service metrics summary"
      >
        <MetricCard label="Metric Series" value={`${series.length} ${plural(series.length, "series", "series")}`} tone="info" />
        <MetricCard label="Metric Types" value={`${metricTypes.length} ${plural(metricTypes.length, "type")}`} tone="good" />
        <MetricCard label="Environments" value={`${environments.length} ${plural(environments.length, "env")}`} tone="info" />
        <MetricPointSummaryCard seriesId={selectedSeriesId} />
      </div>

      <Panel eyebrow="Browse" title="Metric Series">
        <QueryFilterInput
          baseIr={METRICS_BASE_IR}
          serviceName={serviceName}
          placeholder='Filter metric series, e.g. "histogram latency metrics in prod" or raw NLQ IR JSON'
          onIr={(ir) => {
            const next = deriveViewFiltersFromIr(ir, "metrics");
            setFilters({
              name: next.metricName ?? "",
              type: next.metricType ?? "all",
              environment: next.environment ?? "all",
            });
            setSelectedSeriesId(null);
          }}
        />

        <div className="mt-4">
          {filteredSeries.length ? (
            <MetricSeriesTable
              series={filteredSeries}
              selectedSeriesId={selectedSeriesId}
              onSelect={setSelectedSeriesId}
            />
          ) : (
            <div className="signal-empty">No metric series matched the current filters.</div>
          )}
        </div>
      </Panel>

      <MetricPointPreview selectedSeries={selectedSeries} />
    </div>
  );
}

function MetricSeriesTable({
  series,
  selectedSeriesId,
  onSelect,
}: {
  series: MetricSeries[];
  selectedSeriesId: string | null;
  onSelect: (seriesId: string) => void;
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
            <th>Action</th>
          </tr>
        </thead>
        <tbody>
          {series.map((item) => (
            <tr key={item.metric_series_id}>
              <td>
                <div className="font-semibold text-[var(--text-strong)]">{item.metric_name}</div>
                <div className="text-xs text-[var(--muted)]">{shortId(item.metric_series_id)}</div>
              </td>
              <td>{item.metric_type}</td>
              <td>{item.unit || "none"}</td>
              <td>{item.environment || "default"}</td>
              <td>{labelSummary(item)}</td>
              <td>
                <Button
                  variant={selectedSeriesId === item.metric_series_id ? "primary" : "secondary"}
                  onClick={() => onSelect(item.metric_series_id)}
                  aria-pressed={selectedSeriesId === item.metric_series_id}
                >
                  Select {item.metric_name}
                </Button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </TablePanel>
  );
}

function MetricPointPreview({ selectedSeries }: { selectedSeries: MetricSeries | null }) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["metric-points", selectedSeries?.metric_series_id],
    queryFn: () => getMetricPoints(selectedSeries!.metric_series_id),
    enabled: Boolean(selectedSeries),
  });

  if (!selectedSeries) {
    return (
      <Panel eyebrow="Inspect" title="Selected series">
        <div className="signal-empty">Select a metric series to preview recent points.</div>
      </Panel>
    );
  }

  if (isLoading) {
    return (
      <Panel eyebrow="Inspect" title="Selected series">
        <LoadingState>Loading metric points...</LoadingState>
      </Panel>
    );
  }

  if (error) {
    return (
      <Panel eyebrow="Inspect" title="Selected series">
        <div className="signal-empty">Metric points could not be loaded.</div>
      </Panel>
    );
  }

  const points = data?.points ?? [];
  const latest = points.length ? points[points.length - 1] : undefined;

  return (
    <Panel
      eyebrow="Inspect"
      title="Selected series"
      actions={<span className="context-pill">{points.length} {plural(points.length, "point")}</span>}
    >
      <div className="grid grid-cols-[minmax(220px,0.8fr)_minmax(280px,1.2fr)] gap-4 max-[860px]:grid-cols-1">
        <dl className="definition-grid">
          <div>
            <dt>Name</dt>
            <dd>{selectedSeries.metric_name}</dd>
          </div>
          <div>
            <dt>Latest value</dt>
            <dd>{latest ? pointValue(latest) : "No points"}</dd>
          </div>
          <div>
            <dt>Type</dt>
            <dd>{selectedSeries.metric_type}</dd>
          </div>
          <div>
            <dt>Unit</dt>
            <dd>{selectedSeries.unit || "none"}</dd>
          </div>
        </dl>

        {points.length ? (
          <TablePanel>
            <table aria-label="Selected metric points">
              <thead>
                <tr>
                  <th>Time</th>
                  <th>Value</th>
                </tr>
              </thead>
              <tbody>
                {points.slice(-8).map((point) => (
                  <tr key={`${point.metric_series_id}-${point.time_unix_nano}`}>
                    <td>{String(point.time_unix_nano)}</td>
                    <td>{pointValue(point)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TablePanel>
        ) : (
          <div className="signal-empty">No points found for this metric series.</div>
        )}
      </div>
    </Panel>
  );
}

function MetricPointSummaryCard({ seriesId }: { seriesId: string | null }) {
  const { data } = useQuery({
    queryKey: ["metric-points", seriesId],
    queryFn: () => getMetricPoints(seriesId!),
    enabled: Boolean(seriesId),
  });

  return (
    <MetricCard
      label="Selected Points"
      value={seriesId ? `${data?.points.length ?? 0} ${plural(data?.points.length ?? 0, "point")}` : "none"}
      tone="info"
    />
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

function pointValue(point: MetricPoint): string {
  if (point.value_double != null) return String(point.value_double);
  if (point.value_int != null) return String(point.value_int);
  if (point.histogram_count != null) return `${point.histogram_count} buckets`;
  return "n/a";
}

function shortId(value: string): string {
  return value.slice(0, 8);
}

function plural(count: number, singular: string, pluralValue = `${singular}s`): string {
  return count === 1 ? singular : pluralValue;
}
