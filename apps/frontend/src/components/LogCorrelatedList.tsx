import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { LogRecord } from "../api/logs";
import { searchLogs } from "../api/logs";
import { LogContextView } from "./LogContextView";
import { LogList } from "./shared/LogList";
import { useTimeDisplay } from "../lib/timeDisplay";

interface Props {
  traceId: string;
  spanId?: string;
}

export function LogCorrelatedList({ traceId, spanId }: Props) {
  const [focusedLogId, setFocusedLogId] = useState<string | undefined>();
  const { format } = useTimeDisplay();
  const { data, isLoading } = useQuery({
    queryKey: ["logs", traceId],
    queryFn: () => searchLogs({ trace_id: traceId }),
  });

  const logs = filterCorrelatedLogs(data?.logs ?? [], spanId);

  return (
    <div className="mt-5">
      <h3 className="text-sm font-bold text-[var(--text-strong)] mb-2">
        {spanId
          ? `Exact span logs and trace-level logs (${spanId.substring(0, 8)})`
          : "Trace-correlated logs"}
      </h3>
      <LogList
        logs={logs}
        loading={isLoading}
        emptyMessage="No correlated logs found."
        onRowClick={(log) => setFocusedLogId(log.log_id)}
        showTraceLink
        timeFormat={format}
      />
      {focusedLogId && (
        <LogContextView logId={focusedLogId} onClose={() => setFocusedLogId(undefined)} />
      )}
    </div>
  );
}

export function filterCorrelatedLogs(logs: LogRecord[], spanId?: string): LogRecord[] {
  if (!spanId) return logs;
  return logs.filter((log) => log.span_id === spanId || !log.span_id);
}

export function correlationLabel(log: LogRecord): "Exact span" | "Trace-level" {
  return log.span_id ? "Exact span" : "Trace-level";
}
