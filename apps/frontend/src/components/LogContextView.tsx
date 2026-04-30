import { useQuery } from "@tanstack/react-query";
import { getLogContext, LogRecord } from "../api/logs";
import { Button } from "./ui/button";
import { formatLogMessage, getSeverityColor } from "../utils/logFormatting";

interface Props {
  logId: string;
  onClose: () => void;
}

export function LogContextView({ logId, onClose }: Props) {
  const { data, isLoading } = useQuery({
    queryKey: ["logs", "context", logId],
    queryFn: () => getLogContext(logId),
  });

  if (isLoading) {
    return (
      <div className="mt-3 p-3 bg-[var(--surface-inset)] border border-[var(--border)] rounded-lg">
        <p className="m-0 text-sm text-[var(--muted)]">Loading context…</p>
      </div>
    );
  }

  return (
    <div className="mt-3 p-3 bg-[var(--surface-inset)] border border-[var(--border)] rounded-lg">
      <div className="flex justify-between items-center mb-3">
        <h4 className="m-0 text-sm font-bold text-[var(--text-strong)]">Surrounding Logs</h4>
        <Button variant="secondary" onClick={onClose}>Close</Button>
      </div>
      <div className="font-mono text-xs max-h-[400px] overflow-y-auto border border-[var(--border)] rounded bg-[var(--surface)] p-2">
        {data?.logs.map((log: LogRecord) => {
          const isPivot = log.log_id === logId;
          return (
            <div
              key={log.log_id}
              className={`flex gap-3 py-1 border-b border-[var(--border)] last:border-b-0 ${
                isPivot ? "bg-[var(--warn-bg)] font-bold" : ""
              }`}
            >
              <span className="text-[var(--muted)] shrink-0">
                {new Date(Number(log.timestamp_unix_nano) / 1e6)
                  .toISOString()
                  .split("T")[1]
                  .replace("Z", "")}
              </span>
              <span
                className="w-[50px] shrink-0"
                style={{ color: getSeverityColor(log.severity_number) }}
              >
                {log.severity_text || `LVL ${log.severity_number}`}
              </span>
              <span className="flex-1 min-w-0 break-all">
                {formatLogMessage(log.body)}
              </span>
              {log.trace_id && (
                <a
                  href={`/traces/${log.trace_id}${log.span_id ? `#${log.span_id}` : ""}`}
                  className="shrink-0 text-[var(--brand)] hover:underline"
                  title={log.span_id ? `Span ${log.span_id}` : `Trace ${log.trace_id}`}
                >
                  {log.span_id ? "span" : "trace"}
                </a>
              )}
              {isPivot && (
                <span className="text-[var(--warn)] text-[10px] shrink-0">[PIVOT]</span>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

