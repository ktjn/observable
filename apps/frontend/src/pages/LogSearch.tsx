import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createDashboard } from "../api/dashboards";
import {
  LogRecord,
  LogHistogramBucket as ApiHistogramBucket,
  LogHistogramResponse,
  fetchLogHistogram,
} from "../api/logs";
import { submitNlqQuery } from "../api/nlq";
import type { NlqIrLike } from "../features/nlq/queryFilters";
import { infraLinks } from "../utils/infraLinks";
import { formatTimestamp } from "../utils/formatTimestamp";
import { formatBucketLabel } from "../utils/formatBucketLabel";
import { OTelLevel, otelSeverity, formatLogMessage, formatContextValue } from "../utils/logFormatting";
import { useTimeDisplay } from "../lib/timeDisplay";
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";
import { useTenantContext } from "../hooks/useTenantContext";
import { liveViewQueryOptions } from "../hooks/useLiveRefresh";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/empty-state";
import { LoadingState } from "../components/ui/loading-state";
import { TablePanel } from "../components/ui/table-panel";
import { Histogram, HistogramBucket } from "../components/ui/histogram";
import { SignalExplorer, SaveStatus } from "../components/shared/SignalExplorer";
import { LogResultsTable } from "../features/signals/components/LogResultsTable";

const LOG_BASE_IR: NlqIrLike = {
  operation: "table",
  signals: ["logs"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
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
const ROW_LIMIT = 500;

type SeverityFilter = "all" | "error" | "warn" | "info" | "debug";

const SEVERITY_PILLS: { key: SeverityFilter; label: string; test: (sev: number) => boolean }[] = [
  { key: "all", label: "All", test: () => true },
  { key: "error", label: "Error", test: (sev) => sev >= 17 },
  { key: "warn", label: "Warn", test: (sev) => sev >= 13 && sev < 17 },
  { key: "info", label: "Info", test: (sev) => sev >= 9 && sev < 13 },
  { key: "debug", label: "Debug", test: (sev) => sev < 9 },
];

const SEVERITY_PILL_ACTIVE_COLOR: Record<SeverityFilter, string> = {
  all: "var(--brand)",
  error: "var(--bad)",
  warn: "var(--warn)",
  info: "var(--good)",
  debug: "var(--muted)",
};

export type LogExplorerProps = {
  initialService?: string;
  lockedService?: boolean;
  showHeader?: boolean;
  showServiceColumn?: boolean;
  showPromote?: boolean;
  tableAriaLabel?: string;
  /** When set, pre-filters logs to this service. */
  serviceName?: string;
};

export default function LogSearch() {
  return (
    <LogExplorer
      initialService={new URLSearchParams(window.location.search).get("service") ?? ""}
    />
  );
}

export function LogExplorer({
  initialService = "",
  lockedService = false,
  showHeader = true,
  showServiceColumn = true,
  showPromote = true,
  tableAriaLabel,
  serviceName,
}: LogExplorerProps) {
  const { format } = useTimeDisplay();
  const { fromMs, toMs, setCustomRange } = useGlobalDateRange();
  const { tenantId } = useTenantContext();

  // userQuery is the raw text (NLQ or raw IR JSON) submitted by the user.
  // When serviceName is provided, initialise with a pre-set service filter IR.
  const initialQuery = serviceName
    ? JSON.stringify({
        ...LOG_BASE_IR,
        filters: [{ field: "service_name", op: "=", value: serviceName }],
      })
    : null;

  const [userQuery, setUserQuery] = useState<string | null>(initialQuery);
  // Keep service for legacy clear-filter button visibility.
  const [service, setService] = useState(initialService);
  const [messageSearch, setMessageSearch] = useState("");
  const [severityFilter, setSeverityFilter] = useState<SeverityFilter>("all");

  const from = String(BigInt(Math.floor(fromMs)) * 1_000_000n);
  const to = String(BigInt(Math.floor(toMs)) * 1_000_000n);
  const [bucketCount, setBucketCount] = useState(60);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");

  const { data, isLoading, error } = useQuery({
    queryKey: ["logs", "nlq", tenantId, userQuery, fromMs, toMs],
    queryFn: async () => {
      const response = await submitNlqQuery(tenantId, {
        base_ir: { ...LOG_BASE_IR, time_range: { from, to } },
        question: userQuery ?? undefined,
        mode: "execute",
      });
      if (response.type !== "frame") return [];
      return response.frame.data as unknown as LogRecord[];
    },
    ...liveViewQueryOptions,
  });

  const { data: histogramData, isError: isHistogramError } = useQuery({
    queryKey: ["logs-histogram", tenantId, service, fromMs, toMs, bucketCount],
    queryFn: () =>
      fetchLogHistogram(tenantId, {
        service: service || undefined,
        from,
        to,
        buckets: bucketCount,
      }),
    placeholderData: (prev: LogHistogramResponse | undefined) => prev,
    ...liveViewQueryOptions,
  });

  const rawLogs = data ?? [];
  const logs = rawLogs.slice(0, ROW_LIMIT);
  const isCapped = rawLogs.length > ROW_LIMIT;

  // Apply message search first, then compute severity counts, then apply severity filter.
  const messageFilteredLogs = useMemo(() => {
    if (!messageSearch.trim()) return logs;
    const needle = messageSearch.toLowerCase();
    return logs.filter((l) => formatLogMessage(l.body).toLowerCase().includes(needle));
  }, [logs, messageSearch]);

  const severityCounts = useMemo(() => {
    const counts: Record<SeverityFilter, number> = { all: messageFilteredLogs.length, error: 0, warn: 0, info: 0, debug: 0 };
    for (const log of messageFilteredLogs) {
      for (const pill of SEVERITY_PILLS) {
        if (pill.key !== "all" && pill.test(log.severity_number)) {
          counts[pill.key]++;
          break;
        }
      }
    }
    return counts;
  }, [messageFilteredLogs]);

  const displayedLogs = useMemo(() => {
    if (severityFilter === "all") return messageFilteredLogs;
    const pill = SEVERITY_PILLS.find((p) => p.key === severityFilter);
    return pill ? messageFilteredLogs.filter((l) => pill.test(l.severity_number)) : messageFilteredLogs;
  }, [messageFilteredLogs, severityFilter]);

  const histogram = useMemo(
    () =>
      histogramData?.buckets
        ? histogramFromApi(histogramData.buckets)
        : buildLogHistogram([], fromMs, toMs),
    [histogramData, fromMs, toMs],
  );

  const handlePromote = async () => {
    setSaveStatus("saving");
    try {
      await createDashboard(tenantId, {
        name: service ? `Logs for ${service}` : "Promoted log query",
        panels: [
          {
            title: service ? `Logs for ${service}` : "Log search",
            query_kind: "logs",
            service: service || undefined,
            preset: null,
            filters: { facets: ["service_name", "severity_number", "environment", "host_id"] },
          },
        ],
      });
      setSaveStatus("saved");
    } catch {
      setSaveStatus("error");
    }
  };

  return (
    <SignalExplorer
      title="Logs"
      service={service}
      onServiceChange={(s) => {
        setService(s);
      }}
      lockedService={lockedService}
      showHeader={showHeader}
      showPromote={showPromote}
      baseIr={LOG_BASE_IR}
      onQuerySubmit={(text) => {
        setUserQuery(text || null);
        setService("");
      }}
      saveStatus={saveStatus}
      onPromote={handlePromote}
      histogram={
        histogramData ? (
          <Histogram
            buckets={histogram}
            categoryOrder={levelOrder}
            categoryColors={levelBarClasses}
            format={(ms) => formatBucketLabel(ms, format)}
            onRangeSelect={setCustomRange}
            onBucketCountChange={setBucketCount}
            ariaLabel="Log volume histogram"
            title="Logs over time"
            subtitle="Volume"
          />
        ) : !isHistogramError ? (
          <div
            aria-hidden="true"
            className="border border-[var(--border)] bg-[var(--surface)] p-3 h-[168px] animate-pulse"
          />
        ) : (
          <p className="text-xs text-[var(--muted)]">Histogram unavailable</p>
        )
      }
      renderTable={(selectedId, onSelect) => (
        <div className="flex flex-col flex-1 min-h-0 gap-2">
          {/* Severity pills + message search */}
          <div className="flex flex-wrap items-center gap-2">
            <div className="flex items-center gap-1 flex-wrap" aria-label="Filter by severity">
              {SEVERITY_PILLS.map((pill) => {
                const isActive = severityFilter === pill.key;
                const activeColor = SEVERITY_PILL_ACTIVE_COLOR[pill.key];
                return (
                  <button
                    key={pill.key}
                    type="button"
                    onClick={() => setSeverityFilter(pill.key)}
                    style={isActive ? { borderColor: activeColor, color: activeColor } : undefined}
                    className={[
                      "flex items-center gap-1 px-2.5 py-1 text-xs font-bold border transition-colors",
                      isActive
                        ? ""
                        : "border-[var(--border)] text-[var(--muted)] hover:border-[var(--brand)] hover:text-[var(--text)]",
                    ].join(" ")}
                  >
                    {pill.label}
                    <span aria-hidden="true" className="opacity-70">({severityCounts[pill.key]})</span>
                  </button>
                );
              })}
            </div>
            <input
              type="search"
              value={messageSearch}
              onChange={(e) => setMessageSearch(e.target.value)}
              placeholder="Search messages…"
              aria-label="Search log messages"
              className="min-w-[180px] flex-1 border border-[var(--border)] bg-[var(--surface)] px-2.5 py-1 text-xs text-[var(--text)] placeholder:text-[var(--muted)] focus:border-[var(--brand)] focus:outline-none"
            />
          </div>

          <TablePanel className="flex-1 min-h-0 flex flex-col">
            {isLoading ? (
              <LoadingState>Loading logs…</LoadingState>
            ) : error ? (
              <LoadingState className="text-[var(--bad)]">Error loading logs: {String(error)}</LoadingState>
            ) : displayedLogs.length === 0 ? (
              <EmptyState
                title="No logs found"
                description={
                  messageSearch || severityFilter !== "all"
                    ? "No logs match the current filters. Try clearing the search or selecting a different severity."
                    : "No logs in the selected time range. Try widening the time window or checking your service filter."
                }
              />
            ) : (
              <>
                <LogResultsTable
                  logs={displayedLogs}
                  selectedLogId={selectedId ?? undefined}
                  onSelectLog={(id) => onSelect(id)}
                  timeFormat={format}
                  showServiceColumn={showServiceColumn}
                  ariaLabel={tableAriaLabel}
                />
                {isCapped && (
                  <p className="px-3 py-2 text-xs text-[var(--muted)] border-t border-[var(--border)]">
                    Showing {ROW_LIMIT} results — narrow the time range or add filters to see fewer.
                  </p>
                )}
              </>
            )}
          </TablePanel>
        </div>
      )}
      renderPanel={(selectedId, onClose) => {
        const log = logs.find((l) => l.log_id === selectedId);
        return log ? <LogContextSidebar log={log} format={format} onClose={onClose} /> : null;
      }}
    />
  );
}

function LogContextSidebar({
  log,
  format,
  onClose,
}: {
  log: LogRecord;
  format: import("../lib/timeDisplay").TimeFormat;
  onClose: () => void;
}) {
  const severity = otelSeverity(log.severity_number);
  const entries = logContextEntries(log, format);
  const badges = infraLinks(log.resource_attributes ?? {});

  return (
    <aside
      aria-label="Selected log context"
      className="w-full h-full max-[900px]:max-h-[calc(100vh-200px)] overflow-y-auto border border-[var(--border)] bg-[var(--surface)] p-4"
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
            <dd className="m-0 min-w-0 break-all text-[var(--text)]">
              {key === "trace_id" && log.trace_id ? (
                <a
                  href={`/traces/${log.trace_id}`}
                  className="text-[var(--brand)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)]"
                >
                  {value}
                </a>
              ) : key === "span_id" && log.trace_id ? (
                <a
                  href={`/traces/${log.trace_id}`}
                  title="View parent trace"
                  className="text-[var(--brand)] hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)]"
                >
                  {value}
                </a>
              ) : (
                value
              )}
            </dd>
          </div>
        ))}
      </dl>
      {badges.length > 0 && (
        <div className="mt-4 flex flex-wrap gap-1.5">
          {badges.map((link) => (
            <a
              key={link.href}
              href={link.href}
              className="text-[11px] px-1.5 py-0.5 bg-[var(--surface-subtle)] text-[var(--text)] border border-[var(--border)] no-underline whitespace-nowrap hover:border-[var(--brand)] hover:text-[var(--brand)]"
            >
              {link.label}
            </a>
          ))}
        </div>
      )}
    </aside>
  );
}

