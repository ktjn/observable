import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { searchTraces } from "../api/traces";
import { LogLiveTail } from "../components/LogLiveTail";

export default function TraceSearch() {
  const [service, setService] = useState("");
  const { data, isLoading, error } = useQuery({
    queryKey: ["traces", service],
    queryFn: () => searchTraces({ service: service || undefined, limit: 50 }),
  });

  return (
    <div className="trace-explorer-page" style={{ fontFamily: "monospace" }}>
      <h1>Trace Explorer</h1>
      <input
        placeholder="Filter by service"
        value={service}
        onChange={(e) => setService(e.target.value)}
        style={{ marginBottom: "1rem", padding: "0.5rem", width: "300px" }}
      />
      {isLoading && <p>Loading...</p>}
      {error && <p>Error: {String(error)}</p>}
      {data?.traces.length === 0 && <p>No traces found.</p>}
      <table style={{ borderCollapse: "collapse", width: "100%" }}>
        <thead>
          <tr>
            <th>Trace ID</th>
            <th>Service</th>
            <th>Operation</th>
            <th>Duration</th>
            <th>Status</th>
          </tr>
        </thead>
        <tbody>
          {data?.traces.map((t) => {
            const root = t.spans[0];
            if (!root) return null;
            return (
              <tr key={t.trace_id}>
                <td>{t.trace_id.substring(0, 16)}…</td>
                <td>{root.service_name}</td>
                <td>{root.operation_name}</td>
                <td>{(root.duration_ns / 1e6).toFixed(2)}ms</td>
                <td>{root.status_code}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
      <LogLiveTail />
    </div>
  );
}
