import type { LogRecord } from "../../api/logs";
import { formatTimestamp } from "../../utils/formatTimestamp";
import { formatLogMessage, severityTextClass } from "../../utils/logFormatting";
import type { TimeFormat } from "../../lib/timeDisplay";
import { CopyButton } from "../ui/copy-button";

export interface LogListProps {
  logs: LogRecord[];
  loading?: boolean;
  emptyMessage?: string;
  pivotId?: string;
  onRowClick?: (log: LogRecord) => void;
  showTraceLink?: boolean;
  timeFormat: TimeFormat;
}

export function LogList({
  logs,
  loading = false,
  emptyMessage = "No logs found.",
  pivotId,
  onRowClick,
  showTraceLink = false,
  timeFormat,
}: LogListProps) {
  if (loading) {
    return <p className="text-sm text-[var(--muted)]">Loading logs…</p>;
  }
  if (!logs.length) {
    return <p className="text-sm text-[var(--muted)]">{emptyMessage}</p>;
  }

  return (
    <div className="font-mono text-xs max-h-[400px] overflow-y-auto border border-[var(--border)] bg-[var(--surface)] p-2">
      {logs.map((log) => {
        const isPivot = pivotId !== undefined && log.log_id === pivotId;
        return (
          <div
            key={log.log_id}
            data-log-id={log.log_id}
            role={onRowClick ? "listitem" : undefined}
            tabIndex={onRowClick ? 0 : undefined}
            onClick={onRowClick ? () => onRowClick(log) : undefined}
            onKeyDown={
              onRowClick
                ? (e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      onRowClick(log);
                    }
                  }
                : undefined
            }
            className={[
              "group flex gap-3 py-1 border-b border-[var(--border)] last:border-b-0",
              isPivot ? "bg-[var(--warn-bg)] font-bold" : "",
              onRowClick ? "cursor-pointer hover:bg-[var(--surface-subtle)]" : "",
            ]
              .filter(Boolean)
              .join(" ")}
          >
            <span className="text-[var(--muted)] shrink-0">
              {formatTimestamp(log.timestamp_unix_nano, timeFormat)}
            </span>
            <span className={`w-[50px] shrink-0 font-bold ${severityTextClass(log.severity_number)}`}>
              {log.severity_text || `LVL ${log.severity_number}`}
            </span>
            <span className="flex-1 min-w-0 break-all inline-flex items-start gap-1">
              <span className="min-w-0 break-all">{formatLogMessage(log.body)}</span>
              <CopyButton value={formatLogMessage(log.body)} label="Copy message" />
            </span>
            {showTraceLink && log.trace_id && (
              <a
                href={`/traces/${log.trace_id}`}
                className="shrink-0 text-[var(--brand)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)]"
                aria-label={log.span_id ? `View span ${log.span_id}` : `View trace ${log.trace_id}`}
              >
                trace
              </a>
            )}
            {isPivot && (
              <span className="text-[var(--warn)] text-[10px] shrink-0">[PIVOT]</span>
            )}
          </div>
        );
      })}
    </div>
  );
}
