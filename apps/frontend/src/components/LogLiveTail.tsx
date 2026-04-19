import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { tailLogs } from "../api/logs";
import type { LogRecord } from "../api/logs";

const POLL_INTERVAL_MS = 1000;
const MAX_LOGS = 200;

export function LogLiveTail() {
  const [service, setService] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [logs, setLogs] = useState<LogRecord[]>([]);
  const bottomRef = useRef<HTMLDivElement | null>(null);

  const cursor = useMemo(() => latestTimestamp(logs), [logs]);
  const { error } = useQuery({
    queryKey: ["logs", "live-tail", service || "all", cursor],
    queryFn: async () => {
      const result = await tailLogs({
        service: service || undefined,
        since_unix_nano: cursor,
        limit: 100,
      });
      if (result.logs.length > 0) {
        setLogs((current) => mergeLogs(current, result.logs).slice(-MAX_LOGS));
      }
      return result;
    },
    enabled,
    refetchInterval: enabled ? POLL_INTERVAL_MS : false,
  });

  useEffect(() => {
    setLogs([]);
  }, [service]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView?.({ block: "end" });
  }, [logs]);

  return (
    <section style={{ marginTop: "2rem" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "0.75rem", flexWrap: "wrap" }}>
        <h2 style={{ margin: 0 }}>Live Logs</h2>
        <button
          type="button"
          onClick={() => setEnabled((value) => !value)}
          style={{ borderRadius: 4, padding: "0.4rem 0.6rem" }}
        >
          {enabled ? "Pause" : "Resume"}
        </button>
        <input
          aria-label="Live log service filter"
          placeholder="Filter by service"
          value={service}
          onChange={(event) => setService(event.target.value)}
          style={{ padding: "0.5rem", width: "260px" }}
        />
        <span aria-live="polite">{enabled ? "Tailing every 1s" : "Paused"}</span>
      </div>
      {error && <p>Error: {String(error)}</p>}
      <div
        aria-label="Live log stream"
        style={{
          border: "1px solid #cbd5e1",
          borderRadius: 4,
          fontFamily: "monospace",
          fontSize: 12,
          height: 320,
          marginTop: "0.75rem",
          overflowY: "auto",
          padding: 8,
        }}
      >
        {logs.length === 0 && <p>No live logs yet.</p>}
        {logs.map((log) => (
          <div
            key={log.log_id}
            style={{
              borderBottom: "1px solid #e2e8f0",
              display: "grid",
              gap: 8,
              gridTemplateColumns: "100px 80px minmax(120px, 180px) 1fr",
              padding: "4px 0",
            }}
          >
            <span style={{ color: "#475569" }}>{formatTime(log.timestamp_unix_nano)}</span>
            <span style={{ color: severityColor(log.severity_number), fontWeight: 700 }}>
              {log.severity_text || `L${log.severity_number}`}
            </span>
            <span>{log.service_name || "unknown"}</span>
            <span>{typeof log.body === "string" ? log.body : JSON.stringify(log.body)}</span>
          </div>
        ))}
        <div ref={bottomRef} />
      </div>
    </section>
  );
}

export function mergeLogs(current: LogRecord[], incoming: LogRecord[]): LogRecord[] {
  const seen = new Set(current.map((log) => log.log_id));
  const next = [...current];

  for (const log of incoming) {
    if (!seen.has(log.log_id)) {
      next.push(log);
      seen.add(log.log_id);
    }
  }

  return next.sort((a, b) => Number(a.timestamp_unix_nano) - Number(b.timestamp_unix_nano));
}

function latestTimestamp(logs: LogRecord[]): string | undefined {
  return logs[logs.length - 1]?.timestamp_unix_nano;
}

function formatTime(timestampUnixNano: string): string {
  return new Date(Number(timestampUnixNano) / 1e6).toISOString().split("T")[1].replace("Z", "");
}

function severityColor(severity: number): string {
  if (severity >= 13) return "#dc2626";
  if (severity >= 9) return "#ca8a04";
  if (severity >= 5) return "#2563eb";
  return "#475569";
}
