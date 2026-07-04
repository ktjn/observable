import { Link } from "@tanstack/react-router";
import type { TraceResponse } from "../../../api/traces";
import { Badge } from "../../../components/ui/badge";
import { CopyButton } from "../../../components/ui/copy-button";
import { DurationCell } from "../../../components/ui/metric-cells";
import { VirtualTable } from "../../../components/ui/VirtualTable";
import { formatTimestamp } from "../../../utils/formatTimestamp";
import { formatStatusLabel } from "../../../utils/traceStatus";
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

  // The row nests a real <Link> to the trace's full detail page, so it can't
  // also carry role="button"/tabIndex (nested-interactive a11y violation —
  // assistive tech can't handle a focusable control inside another one).
  // Row click remains a mouse-only convenience; keyboard/SR users select via
  // the link's Enter/click, which navigates straight to the same trace.
  return (
    <tr
      ref={measureRef}
      data-index={index}
      className={`modern-table-row ${accentClass} ${mode === "select" ? "cursor-pointer" : ""} ${selected ? "bg-[var(--surface-subtle)]" : ""}`}
      onClick={mode === "select" ? onSelect : undefined}
    >
      <td className="whitespace-nowrap">{formatTimestamp(root.start_time_unix_nano, timeFormat)}</td>
      <td className="strong-cell group">
        <span className="inline-flex items-center gap-1">
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
          <CopyButton value={trace.trace_id} label="Copy trace id" />
        </span>
      </td>
      {showServiceColumn && <td>{root.service_name}</td>}
      <td className="whitespace-normal break-all">{root.operation_name}</td>
      <td className="whitespace-nowrap font-mono">
        <DurationCell durationNs={root.duration_ns} />
      </td>
      <td>
        <Badge tone={isError ? "bad" : "good"}>{formatStatusLabel(root.status_code)}</Badge>
      </td>
    </tr>
  );
}
