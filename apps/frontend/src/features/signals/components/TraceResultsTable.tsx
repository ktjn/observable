import { Link } from "@tanstack/react-router";
import type { TraceResponse } from "../../../api/traces";
import { Badge } from "../../../components/ui/badge";
import { formatTimestamp } from "../../../utils/formatTimestamp";
import type { TimeFormat } from "../../../lib/timeDisplay";

export function TraceResultsTable({
  traces,
  selectedTraceId,
  onSelectTrace,
  mode = "select",
  showServiceColumn = true,
  timeFormat = "iso-local-ms",
  ariaLabel = showServiceColumn ? "Trace results" : "Service traces",
}: {
  traces: TraceResponse[];
  selectedTraceId: string | undefined;
  onSelectTrace: (traceId: string) => void;
  mode?: "select" | "link";
  showServiceColumn?: boolean;
  timeFormat?: TimeFormat;
  ariaLabel?: string;
}) {
  return (
    <table aria-label={ariaLabel}>
      <thead>
        <tr>
          <th aria-label="Time">Time</th>
          <th>Trace ID</th>
          {showServiceColumn && <th>Service</th>}
          <th>Operation</th>
          <th>Duration</th>
          <th>Status</th>
        </tr>
      </thead>
      <tbody>
        {traces.map((trace) => (
          <TraceResultsRow
            key={trace.trace_id}
            trace={trace}
            selected={selectedTraceId === trace.trace_id}
            onSelect={() => onSelectTrace(trace.trace_id)}
            mode={mode}
            showServiceColumn={showServiceColumn}
            timeFormat={timeFormat}
          />
        ))}
      </tbody>
    </table>
  );
}

function TraceResultsRow({
  trace,
  selected,
  onSelect,
  mode,
  showServiceColumn,
  timeFormat,
}: {
  trace: TraceResponse;
  selected: boolean;
  onSelect: () => void;
  mode: "select" | "link";
  showServiceColumn: boolean;
  timeFormat: TimeFormat;
}) {
  const root = trace.spans[0];
  if (!root) return null;

  return (
    <tr
      className={`modern-table-row ${mode === "select" ? "cursor-pointer" : ""} ${selected ? "bg-[var(--surface-subtle)]" : ""}`}
      onClick={mode === "select" ? onSelect : undefined}
      onKeyDown={mode === "select" ? (e) => (e.key === "Enter" || e.key === " ") && onSelect() : undefined}
      tabIndex={mode === "select" ? 0 : undefined}
      role={mode === "select" ? "button" : undefined}
      aria-label={mode === "select" ? `Open trace ${trace.trace_id.substring(0, 16)}` : undefined}
      aria-pressed={mode === "select" ? selected : undefined}
    >
      <td className="whitespace-nowrap">{formatTimestamp(root.start_time_unix_nano, timeFormat)}</td>
      <td className="strong-cell">
        {mode === "link" ? (
          <Link to="/traces/$traceId" params={{ traceId: trace.trace_id }}>
            {trace.trace_id.substring(0, 16)}
          </Link>
        ) : (
          <Link
            to="/traces/$traceId"
            params={{ traceId: trace.trace_id }}
            onClick={(e) => e.stopPropagation()}
          >
            {trace.trace_id.substring(0, 16)}…
          </Link>
        )}
      </td>
      {showServiceColumn && <td>{root.service_name}</td>}
      <td>{root.operation_name}</td>
      <td>{(root.duration_ns / 1e6).toFixed(2)}ms</td>
      <td>
        <Badge tone={root.status_code === "ERROR" ? "bad" : "good"}>{root.status_code}</Badge>
      </td>
    </tr>
  );
}