function histogramFromApi(buckets: ApiHistogramBucket[]): HistogramBucket<OTelLevel>[] {
  return buckets.map((b) => {
    const categories = emptyLevels();
    let total = 0;
    for (const [sev, count] of Object.entries(b.counts)) {
      const level = otelSeverity(Number(sev)).label;
      categories[level] += count;
      total += count;
    }
    return { startMs: b.start_ms, endMs: b.end_ms, total, categories };
  });
}

export function buildLogHistogram(logs: LogRecord[], fromMs: number, toMs: number): HistogramBucket<OTelLevel>[] {
  const bucketCount = 30;
  const rangeMs = toMs - fromMs;
  const bucketMs = rangeMs / bucketCount;
  const buckets = Array.from({ length: bucketCount }, (_, i) => ({
    startMs: fromMs + i * bucketMs,
    endMs: fromMs + (i + 1) * bucketMs,
    total: 0,
    categories: emptyLevels(),
  }));
  for (const log of logs) {
    const ms = Number(log.timestamp_unix_nano) / 1_000_000;
    if (!Number.isFinite(ms)) continue;
    const idx = Math.min(bucketCount - 1, Math.max(0, Math.floor((ms - fromMs) / bucketMs)));
    const level = otelSeverity(log.severity_number).label;
    buckets[idx].total += 1;
    buckets[idx].categories[level] += 1;
  }
  return buckets;
}

