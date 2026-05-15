import { Link } from "@tanstack/react-router";
import type { TraceResponse } from "../../../api/traces";
import { Badge } from "../../../components/ui/badge";
import { VirtualTable } from "../../../components/ui/VirtualTable";
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
    <VirtualTable
      rows={traces}
      ariaLabel={ariaLabel}
      renderHead={() => (
        <tr>
          <th aria-label="Time">Time</th>
          <th>Trace ID</th>
          {showServiceColumn && <th>Service</th>}
          <th>Operation</th>
          <th>Duration</th>
          <th>Status</th>
        </tr>
      )}
      renderRow={(trace, ref, index) => (
        <TraceResultsRow
          key={trace.trace_id}
          trace={trace}
          selected={selectedTraceId === trace.trace_id}
          onSelect={() => onSelectTrace(trace.trace_id)}
          mode={mode}
          showServiceColumn={showServiceColumn}
          timeFormat={timeFormat}
          measureRef={ref}
          index={index}
        />
      )}
    />
  );
}

function durationToneClass(durationNs: number): string {
  const ms = durationNs / 1_000_000;
  if (ms > 500) return "text-[var(--bad)]";
  if (ms > 100) return "text-[var(--warn)]";
  return "text-[var(--good)]";
}

function TraceResultsRow({
  trace,
  selected,
  onSelect,
  mode,
  showServiceColumn,
  timeFormat,
  measureRef,
  index,
}: {
  trace: TraceResponse;
  selected: boolean;
  onSelect: () => void;
  mode: "select" | "link";
  showServiceColumn: boolean;
  timeFormat: TimeFormat;
  measureRef: (el: Element | null) => void;
  index: number;
}) {
  const root = trace.spans[0];
  if (!root) return null;

  const isError = root.status_code === "ERROR";
  const isSlowMs = root.duration_ns / 1_000_000 > 2000;
  const accentClass = isError
    ? "border-l-2 border-l-[var(--bad)]"
    : isSlowMs
      ? "border-l-2 border-l-[var(--warn)]"
      : "";

  return (
    <tr
      ref={measureRef}
      data-index={index}
      className={`modern-table-row ${accentClass} ${mode === "select" ? "cursor-pointer" : ""} ${selected ? "bg-[var(--surface-subtle)]" : ""}`}
      onClick={mode === "select" ? onSelect : undefined}
      onKeyDown={
        mode === "select" ? (e) => (e.key === "Enter" || e.key === " ") && onSelect() : undefined
      }
      tabIndex={mode === "select" ? 0 : undefined}
      role={mode === "select" ? "button" : undefined}
      aria-label={mode === "select" ? `${trace.trace_id.substring(0, 16)}…` : undefined}
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
      <td className="whitespace-normal break-all">{root.operation_name}</td>
      <td className={`whitespace-nowrap font-mono ${durationToneClass(root.duration_ns)}`}>
        {(root.duration_ns / 1e6).toFixed(2)}ms
      </td>
      <td>
        <Badge tone={isError ? "bad" : "good"}>{root.status_code}</Badge>
      </td>
    </tr>
  );
}
