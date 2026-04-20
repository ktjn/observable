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

  if (isLoading) return <p>Loading logs…</p>;
  const logs = filterCorrelatedLogs(data?.logs ?? [], spanId);
  if (!logs.length) return <p>No correlated logs found.</p>;

  return (
    <div style={{ marginTop: 20 }}>
      <h3>
        {spanId
          ? `Exact span logs and trace-level logs (${spanId.substring(0, 8)})`
          : "Trace-correlated logs"}
      </h3>
      <div style={{ 
        fontFamily: "monospace", 
        fontSize: "12px", 
        maxHeight: "300px", 
        overflowY: "auto",
        border: "1px solid #e2e8f0",
        borderRadius: "4px",
        padding: "8px"
      }}>
        {logs.map((log) => (
          <div 
            key={log.log_id} 
            onClick={() => setFocusedLogId(log.log_id)}
            style={{ 
              display: "flex", 
              gap: "12px", 
              padding: "4px 0",
              borderBottom: "1px solid #f7fafc",
              cursor: "pointer",
              background: focusedLogId === log.log_id ? "#edf2f7" : "transparent"
            }}
          >
            <span style={{ color: "#718096" }}>
              {new Date(Number(log.timestamp_unix_nano) / 1e6).toISOString().split('T')[1].replace('Z', '')}
            </span>
            <span style={{ 
              fontWeight: "bold",
              width: "50px",
              color: getSeverityColor(log.severity_number)
            }}>
              {log.severity_text || `LVL ${log.severity_number}`}
            </span>
            <span style={{ width: "90px", color: log.span_id ? "#2b6cb0" : "#805ad5" }}>
              {correlationLabel(log)}
            </span>
            <span style={{ flex: 1 }}>
              {typeof log.body === 'string' ? log.body : JSON.stringify(log.body)}
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
  if (severity >= 17) return "#e53e3e"; // Critical/Fatal/Error
  if (severity >= 13) return "#e53e3e"; // Error
  if (severity >= 9) return "#dd6b20";  // Warn
  if (severity >= 5) return "#3182ce";  // Info
  return "#718096";                    // Debug/Trace
}
