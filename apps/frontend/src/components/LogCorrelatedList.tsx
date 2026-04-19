import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { searchLogs } from "../api/logs";
import { LogContextView } from "./LogContextView";

interface Props {
  traceId: string;
  spanId?: string;
}

export function LogCorrelatedList({ traceId, spanId }: Props) {
  const [focusedLogId, setFocusedLogId] = useState<string | undefined>();
  const { data, isLoading } = useQuery({
    queryKey: ["logs", traceId, spanId],
    queryFn: () => searchLogs({ trace_id: traceId, span_id: spanId }),
  });

  if (isLoading) return <p>Loading logs…</p>;
  if (!data?.logs?.length) return <p>No correlated logs found.</p>;

  return (
    <div style={{ marginTop: 20 }}>
      <h3>Correlated Logs {spanId ? `(Span: ${spanId.substring(0, 8)})` : "(Trace)"}</h3>
      <div style={{ 
        fontFamily: "monospace", 
        fontSize: "12px", 
        maxHeight: "300px", 
        overflowY: "auto",
        border: "1px solid #e2e8f0",
        borderRadius: "4px",
        padding: "8px"
      }}>
        {data.logs.map((log) => (
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

function getSeverityColor(severity: number): string {
  if (severity >= 17) return "#e53e3e"; // Critical/Fatal/Error
  if (severity >= 13) return "#e53e3e"; // Error
  if (severity >= 9) return "#dd6b20";  // Warn
  if (severity >= 5) return "#3182ce";  // Info
  return "#718096";                    // Debug/Trace
}
