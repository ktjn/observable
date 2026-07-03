/**
 * VisualizationPanel — auto-selects rendering based on VisualizationFrame.frame_type.
 *
 * Drives all rendering from the frame contract fields (x_field, y_field, field_roles, data)
 * as per ADR-021 §VisualizationFrame Contract. No chart library required.
 */
import type { VisualizationFrame } from "../../api/nlq";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp } from "../../utils/formatTimestamp";
import { CopyableText } from "../../components/ui/copy-button";

interface Props {
  frame: VisualizationFrame;
}

export function VisualizationPanel({ frame }: Props) {
  if (frame.data.length === 0) {
    return (
      <div
        className="py-8 text-center text-[var(--text-muted)]"
        data-testid="viz-empty"
      >
        No data returned for this query.
      </div>
    );
  }

  const renderer = getRenderer(frame);
  return (
    <div data-testid="viz-panel" data-frame-type={frame.frame_type}>
      {renderer}
    </div>
  );
}

function getRenderer(frame: VisualizationFrame) {
  switch (frame.frame_type) {
    case "timeseries":
      return <TimeseriesTable frame={frame} />;
    case "histogram":
      return <HistogramTable frame={frame} />;
    case "topk":
      return <TopkTable frame={frame} />;
    case "distribution":
      return <DistributionTable frame={frame} />;
    case "table":
    default:
      return <GenericTable frame={frame} />;
  }
}

// ── Timeseries ────────────────────────────────────────────────────────────────

