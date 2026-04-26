import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { searchLogs, LogRecord } from "../api/logs";
import { FacetSidebar } from "../components/FacetSidebar";
import { infraLinks } from "../utils/infraLinks";
import { formatTimestamp } from "../utils/formatTimestamp";

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
        {service && (
          <button
            className="secondary-link"
            onClick={() => setService("")}
            style={{ cursor: "pointer", background: "none" }}
          >
            Clear filters
          </button>
        )}
      </div>

      <div style={{ display: "flex", alignItems: "flex-start" }}>
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
                  <th>
                    Timestamp{" "}
                    <button
                      type="button"
                      onClick={() => setUtc((v) => !v)}
                      aria-pressed={utc}
                      style={{
                        marginLeft: 6,
                        fontSize: 11,
                        padding: "1px 6px",
                        borderRadius: 10,
                        border: "1px solid currentColor",
                        background: utc ? "var(--color-accent, #3182ce)" : "transparent",
                        color: utc ? "#fff" : "inherit",
                        cursor: "pointer",
                        verticalAlign: "middle",
                      }}
                    >
                      UTC
                    </button>
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
        </div>
      </div>
    </section>
  );
}

function LogRow({ log, utc }: { log: LogRecord; utc: boolean }) {
  const badges = infraLinks(log.resource_attributes ?? {});
  return (
    <tr>
      <td style={{ whiteSpace: "nowrap" }}>{formatTimestamp(log.timestamp_unix_nano, utc)}</td>
      <td>
        {log.service_name}
        {badges.length > 0 && (
          <span style={{ display: "inline-flex", gap: 4, marginLeft: 6 }}>
            {badges.map((link) => (
              <a
                key={link.href}
                href={link.href}
                style={{
                  fontSize: 11,
                  padding: "1px 6px",
                  borderRadius: 10,
                  background: "var(--color-bg-subtle, #edf2f7)",
                  color: "var(--color-text, #2d3748)",
                  textDecoration: "none",
                  border: "1px solid var(--color-border, #e2e8f0)",
                  whiteSpace: "nowrap",
                }}
              >
                {link.label}
              </a>
            ))}
          </span>
        )}
      </td>
      <td>
        <span className={`status ${severityTone(log.severity_number)}`}>
          {log.severity_text || log.severity_number}
        </span>
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
