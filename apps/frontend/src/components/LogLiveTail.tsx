import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { tailLogs } from "../api/logs";
import type { LogRecord } from "../api/logs";
import { Button } from "./ui/button";
import { formatLogMessage, getSeverityColor } from "../utils/logFormatting";
import { formatTimestamp } from "../utils/formatTimestamp";
import { useTimeDisplay } from "../lib/timeDisplay";
import { QueryFilterInput } from "../features/nlq/QueryFilterInput";
import { deriveViewFiltersFromIr, type NlqIrLike } from "../features/nlq/queryFilters";
import { useTenantContext } from "../hooks/useTenantContext";

const POLL_INTERVAL_MS = 1000;
const MAX_LOGS = 200;

const LIVE_LOGS_BASE_IR: NlqIrLike = {
  operation: "table",
  signals: ["logs"],
  filters: [],
  time_range: { from: "now-5m", to: "now" },
};

export function LogLiveTail() {
  const [service, setService] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [logs, setLogs] = useState<LogRecord[]>([]);
  const bottomRef = useRef<HTMLDivElement | null>(null);
  const { format } = useTimeDisplay();
  const { tenantId } = useTenantContext();

  const cursor = useMemo(() => latestTimestamp(logs), [logs]);
  const { error } = useQuery({
    queryKey: ["logs", "live-tail", tenantId, service || "all", cursor],
    queryFn: async () => {
      const result = await tailLogs(tenantId, {
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
    <section className="mt-8">
      <div className="flex items-center gap-3 flex-wrap">
        <h2 className="m-0 text-lg font-bold text-[var(--text-strong)]">Live Logs</h2>
        <Button
          type="button"
          variant="secondary"
          onClick={() => setEnabled((value) => !value)}
        >
          {enabled ? "Pause" : "Resume"}
        </Button>
        <QueryFilterInput
          baseIr={LIVE_LOGS_BASE_IR}
          placeholder='Filter live logs, e.g. "checkout service"'
          onIr={(ir) => {
            const filters = deriveViewFiltersFromIr(ir, "live-logs");
            setService(filters.service ?? filters.text ?? "");
          }}
        />
        <span aria-live="polite" className="text-sm text-[var(--muted)]">
          {enabled ? "Tailing every 1s" : "Paused"}
        </span>
      </div>
      {error && (
        <p className="mt-2 text-sm text-[var(--bad)]">Error: {String(error)}</p>
      )}
      <div
        aria-label="Live log stream"
        className="border border-[var(--border)] rounded font-mono text-xs h-80 mt-3 overflow-y-auto p-2 bg-[var(--surface)]"
      >
        {logs.length === 0 && (
          <p className="m-0 text-[var(--muted)]">No live logs yet.</p>
        )}
        {logs.map((log) => (
          <div
            key={log.log_id}
            className="border-b border-[var(--border)] grid gap-2 py-1 last:border-b-0"
            style={{ gridTemplateColumns: "100px 80px minmax(120px, 180px) 1fr" }}
          >
            <span className="text-[var(--muted)]">{formatTimestamp(log.timestamp_unix_nano, format)}</span>
            <span
              className="font-bold"
              style={{ color: getSeverityColor(log.severity_number) }}
            >
              {log.severity_text || `L${log.severity_number}`}
            </span>
            <span>{log.service_name || "unknown"}</span>
            <span>{formatLogMessage(log.body)}</span>
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
  const ts = logs[logs.length - 1]?.timestamp_unix_nano;
  return ts === undefined ? undefined : String(ts);
}

