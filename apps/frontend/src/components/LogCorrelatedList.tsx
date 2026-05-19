import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { LogRecord } from "../api/logs";
import { searchLogs } from "../api/logs";
import { LogContextView } from "./LogContextView";
import { LogList } from "./shared/LogList";
import { useTimeDisplay } from "../lib/timeDisplay";
import { useTenantContext } from "../hooks/useTenantContext";

interface Props {
  traceId: string;
  spanId?: string;
}

export function LogCorrelatedList({ traceId, spanId }: Props) {
  const [focusedLogId, setFocusedLogId] = useState<string | undefined>();
  const { format } = useTimeDisplay();
  const { tenantId } = useTenantContext();
  const { data, isLoading } = useQuery({
    queryKey: ["logs", tenantId, traceId],
    queryFn: () => searchLogs(tenantId, { trace_id: traceId }),
  });

  const logs = filterCorrelatedLogs(data?.logs ?? [], spanId);

  return (
    <div>
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