function TimeseriesTable({ frame }: Props) {
  const xField = frame.x_field ?? "bucket";
  const yField = frame.y_field ?? "value";
  const seriesField = frame.series_field ?? null;
  const unit = frame.unit ? ` [${frame.unit}]` : "";

  return (
    <table
      className="w-full text-sm border-collapse"
      data-testid="timeseries-table"
    >
      <thead>
        <tr className="border-b border-[var(--border)]">
          <th className="py-1 pr-4 text-left font-medium">Time bucket</th>
          {seriesField && (
            <th className="py-1 pr-4 text-left font-medium">{seriesField}</th>
          )}
          <th className="py-1 text-right font-medium">
            {yField}
            {unit}
          </th>
        </tr>
      </thead>
      <tbody>
        {frame.data.map((row, i) => (
          <tr key={i} className="border-b border-[var(--border-subtle)]">
            <td className="py-1 pr-4 text-[var(--text-muted)]">
              {String(row[xField] ?? "—")}
            </td>
            {seriesField && (
              <td className="py-1 pr-4">{String(row[seriesField] ?? "—")}</td>
            )}
            <td className="py-1 text-right font-mono">
              {formatValue(row[yField])}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

// ── Histogram ─────────────────────────────────────────────────────────────────

function HistogramTable({ frame }: Props) {
  const xField = frame.x_field ?? "bound";
  const yField = frame.y_field ?? "count";
  const maxCount = Math.max(
    ...frame.data.map((r) => Number(r[yField] ?? 0)),
    1
  );

  return (
    <table className="w-full text-sm border-collapse" data-testid="histogram-table">
      <thead>
        <tr className="border-b border-[var(--border)]">
          <th className="py-1 pr-4 text-left font-medium">Bucket (≤)</th>
          <th className="py-1 text-right font-medium">Count</th>
          <th className="py-1 pl-4 text-left font-medium">Distribution</th>
        </tr>
      </thead>
      <tbody>
        {frame.data.map((row, i) => {
          const count = Number(row[yField] ?? 0);
          const barWidth = Math.round((count / maxCount) * 100);
          return (
            <tr key={i} className="border-b border-[var(--border-subtle)]">
              <td className="py-1 pr-4 font-mono">{formatValue(row[xField])}</td>
              <td className="py-1 text-right font-mono">{count}</td>
              <td className="py-1 pl-4">
                <div
                  className="h-3 rounded-sm bg-[var(--brand)]"
                  style={{ width: `${barWidth}%`, minWidth: count > 0 ? "2px" : "0" }}
                  aria-label={`${barWidth}% of max`}
                />
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

// ── Top-K ─────────────────────────────────────────────────────────────────────

function TopkTable({ frame }: Props) {
  const labelField = frame.x_field ?? "service_name";
  const valueField = frame.y_field ?? "avg_value";
  const unit = frame.unit ? ` [${frame.unit}]` : "";

  return (
    <table className="w-full text-sm border-collapse" data-testid="topk-table">
      <thead>
        <tr className="border-b border-[var(--border)]">
          <th className="py-1 pr-4 text-left font-medium">Rank</th>
          <th className="py-1 pr-4 text-left font-medium">{labelField}</th>
          <th className="py-1 text-right font-medium">
            Avg{unit}
          </th>
        </tr>
      </thead>
      <tbody>
        {frame.data.map((row, i) => (
          <tr key={i} className="border-b border-[var(--border-subtle)]">
            <td className="py-1 pr-4 text-[var(--text-muted)]">#{i + 1}</td>
            <td className="py-1 pr-4">{String(row[labelField] ?? "—")}</td>
            <td className="py-1 text-right font-mono">
              {formatValue(row[valueField])}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

// ── Distribution ──────────────────────────────────────────────────────────────

function DistributionTable({ frame }: Props) {
  const unit = frame.unit ? ` [${frame.unit}]` : "";
  // Data-driven: show exactly the columns the backend returned, in their order.
  const stats = frame.data.length > 0 ? Object.keys(frame.data[0]) : [];

  return (
    <table className="w-full text-sm border-collapse" data-testid="distribution-table">
      <thead>
        <tr className="border-b border-[var(--border)]">
          <th className="py-1 pr-8 text-left font-medium">Stat</th>
          <th className="py-1 text-right font-medium">
            Value{unit}
          </th>
        </tr>
      </thead>
      <tbody>
        {frame.data.flatMap((row, i) =>
          stats.map((stat) => (
            <tr key={`${i}-${stat}`} className="border-b border-[var(--border-subtle)]">
              <td className="py-1 pr-8 font-medium">{formatPercentileLabel(stat)}</td>
              <td className="py-1 text-right font-mono">{formatValue(row[stat])}</td>
            </tr>
          ))
        )}
      </tbody>
    </table>
  );
}

// ── Generic table ─────────────────────────────────────────────────────────────

/** Human-readable labels for well-known signal field names. */
const COLUMN_LABEL: Record<string, string> = {
  // Timestamps — always pinned first; see TIMESTAMP_COLS
  timestamp_unix_nano:              "Occurred Time",
  observed_timestamp_unix_nano:     "Observed Time",
  start_time_unix_nano:             "Start Time",
  end_time_unix_nano:               "End Time",
  last_seen_unix_nano:              "Last Seen",
  event_time_unix_nano:             "Occurred Time",
  // Logs
  body:                      "Message",
  severity_text:             "Severity",
  severity_number:           "Sev#",
  service_name:              "Service",
  log_id:                    "Log ID",
  trace_id:                  "Trace ID",
  span_id:                   "Span ID",
  resource_attributes:       "Resources",
  tenant_id:                 "Tenant",
  // Traces
  operation_name:            "Operation",
  duration_ns:               "Duration (ns)",
  duration_ms:               "Duration (ms)",
  status_code:               "Status",
  root_service:              "Service",
  root_operation:            "Operation",
  span_count:                "Spans",
  // Infrastructure
  entity_type:               "Type",
  entity_id:                 "Entity ID",
  display_name:              "Entity",
  environment:               "Environment",
  health_state:              "Health",
  parent_display_name:       "Parent",
  related_services:          "Services",
  error_rate:                "Error Rate",
  restart_count:             "Restarts",
  cpu_usage:                 "CPU",
  memory_usage:              "Memory",
  // Metrics / general
  metric_name:               "Metric",
  bucket:                    "Time bucket",
  value:                     "Value",
  avg_value:                 "Avg value",
};

/**
 * Parse a ClickHouse DateTime64 string ("YYYY-MM-DD HH:MM:SS[.frac]", UTC) into a
 * synthetic nanosecond string compatible with formatTimestamp().
 */
function parseCHDatetime(val: string): string {
  const [datePart = "", timePart = ""] = val.split(" ");
  const [hmsPart = "00:00:00", fracStr = ""] = timePart.split(".");
  const [yyyy = "1970", mo = "1", dd = "1"] = datePart.split("-");
  const [hh = "0", min = "0", sec = "0"] = hmsPart.split(":");
  const fracPadded = fracStr.padEnd(9, "0").slice(0, 9);
  const fracMs = +fracPadded.slice(0, 3);
  const epochMs = Date.UTC(+yyyy, +mo - 1, +dd, +hh, +min, +sec) + fracMs;
  // Concat epochMs + sub-ms digits → formatTimestamp reads ms via /1e6, subMs via slice(-6)
  return `${epochMs}${fracPadded.slice(3)}`;
}

/** Columns that contain nanosecond timestamps and should be formatted via formatTimestamp. */
const TIMESTAMP_COLS = new Set([
  "timestamp_unix_nano",
  "observed_timestamp_unix_nano",
  "start_time_unix_nano",
  "end_time_unix_nano",
  "last_seen_unix_nano",
  "event_time_unix_nano",
]);

/** Columns holding opaque identifiers users typically paste elsewhere — get a copy affordance. */
const ID_COLS = new Set(["log_id", "trace_id", "span_id", "entity_id", "tenant_id"]);

/** Sort columns so that timestamp fields come first, then the rest in backend order. */
function sortColumns(cols: string[]): string[] {
  const ts = cols.filter((c) => TIMESTAMP_COLS.has(c));
  const rest = cols.filter((c) => !TIMESTAMP_COLS.has(c));
  return [...ts, ...rest];
}

function GenericTable({ frame }: Props) {
  const { format } = useTimeDisplay();
  if (frame.data.length === 0) return null;
  const cols = sortColumns(Object.keys(frame.data[0]));

  return (
    <div className="overflow-x-auto" data-testid="generic-table">
      <table className="w-full text-sm border-collapse">
        <thead>
          <tr className="border-b border-[var(--border)]">
            {cols.map((c) => (
              <th key={c} className="py-1 pr-4 text-left font-medium whitespace-nowrap">
                {COLUMN_LABEL[c] ?? c}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {frame.data.map((row, i) => (
            <tr key={i} className="border-b border-[var(--border-subtle)]">
              {cols.map((c) => (
                <td key={c} className="py-1 pr-4 font-mono whitespace-nowrap">
                  {TIMESTAMP_COLS.has(c) && row[c] != null ? (
                    formatTimestamp(
                      typeof row[c] === "string" && (row[c] as string).includes(" ")
                        ? parseCHDatetime(row[c] as string)
                        : (row[c] as string | number),
                      format,
                    )
                  ) : ID_COLS.has(c) && row[c] != null ? (
                    <CopyableText value={formatValue(row[c])} label={`Copy ${COLUMN_LABEL[c] ?? c}`} />
                  ) : (
                    formatValue(row[c])
                  )}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatValue(v: unknown): string {
  if (v === null || v === undefined) return "—";
  if (typeof v === "number") {
    if (isNaN(v)) return "—";
    return v % 1 === 0 ? v.toString() : v.toFixed(3);
  }
  return String(v);
}

function formatPercentileLabel(key: string): string {
  // Named aliases.
  const named: Record<string, string> = {
    median: "median",
    average: "average",
    mean: "mean",
    min: "min",
    max: "max",
    // Legacy aliases from old SQL templates.
    min_val: "min",
    max_val: "max",
    p50: "p50 (median)",
  };
  if (named[key]) return named[key];
  // p{N} — display as-is.
  if (/^p\d+$/.test(key)) return key;
  return key;
}
