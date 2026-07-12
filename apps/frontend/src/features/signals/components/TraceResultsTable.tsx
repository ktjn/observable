import { Link } from "@tanstack/react-router";
import type { TraceResponse } from "../../../api/traces";
import { Badge } from "../../../components/ui/badge";
import { CopyButton } from "../../../components/ui/copy-button";
import { DurationCell } from "../../../components/ui/metric-cells";
import { VirtualTable } from "../../../components/ui/VirtualTable";
import type { TimeFormat } from "../../../lib/timeDisplay";
import { DEFAULT_TRACE_COLUMNS, getTraceFieldValue } from "../../../utils/traceContext";

export type TraceTableColumn = string;
const COLUMN_LABELS: Record<string, string> = {
  start_time: "Time", trace_id: "Trace ID", "service.name": "service.name",
  operation: "Operation", duration: "Duration", status: "Status",
};

export function TraceResultsTable({
  traces, selectedTraceId, onSelectTrace, mode = "select", showServiceColumn = true,
  visibleColumns, timeFormat = "iso-local-ms",
  ariaLabel = showServiceColumn ? "Trace results" : "Service traces",
}: {
  traces: TraceResponse[];
  selectedTraceId: string | undefined;
  onSelectTrace: (traceId: string) => void;
  mode?: "select" | "link";
  showServiceColumn?: boolean;
  visibleColumns?: readonly TraceTableColumn[];
  timeFormat?: TimeFormat;
  ariaLabel?: string;
}) {
  const columns = visibleColumns ?? DEFAULT_TRACE_COLUMNS.filter((key) => showServiceColumn || key !== "service.name");
  return (
    <VirtualTable
      rows={traces}
      ariaLabel={ariaLabel}
      renderHead={() => <tr>{columns.length === 0 ? <th>No columns selected</th> : columns.map((key) => <th key={key}>{COLUMN_LABELS[key] ?? key}</th>)}</tr>}
      renderRow={(trace, ref, index) => (
        <TraceResultsRow key={trace.trace_id} trace={trace} selected={selectedTraceId === trace.trace_id}
          onSelect={() => onSelectTrace(trace.trace_id)} mode={mode} columns={columns}
          timeFormat={timeFormat} measureRef={ref} index={index} />
      )}
    />
  );
}

function TraceResultsRow({ trace, selected, onSelect, mode, columns, timeFormat, measureRef, index }: {
  trace: TraceResponse; selected: boolean; onSelect: () => void; mode: "select" | "link";
  columns: readonly string[]; timeFormat: TimeFormat; measureRef: (el: Element | null) => void; index: number;
}) {
  const root = trace.spans[0];
  if (!root) return null;
  const isError = root.status_code === "ERROR";
  const isSlowMs = root.duration_ns / 1_000_000 > 2000;
  const accentClass = isError ? "border-l-2 border-l-[var(--bad)]" : isSlowMs ? "border-l-2 border-l-[var(--warn)]" : "";
  return (
    <tr ref={measureRef} data-index={index}
      className={`modern-table-row ${accentClass} ${mode === "select" ? "cursor-pointer" : ""} ${selected ? "bg-[var(--surface-subtle)]" : ""}`}
      onClick={mode === "select" ? onSelect : undefined}>
      {columns.length === 0 ? <td>No columns selected</td> : columns.map((key) => {
        const value = getTraceFieldValue(trace, key, timeFormat);
        return <td key={key} className={key === "operation" ? "whitespace-normal break-all" : undefined}>
          {key === "trace_id" ? (
            <span className="inline-flex items-center gap-1 strong-cell group">
              <Link to="/traces/$traceId" params={{ traceId: trace.trace_id }}
                onClick={mode === "select" ? (event) => event.stopPropagation() : undefined}>
                {mode === "link" ? trace.trace_id.substring(0, 16) : `${trace.trace_id.substring(0, 16)}…`}
              </Link>
              <CopyButton value={trace.trace_id} label="Copy trace id" />
            </span>
          ) : key === "duration" ? (
            <span className="whitespace-nowrap font-mono"><DurationCell durationNs={root.duration_ns} /></span>
          ) : key === "status" ? (
            <Badge tone={isError ? "bad" : "good"}>{value}</Badge>
          ) : value}
        </td>;
      })}
    </tr>
  );
}