function emptyLevels(): Record<OTelLevel, number> {
  return { TRACE: 0, DEBUG: 0, INFO: 0, WARN: 0, ERROR: 0, FATAL: 0 };
}

function logContextEntries(
  log: LogRecord,
  format: import("../lib/timeDisplay").TimeFormat,
): [string, string][] {
  const entries: [string, string][] = [
    ["time", formatTimestamp(log.timestamp_unix_nano, format)],
    ["service.name", log.service_name],
    ["severity_number", String(log.severity_number)],
    ["message", formatLogMessage(log.body)],
  ];
  if (log.observed_timestamp_unix_nano)
    entries.push(["observed_time", formatTimestamp(log.observed_timestamp_unix_nano, format)]);
  if (log.environment) entries.push(["environment", log.environment]);
  if (log.host_id) entries.push(["host_id", log.host_id]);
  if (log.trace_id) entries.push(["trace_id", log.trace_id]);
  if (log.span_id) entries.push(["span_id", log.span_id]);
  if (log.fingerprint !== null && log.fingerprint !== undefined)
    entries.push(["fingerprint", String(log.fingerprint)]);
  for (const [k, v] of Object.entries(log.attributes ?? {}).sort(([a], [b]) => a.localeCompare(b)))
    entries.push([`log.${k}`, formatContextValue(v)]);
  for (const [k, v] of Object.entries(log.resource_attributes ?? {}).sort(([a], [b]) => a.localeCompare(b)))
    entries.push([k, formatContextValue(v)]);
  return entries;
}

export { otelSeverity, formatLogMessage } from "../utils/logFormatting";
