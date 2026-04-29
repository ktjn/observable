import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { searchLogs, LogRecord } from "../api/logs";
import { FacetSidebar } from "../components/FacetSidebar";
import { infraLinks } from "../utils/infraLinks";
import { formatTimestamp } from "../utils/formatTimestamp";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { LoadingState } from "../components/ui/loading-state";
import { TablePanel } from "../components/ui/table-panel";

export default function LogSearch() {
  const [service, setService] = useState("");
  const [utc, setUtc] = useState(false);

  const { data, isLoading, error } = useQuery({
    queryKey: ["logs", service],
    queryFn: () =>
      searchLogs({
        service: service || undefined,
        limit: 50,
        facets: ["service_name", "severity_number", "environment", "host_id"],
      }),
  });

  const handleFacetClick = (field: string, value: string) => {
    if (field === "service_name") {
      setService(value);
    }
  };

  return (
    <div className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Explorer</div>
          <h1>Logs</h1>
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
            <LoadingState>Loading logs…</LoadingState>
          ) : error ? (
            <div className="signal-empty">Error loading logs: {String(error)}</div>
          ) : data?.logs.length === 0 ? (
            <div className="signal-empty">No logs found.</div>
          ) : (
            <table>
              <thead>
                <tr>
                  <th>
                    Timestamp{" "}
                    <Button
                      type="button"
                      onClick={() => setUtc((v) => !v)}
                      aria-pressed={utc}
                      variant="secondary"
                      className={`ml-1.5 min-h-0 px-1.5 py-0 text-[11px] rounded-full align-middle ${utc ? "bg-[var(--brand)] text-white border-[var(--brand)]" : ""}`}
                    >
                      UTC
                    </Button>
                  </th>
                  <th>Service</th>
                  <th>Level</th>
                  <th>Message</th>
                </tr>
              </thead>
              <tbody>
                {data?.logs.map((log) => (
                  <LogRow key={log.log_id} log={log} utc={utc} />
                ))}
              </tbody>
            </table>
          )}
        </TablePanel>
      </div>
    </div>
  );
}

function LogRow({ log, utc }: { log: LogRecord; utc: boolean }) {
  const badges = infraLinks(log.resource_attributes ?? {});
  return (
    <tr>
      <td className="whitespace-nowrap">{formatTimestamp(log.timestamp_unix_nano, utc)}</td>
      <td>
        {log.service_name}
        {badges.length > 0 && (
          <span className="inline-flex gap-1 ml-1.5">
            {badges.map((link) => (
              <a
                key={link.href}
                href={link.href}
                className="text-[11px] px-1.5 rounded-full bg-[var(--surface-subtle)] text-[var(--text)] no-underline border border-[var(--border)] whitespace-nowrap"
              >
                {link.label}
              </a>
            ))}
          </span>
        )}
      </td>
      <td>
        <Badge tone={severityTone(log.severity_number)}>
          {log.severity_text || String(log.severity_number)}
        </Badge>
      </td>
      <td>{typeof log.body === "string" ? log.body : JSON.stringify(log.body)}</td>
    </tr>
  );
}

function severityTone(severity: number) {
  if (severity >= 17) return "bad";
  if (severity >= 13) return "warn";
  if (severity >= 9) return "info";
  return "good";
}
