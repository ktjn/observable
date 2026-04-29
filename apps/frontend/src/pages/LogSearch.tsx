import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createDashboard } from "../api/dashboards";
import { searchLogs, LogRecord } from "../api/logs";
import { infraLinks } from "../utils/infraLinks";
import { formatTimestamp } from "../utils/formatTimestamp";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { LoadingState } from "../components/ui/loading-state";
import { Select, SelectOption } from "../components/ui/select";
import { TablePanel } from "../components/ui/table-panel";

const timeRangeOptions = [
  { label: "15m", value: 15 },
  { label: "1h", value: 60 },
  { label: "6h", value: 360 },
  { label: "24h", value: 1440 },
];

type OTelLevel = "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR" | "FATAL";

type HistogramBucket = {
  startMs: number;
  endMs: number;
  total: number;
  levels: Record<OTelLevel, number>;
};

const levelOrder: OTelLevel[] = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR", "FATAL"];
const levelBarClasses: Record<OTelLevel, string> = {
  TRACE: "bg-[var(--muted)]",
  DEBUG: "bg-[var(--brand)]",
  INFO: "bg-[var(--good)]",
  WARN: "bg-[var(--warn)]",
  ERROR: "bg-[var(--bad)]",
  FATAL: "bg-[var(--bad)]",
};

