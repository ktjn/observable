import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { createDashboard } from "../api/dashboards";
import { searchTraces } from "../api/traces";
import { FacetSidebar } from "../components/FacetSidebar";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { LoadingState } from "../components/ui/loading-state";
import { Select, SelectOption } from "../components/ui/select";
import { TablePanel } from "../components/ui/table-panel";

export default function TraceSearch() {
  const [service, setService] = useState(() => new URLSearchParams(window.location.search).get("service") ?? "");
  const [lookback, setLookback] = useState(60);
  const [saveStatus, setSaveStatus] = useState<"idle" | "saving" | "saved" | "error">("idle");
  
  const { data, isLoading, error } = useQuery({
    queryKey: ["traces", service, lookback],
    queryFn: () => searchTraces({
      service: service || undefined,
      lookback_minutes: lookback,
      limit: 50,
      facets: ["service_name", "status_code", "span_kind"]
    }),
  });

  const handleFacetClick = (field: string, value: string) => {
    if (field === "service_name") {
      setService(value);
    }
  };

  const handlePromote = async () => {
    setSaveStatus("saving");
    try {
      await createDashboard({
        name: service ? `Traces for ${service}` : "Promoted trace query",
        panels: [
          {
            title: service ? `Traces for ${service}` : "Trace search",
            query_kind: "traces",
            service: service || undefined,
            lookback_minutes: 60,
            filters: { facets: ["service_name", "status_code", "span_kind"] },
          },
        ],
      });
      setSaveStatus("saved");
    } catch (error) {
      console.error(error);
      setSaveStatus("error");
    }
  };

  return (
    <section className="page-stack trace-explorer-page">
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
        <div className="flex flex-col">
          <label htmlFor="lookback-select" className="text-xs font-medium text-[var(--muted)] mb-1">
            Lookback
          </label>
          <Select
            id="lookback-select"
            value={lookback}
            onChange={(e) => setLookback(parseInt(e.target.value, 10))}
            aria-label="Select lookback time period"
          >
            <SelectOption value={15}>Last 15 min</SelectOption>
            <SelectOption value={60}>Last 1 hour</SelectOption>
            <SelectOption value={360}>Last 6 hours</SelectOption>
            <SelectOption value={1440}>Last 24 hours</SelectOption>
            <SelectOption value={10080}>Last 7 days</SelectOption>
          </Select>
        </div>
        {service && (
          <Button variant="secondary" onClick={() => setService("")}>
            Clear filters
          </Button>
        )}
        <Button onClick={handlePromote} disabled={saveStatus === "saving"}>
          Promote to dashboard
        </Button>
        {saveStatus === "saved" && (
          <span className="text-sm font-semibold text-[var(--good)]">Saved to dashboard</span>
        )}
        {saveStatus === "error" && (
          <span className="text-sm font-semibold text-[var(--bad)]">Dashboard save failed</span>
        )}
      </div>

      <div className="flex items-start gap-3 max-[760px]:flex-col">
        <FacetSidebar
          facets={data?.facets}
          onFacetClick={handleFacetClick}
          ariaLabel="Trace facets"
        />

        <TablePanel className="flex-1">
          {isLoading ? (
            <LoadingState>Loading traces…</LoadingState>
          ) : error ? (
            <LoadingState className="text-[var(--bad)]">Error loading traces: {String(error)}</LoadingState>
          ) : data?.traces.length === 0 ? (
            <LoadingState>No traces found.</LoadingState>
          ) : (
            <table aria-label="Trace results">
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
                    <tr key={t.trace_id} className="modern-table-row">
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
