import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { searchLogs } from "../api/logs";
import { FacetSidebar } from "../components/FacetSidebar";

export default function LogSearch() {
  const [service, setService] = useState("");
  
  const { data, isLoading, error } = useQuery({
    queryKey: ["logs", service],
    queryFn: () => searchLogs({ 
      service: service || undefined, 
      limit: 50,
      facets: ["service_name", "severity_number", "environment", "host_id"]
    }),
  });

  const handleFacetClick = (field: string, value: string) => {
    if (field === "service_name") {
      setService(value);
    }
  };

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="field-label">Explorer</div>
          <h1>Logs</h1>
        </div>
      </div>

      <div className="toolbar-row">
        <input
          className="search-input"
          placeholder="Filter by service"
          value={service}
          onChange={(e) => setService(e.target.value)}
          aria-label="Filter by service"
        />
      </div>

      <div style={{ display: "flex", gap: "2rem", alignItems: "flex-start" }}>
        <FacetSidebar facets={data?.facets} onFacetClick={handleFacetClick} />

        <div className="table-panel" style={{ flex: 1 }}>
          {isLoading ? (
            <div className="loading-state">Loading logs...</div>
          ) : error ? (
            <div className="signal-empty">Error loading logs: {String(error)}</div>
          ) : data?.logs.length === 0 ? (
            <div className="signal-empty">No logs found.</div>
          ) : (
            <table style={{ borderCollapse: "collapse", width: "100%" }}>
              <thead>
                <tr>
                  <th>Timestamp</th>
                  <th>Service</th>
                  <th>Level</th>
                  <th>Message</th>
                </tr>
              </thead>
              <tbody>
                {data?.logs.map((log) => (
                  <tr key={log.log_id}>
                    <td>{log.timestamp_unix_nano}</td>
                    <td>{log.service_name}</td>
                    <td>
                      <span className={`status ${severityTone(log.severity_number)}`}>
                        {log.severity_text || log.severity_number}
                      </span>
                    </td>
                    <td>{String(log.body)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </section>
  );
}

function severityTone(severity: number) {
  if (severity >= 17) return "bad"; // Error
  if (severity >= 13) return "warn"; // Warn
  if (severity >= 9) return "info"; // Info
  return "good"; // Debug/Trace
}
