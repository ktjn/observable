/**
 * VisualizationPanel — auto-selects rendering based on VisualizationFrame.frame_type.
 *
 * Drives all rendering from the frame contract fields (x_field, y_field, field_roles, data)
 * as per ADR-021 §VisualizationFrame Contract. No chart library required.
 */
import type { VisualizationFrame } from "../../api/nlq";

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
  const unit = frame.unit ? ` (${frame.unit})` : "";

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
  const unit = frame.unit ? ` (${frame.unit})` : "";

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
  const unit = frame.unit ? ` ${frame.unit}` : "";
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

function GenericTable({ frame }: Props) {
  if (frame.data.length === 0) return null;
  const cols = Object.keys(frame.data[0]);

  return (
    <div className="overflow-x-auto" data-testid="generic-table">
      <table className="w-full text-sm border-collapse">
        <thead>
          <tr className="border-b border-[var(--border)]">
            {cols.map((c) => (
              <th key={c} className="py-1 pr-4 text-left font-medium whitespace-nowrap">
                {c}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {frame.data.map((row, i) => (
            <tr key={i} className="border-b border-[var(--border-subtle)]">
              {cols.map((c) => (
                <td key={c} className="py-1 pr-4 font-mono whitespace-nowrap">
                  {formatValue(row[c])}
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
