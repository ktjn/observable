import { useQuery } from "@tanstack/react-query";
import { getLogContext, LogRecord } from "../api/logs";
import { Button } from "./ui/button";

interface Props {
  logId: string;
  onClose: () => void;
}

export function LogContextView({ logId, onClose }: Props) {
  const { data, isLoading } = useQuery({
    queryKey: ["logs", "context", logId],
    queryFn: () => getLogContext(logId),
  });

  if (isLoading) return <div style={containerStyle}><p>Loading context…</p></div>;

  return (
    <div style={containerStyle}>
      <div style={{ display: "flex", justifyContent: "space-between", marginBottom: "12px" }}>
        <h4 style={{ margin: 0 }}>Surrounding Logs</h4>
        <Button variant="secondary" onClick={onClose}>Close</Button>
      </div>
      <div style={{ 
        fontFamily: "monospace", 
        fontSize: "12px", 
        maxHeight: "400px", 
        overflowY: "auto",
        border: "1px solid #cbd5e0",
        borderRadius: "4px",
        padding: "8px",
        background: "#fff"
      }}>
        {data?.logs.map((log: LogRecord) => {
          const isPivot = log.log_id === logId;
          return (
            <div key={log.log_id} style={{ 
              display: "flex", 
              gap: "12px", 
              padding: "4px 0",
              borderBottom: "1px solid #f7fafc",
              background: isPivot ? "#fffaf0" : "transparent",
              fontWeight: isPivot ? "bold" : "normal"
            }}>
              <span style={{ color: "#718096" }}>
                {new Date(Number(log.timestamp_unix_nano) / 1e6).toISOString().split('T')[1].replace('Z', '')}
              </span>
              <span style={{ 
                width: "50px",
                color: getSeverityColor(log.severity_number)
              }}>
                {log.severity_text || `LVL ${log.severity_number}`}
              </span>
              <span style={{ flex: 1 }}>
                {typeof log.body === 'string' ? log.body : JSON.stringify(log.body)}
              </span>
              {isPivot && <span style={{ color: "#dd6b20", fontSize: "10px" }}>[PIVOT]</span>}
            </div>
          );
        })}
      </div>
    </div>
  );
}

const containerStyle: React.CSSProperties = {
  marginTop: "12px",
  padding: "12px",
  background: "#f8fafc",
  border: "1px solid #e2e8f0",
  borderRadius: "8px"
};

function getSeverityColor(severity: number): string {
  if (severity >= 17) return "#e53e3e";
  if (severity >= 13) return "#e53e3e";
  if (severity >= 9) return "#dd6b20";
  if (severity >= 5) return "#3182ce";
  return "#718096";
}
