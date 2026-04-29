import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { searchTraces } from "../api/traces";
import { FacetSidebar } from "../components/FacetSidebar";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { LoadingState } from "../components/ui/loading-state";
import { TablePanel } from "../components/ui/table-panel";

export default function TraceSearch() {
  const [service, setService] = useState("");
  
  const { data, isLoading, error } = useQuery({
    queryKey: ["traces", service],
    queryFn: () => searchTraces({ 
      service: service || undefined, 
      limit: 50,
      facets: ["service_name", "status_code", "span_kind"]
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
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Explorer</div>
          <h1>Traces</h1>
        </div>
      </div>

      <div className="toolbar-row">
        <Input
          className="max-w-[360px]"
          placeholder="Filter by service"
          value={service}
          onChange={(e) => setService(e.target.value)}
          aria-label="Filter by service"
        />
        {service && (
          <Button variant="secondary" onClick={() => setService("")}>
            Clear filters
          </Button>
        )}
      </div>

      <div className="flex items-start">
        <FacetSidebar facets={data?.facets} onFacetClick={handleFacetClick} />

        <TablePanel className="flex-1">
          {isLoading ? (
            <LoadingState>Loading traces…</LoadingState>
          ) : error ? (
            <div className="signal-empty">Error loading traces: {String(error)}</div>
          ) : data?.traces.length === 0 ? (
            <div className="signal-empty">No traces found.</div>
          ) : (
            <table>
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
                      <td className="strong-cell">
                        <Link to="/traces/$traceId" params={{ traceId: t.trace_id }}>
                          {t.trace_id.substring(0, 16)}…
                        </Link>
                      </td>
                      <td>{root.service_name}</td>
                      <td>{root.operation_name}</td>
                      <td>{(root.duration_ns / 1e6).toFixed(2)}ms</td>
                      <td>
                        <Badge tone={root.status_code === "ERROR" ? "bad" : "good"}>
                          {root.status_code}
                        </Badge>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </TablePanel>
      </div>
    </section>
  );
}