export default function LogSearch() {
  const [service, setService] = useState(() => new URLSearchParams(window.location.search).get("service") ?? "");
  const [utc, setUtc] = useState(false);
  const [lookbackMinutes, setLookbackMinutes] = useState(60);
  const [selectedLogId, setSelectedLogId] = useState<string | undefined>();
  const [saveStatus, setSaveStatus] = useState<"idle" | "saving" | "saved" | "error">("idle");

  const from = useMemo(() => {
    const d = new Date();
    d.setMinutes(d.getMinutes() - lookbackMinutes);
    return d.toISOString();
  }, [lookbackMinutes]);

  const { data, isLoading, error } = useQuery({
    queryKey: ["logs", service, from],
    queryFn: () =>
      searchLogs({
        service: service || undefined,
        from,
        limit: 50,
        facets: ["service_name", "severity_number", "environment", "host_id"],
      }),
  });

  const logs = data?.logs ?? [];
  const selectedLog = logs.find((log) => log.log_id === selectedLogId);
  const histogram = useMemo(() => buildLogHistogram(logs, lookbackMinutes), [logs, lookbackMinutes]);

  const handlePromote = async () => {
    setSaveStatus("saving");
    try {
      await createDashboard({
        name: service ? `Logs for ${service}` : "Promoted log query",
        panels: [
          {
            title: service ? `Logs for ${service}` : "Log search",
            query_kind: "logs",
            service: service || undefined,
            lookback_minutes: lookbackMinutes,
            filters: { facets: ["service_name", "severity_number", "environment", "host_id"] },
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
        <Select
          aria-label="Log time range"
          className="max-w-[120px]"
          value={String(lookbackMinutes)}
          onChange={(event) => {
            setLookbackMinutes(Number(event.target.value));
            setSelectedLogId(undefined);
          }}
        >
          {timeRangeOptions.map((option) => (
            <SelectOption key={option.value} value={option.value}>
              {option.label}
            </SelectOption>
          ))}
        </Select>
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

      {!isLoading && !error && logs.length > 0 && (
        <LogHistogram buckets={histogram} utc={utc} />
      )}

      <div className="flex items-start gap-3 max-[900px]:flex-col">
        {selectedLog && (
          <LogContextSidebar log={selectedLog} utc={utc} onClose={() => setSelectedLogId(undefined)} />
        )}

        <TablePanel className="flex-1">
          {isLoading ? (
            <LoadingState>Loading logs…</LoadingState>
          ) : error ? (
            <LoadingState className="text-[var(--bad)]">Error loading logs: {String(error)}</LoadingState>
          ) : logs.length === 0 ? (
            <LoadingState>No logs found.</LoadingState>
          ) : (
            <table aria-label="Log results">
              <thead>
                <tr>
                  <th aria-label="Time">
                    Time{" "}
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
                  <th>Level</th>
                  <th>Message</th>
                </tr>
              </thead>
              <tbody>
                {logs.map((log) => (
                  <LogRow
                    key={log.log_id}
                    log={log}
                    utc={utc}
                    selected={selectedLogId === log.log_id}
                    onSelect={() => setSelectedLogId(log.log_id)}
                  />
                ))}
              </tbody>
            </table>
          )}
        </TablePanel>
      </div>
    </div>
  );
}

function LogRow({
  log,
  utc,
  selected,
  onSelect,
}: {
  log: LogRecord;
  utc: boolean;
  selected: boolean;
  onSelect: () => void;
}) {
  const severity = otelSeverity(log.severity_number);
  const message = formatLogMessage(log.body);
  return (
    <tr className={`modern-table-row ${selected ? "bg-[var(--surface-subtle)]" : ""}`}>
      <td className="whitespace-nowrap">{formatTimestamp(log.timestamp_unix_nano, utc)}</td>
      <td>
        <Badge tone={severity.tone}>
          {severity.label}
        </Badge>
      </td>
      <td>
        <button
          type="button"
          className="w-full text-left text-[var(--text)] bg-transparent border-0 p-0 font-inherit cursor-pointer hover:text-[var(--brand-strong)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--surface)]"
          aria-label={`Open log context for ${message}`}
          onClick={onSelect}
        >
          {message}
        </button>
      </td>
    </tr>
  );
}

function LogHistogram({ buckets, utc }: { buckets: HistogramBucket[]; utc: boolean }) {
  const max = Math.max(1, ...buckets.map((bucket) => bucket.total));

  return (
    <section
      role="img"
      aria-label="Log volume histogram"
      className="rounded-lg border border-[var(--border)] bg-[var(--surface)] p-3"
    >
      <div className="mb-3 flex items-center justify-between gap-3">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Volume</div>
          <h2 className="m-0 text-sm font-bold text-[var(--text-strong)]">Logs over time</h2>
        </div>
        <div className="flex flex-wrap gap-2 text-xs text-[var(--muted)]">
          {levelOrder.map((level) => (
            <span key={level} className="inline-flex items-center gap-1">
              <span className={`h-2 w-2 rounded-full ${levelBarClasses[level]}`} />
              {level}
            </span>
          ))}
        </div>
      </div>
      <div className="grid h-28 grid-cols-12 items-end gap-1" aria-hidden="true">
        {buckets.map((bucket) => (
          <div key={bucket.startMs} className="flex h-full flex-col justify-end gap-px rounded-sm bg-[var(--surface-inset)]">
            {levelOrder.map((level) => {
              const count = bucket.levels[level];
              if (count === 0) return null;
              return (
                <div
                  key={level}
                  className={levelBarClasses[level]}
                  title={`${formatBucketLabel(bucket.startMs, utc)} ${level}: ${count}`}
                  style={{ height: `${Math.max(8, (count / max) * 100)}%` }}
                />
              );
            })}
          </div>
        ))}
      </div>
    </section>
  );
}

function LogContextSidebar({
  log,
  utc,
  onClose,
}: {
  log: LogRecord;
  utc: boolean;
  onClose: () => void;
}) {
  const severity = otelSeverity(log.severity_number);
  const entries = logContextEntries(log, utc);
  const badges = infraLinks(log.resource_attributes ?? {});

  return (
    <aside
      aria-label="Selected log context"
      className="w-[320px] shrink-0 rounded-lg border border-[var(--border)] bg-[var(--surface)] p-4 max-[900px]:w-full"
    >
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Selected Log</div>
          <h2 className="m-0 text-base font-bold text-[var(--text-strong)]">Context Properties</h2>
        </div>
        <Button variant="secondary" className="min-h-8 px-2 text-xs" onClick={onClose}>
          Close
        </Button>
      </div>
      <Badge tone={severity.tone} className="mb-3">
        {severity.label}
      </Badge>
      <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
        {entries.map(([key, value]) => (
          <div key={key} className="contents">
            <dt className="break-all font-bold text-[var(--muted)]">{key}</dt>
            <dd className="m-0 min-w-0 break-all text-[var(--text)]">{value}</dd>
          </div>
        ))}
      </dl>
      {badges.length > 0 && (
        <div className="mt-4 flex flex-wrap gap-1.5">
          {badges.map((link) => (
            <a
              key={link.href}
              href={link.href}
              className="text-[11px] px-1.5 py-0.5 rounded-full bg-[var(--surface-subtle)] text-[var(--text)] border border-[var(--border)] no-underline whitespace-nowrap hover:border-[var(--brand)] hover:text-[var(--brand)]"
            >
              {link.label}
            </a>
          ))}
        </div>
      )}
    </aside>
  );
}

export function otelSeverity(severity: number): { label: OTelLevel; tone: "good" | "warn" | "bad" | "info" | "neutral" } {
  if (severity >= 21) return { label: "FATAL", tone: "bad" };
  if (severity >= 17) return { label: "ERROR", tone: "bad" };
  if (severity >= 13) return { label: "WARN", tone: "warn" };
  if (severity >= 9) return { label: "INFO", tone: "good" };
  if (severity >= 5) return { label: "DEBUG", tone: "info" };
  return { label: "TRACE", tone: "neutral" };
}

export function formatLogMessage(body: unknown): string {
  if (typeof body === "string") return body;
  if (typeof body === "number" || typeof body === "boolean") return String(body);
  if (!body || typeof body !== "object" || Array.isArray(body)) return String(body ?? "");

  const record = body as Record<string, unknown>;
  const message = record.message ?? record.msg ?? record.body;
  if (typeof message === "string") return message;

  return Object.entries(record)
    .map(([key, value]) => `${key}=${formatContextValue(value)}`)
    .join(" ");
}

export function buildLogHistogram(logs: LogRecord[], lookbackMinutes: number): HistogramBucket[] {
  const bucketCount = 12;
  const latestMs = Math.max(Date.now(), ...logs.map((log) => Number(log.timestamp_unix_nano) / 1_000_000));
  const rangeMs = lookbackMinutes * 60 * 1000;
  const startMs = latestMs - rangeMs;
  const bucketMs = rangeMs / bucketCount;
  const buckets = Array.from({ length: bucketCount }, (_, index) => ({
    startMs: startMs + index * bucketMs,
    endMs: startMs + (index + 1) * bucketMs,
    total: 0,
    levels: emptyLevels(),
  }));

  for (const log of logs) {
    const timestampMs = Number(log.timestamp_unix_nano) / 1_000_000;
    if (!Number.isFinite(timestampMs)) continue;
    const rawIndex = Math.floor((timestampMs - startMs) / bucketMs);
    const index = Math.min(bucketCount - 1, Math.max(0, rawIndex));
    const level = otelSeverity(log.severity_number).label;
    buckets[index].total += 1;
    buckets[index].levels[level] += 1;
  }

  return buckets;
}

function emptyLevels(): Record<OTelLevel, number> {
  return {
    TRACE: 0,
    DEBUG: 0,
    INFO: 0,
    WARN: 0,
    ERROR: 0,
    FATAL: 0,
  };
}

function logContextEntries(log: LogRecord, utc: boolean): [string, string][] {
  const entries: [string, string][] = [
    ["time", formatTimestamp(log.timestamp_unix_nano, utc)],
    ["service.name", log.service_name],
    ["severity_number", String(log.severity_number)],
    ["message", formatLogMessage(log.body)],
  ];

  if (log.observed_timestamp_unix_nano) {
    entries.push(["observed_time", formatTimestamp(log.observed_timestamp_unix_nano, utc)]);
  }
  if (log.environment) entries.push(["environment", log.environment]);
  if (log.host_id) entries.push(["host_id", log.host_id]);
  if (log.trace_id) entries.push(["trace_id", log.trace_id]);
  if (log.span_id) entries.push(["span_id", log.span_id]);
  if (log.fingerprint !== null && log.fingerprint !== undefined) {
    entries.push(["fingerprint", String(log.fingerprint)]);
  }

  for (const [key, value] of Object.entries(log.attributes ?? {}).sort(([a], [b]) => a.localeCompare(b))) {
    entries.push([`log.${key}`, formatContextValue(value)]);
  }

  for (const [key, value] of Object.entries(log.resource_attributes ?? {}).sort(([a], [b]) => a.localeCompare(b))) {
    entries.push([key, formatContextValue(value)]);
  }

  return entries;
}

function formatContextValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (value === null || value === undefined) return "";
  return JSON.stringify(value);
}

function formatBucketLabel(ms: number, utc: boolean): string {
  return utc ? new Date(ms).toISOString() : new Date(ms).toLocaleTimeString();
}
