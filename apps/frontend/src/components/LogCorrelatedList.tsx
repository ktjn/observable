import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { LogRecord } from "../api/logs";
import { searchLogs } from "../api/logs";
import { LogContextView } from "./LogContextView";

interface Props {
  traceId: string;
  spanId?: string;
}

export function LogCorrelatedList({ traceId, spanId }: Props) {
  const [focusedLogId, setFocusedLogId] = useState<string | undefined>();
  const { data, isLoading } = useQuery({
    queryKey: ["logs", traceId],
    queryFn: () => searchLogs({ trace_id: traceId }),
  });

  if (isLoading) {
    return <p className="text-sm text-[var(--muted)]">Loading logs…</p>;
  }
  const logs = filterCorrelatedLogs(data?.logs ?? [], spanId);
  if (!logs.length) {
    return <p className="text-sm text-[var(--muted)]">No correlated logs found.</p>;
  }

  return (
    <div className="mt-5">
      <h3 className="text-sm font-bold text-[var(--text-strong)] mb-2">
        {spanId
          ? `Exact span logs and trace-level logs (${spanId.substring(0, 8)})`
          : "Trace-correlated logs"}
      </h3>
      <div className="font-mono text-xs max-h-[300px] overflow-y-auto border border-[var(--border)] rounded bg-[var(--surface)] p-2">
        {logs.map((log) => (
          <div
            key={log.log_id}
            role="button"
            tabIndex={0}
            onClick={() => setFocusedLogId(log.log_id)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                setFocusedLogId(log.log_id);
              }
            }}
            className={`flex gap-3 py-1 border-b border-[var(--border)] last:border-b-0 cursor-pointer ${
              focusedLogId === log.log_id ? "bg-[var(--surface-subtle)]" : ""
            }`}
          >
            <span className="text-[var(--muted)] shrink-0">
              {new Date(Number(log.timestamp_unix_nano) / 1e6)
                .toISOString()
                .split("T")[1]
                .replace("Z", "")}
            </span>
            <span
              className="font-bold w-[50px] shrink-0"
              style={{ color: getSeverityColor(log.severity_number) }}
            >
              {log.severity_text || `LVL ${log.severity_number}`}
            </span>
            <span
              className="w-[90px] shrink-0"
              style={{
                color: log.span_id ? "var(--brand)" : "var(--brand-strong)",
              }}
            >
              {correlationLabel(log)}
            </span>
            <span className="flex-1 min-w-0 break-all">
              {typeof log.body === "string" ? log.body : JSON.stringify(log.body)}
            </span>
          </div>
        ))}
      </div>
      {focusedLogId && (
        <LogContextView
          logId={focusedLogId}
          onClose={() => setFocusedLogId(undefined)}
        />
      )}
    </div>
  );
}

export function filterCorrelatedLogs(logs: LogRecord[], spanId?: string): LogRecord[] {
  if (!spanId) {
    return logs;
  }

  return logs.filter((log) => log.span_id === spanId || !log.span_id);
}

export function correlationLabel(log: LogRecord): "Exact span" | "Trace-level" {
  return log.span_id ? "Exact span" : "Trace-level";
}

function getSeverityColor(severity: number): string {
  if (severity >= 13) return "var(--bad)";
  if (severity >= 9) return "var(--warn)";
  if (severity >= 5) return "var(--brand)";
  return "var(--muted)";
}
