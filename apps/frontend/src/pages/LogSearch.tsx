import { useEffect, useMemo, useRef, useState } from "react";
import { useGlobalServiceFilter } from "../hooks/useGlobalServiceFilter";
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
import { OTelLevel, otelSeverity, severityTextClass, formatLogMessage, formatContextValue } from "../utils/logFormatting";
import { useTimeDisplay } from "../lib/timeDisplay";
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";
import { useTenantContext } from "../hooks/useTenantContext";
import { liveViewQueryOptions } from "../hooks/useLiveRefresh";
import { useLiveTail } from "../hooks/useLiveTail";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/empty-state";
import { ErrorState } from "../components/ui/error-state";
import { LoadingState } from "../components/ui/loading-state";
import { PillFilter } from "../components/ui/pill-filter";
import { TablePanel } from "../components/ui/table-panel";
import { Histogram, HistogramBucket } from "../components/ui/histogram";
import { CopyButton } from "../components/ui/copy-button";
import { DlRow } from "../components/ui/dl-row";
import { SignalExplorer, SaveStatus } from "../components/shared/SignalExplorer";
import { LogResultsTable } from "../features/signals/components/LogResultsTable";
import { SavedViewsControl } from "../features/signals/components/SavedViewsControl";
import { ColumnPickerControl } from "../features/signals/components/ColumnPickerControl";
import type { LogViewConfig } from "../api/savedViews";
import { MetricCard } from "../components/ui/metric-card";

const LOG_BASE_IR: NlqIrLike = {
  operation: "table",
  signals: ["logs"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};

const levelOrder: OTelLevel[] = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR", "FATAL"];
const levelBarClasses: Record<OTelLevel, string> = {
  TRACE: "fill-[var(--muted)]",
  DEBUG: "fill-[var(--brand)]",
  INFO: "fill-[var(--good)]",
  WARN: "fill-[var(--warn)]",
  ERROR: "fill-[var(--bad)]",
  FATAL: "fill-[var(--bad)]",
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
  const { service } = useGlobalServiceFilter();
  return (
    <LogExplorer
      initialService={service ?? ""}
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
  const { preset, fromMs, toMs, setPreset, setCustomRange } = useGlobalDateRange();
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
  const [isLive, setIsLive] = useState(false);
  const [visibleColumns, setVisibleColumns] = useState<("level" | "service")[]>(["level", "service"]);
  const [isRegexMode, setIsRegexMode] = useState(false);

  const SEVERITY_MIN: Partial<Record<SeverityFilter, number>> = {
    error: 17,
    warn: 13,
    info: 9,
  };

  const liveTail = useLiveTail({
    tenantId,
    service: service || undefined,
    severityMin: SEVERITY_MIN[severityFilter],
    enabled: isLive,
  });

  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const userScrolledUp = useRef(false);

  useEffect(() => {
    const el = scrollContainerRef.current;
    if (!el) return;
    const onScroll = () => {
      const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 50;
      userScrolledUp.current = !atBottom;
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => el.removeEventListener("scroll", onScroll);
  }, []);

  useEffect(() => {
    if (!isLive || userScrolledUp.current) return;
    const el = scrollContainerRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [liveTail.logs.length, isLive]);

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

  const rawLogs = isLive ? liveTail.logs : (data ?? []);
  const logs = isLive ? liveTail.logs : rawLogs.slice(0, ROW_LIMIT);
  const isCapped = !isLive && rawLogs.length > ROW_LIMIT;

  const regexPattern = useMemo(() => {
    if (!isRegexMode || !messageSearch.trim()) return null;
    try {
      return new RegExp(messageSearch, "i");
    } catch {
      return undefined; // undefined marks an invalid pattern, distinct from null (no pattern requested)
    }
  }, [isRegexMode, messageSearch]);

  const isRegexInvalid = isRegexMode && messageSearch.trim() !== "" && regexPattern === undefined;

  // Apply message search first, then compute severity counts, then apply severity filter.
  const messageFilteredLogs = useMemo(() => {
    if (!messageSearch.trim()) return logs;
    if (isRegexMode) {
      if (!regexPattern) return logs; // invalid pattern: show everything rather than nothing
      return logs.filter((l) => regexPattern.test(formatLogMessage(l.body)));
    }
    const needle = messageSearch.toLowerCase();
    return logs.filter((l) => formatLogMessage(l.body).toLowerCase().includes(needle));
  }, [logs, messageSearch, isRegexMode, regexPattern]);

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
      histogramData?.buckets?.length
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

  const currentViewConfig: LogViewConfig = {
    query: userQuery,
    severity_filter: severityFilter,
    message_search: messageSearch,
    time_range:
      preset != null
        ? { mode: "preset", preset }
        : { mode: "absolute", from_ms: fromMs, to_ms: toMs },
    visible_columns: visibleColumns,
  };

  const handleLoadView = (config: LogViewConfig) => {
    setUserQuery(config.query);
    setService("");
    setSeverityFilter(config.severity_filter as SeverityFilter);
    setMessageSearch(config.message_search);
    if (config.time_range.mode === "preset") {
      setPreset(config.time_range.preset as Parameters<typeof setPreset>[0]);
    } else {
      setCustomRange(config.time_range.from_ms, config.time_range.to_ms);
    }
    setVisibleColumns(config.visible_columns.filter((c): c is "level" | "service" => c === "level" || c === "service"));
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
      savedViewsControl={
        <>
          <ColumnPickerControl visibleColumns={visibleColumns} onChange={setVisibleColumns} />
          <SavedViewsControl tenantId={tenantId} currentConfig={currentViewConfig} onLoad={handleLoadView} />
        </>
      }
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
          <LoadingState variant="skeleton" className="h-[168px]" />
        ) : (
          <p className="text-xs text-[var(--muted)]">Histogram unavailable</p>
        )
      }
      renderTable={(selectedId, onSelect) => (
        <div className="flex flex-col flex-1 min-h-0 gap-2">
          <div
            className="grid gap-3"
            style={{ gridTemplateColumns: "repeat(4, minmax(100px, 1fr))" }}
            aria-label="Log summary"
          >
            <MetricCard label="Total Logs" value={String(severityCounts.all)} tone="info" />
            <MetricCard label="Errors" value={String(severityCounts.error)} tone={severityCounts.error > 0 ? "bad" : "good"} />
            <MetricCard label="Warnings" value={String(severityCounts.warn)} tone={severityCounts.warn > 0 ? "warn" : "good"} />
            <MetricCard label="Info" value={String(severityCounts.info)} tone="info" />
          </div>

          {/* Severity pills + message search */}
          <div className="flex flex-wrap items-center gap-2">
            <PillFilter
              pills={SEVERITY_PILLS.map((pill) => ({
                key: pill.key,
                label: pill.label,
                count: severityCounts[pill.key],
                activeColor: SEVERITY_PILL_ACTIVE_COLOR[pill.key],
              }))}
              activeKey={severityFilter}
              onSelect={(key) => setSeverityFilter(key as SeverityFilter)}
              ariaLabel="Filter by severity"
            />
            <input
              type="search"
              value={messageSearch}
              onChange={(e) => setMessageSearch(e.target.value)}
              placeholder="Quick filter — plain text or regex"
              aria-label="Search log messages"
              className="min-w-[180px] flex-1 border border-[var(--border)] bg-[var(--surface)] px-2.5 py-1 text-xs text-[var(--text)] placeholder:text-[var(--muted)] focus:border-[var(--brand)] focus:outline-none"
            />
            <button
              type="button"
              onClick={() => setIsRegexMode((v) => !v)}
              aria-pressed={isRegexMode}
              aria-label={isRegexMode ? "Disable regex quick filter" : "Enable regex quick filter"}
              title="Toggle regex matching for the quick filter"
              className={[
                "px-2 py-1 text-xs font-mono font-bold border transition-colors",
                isRegexMode
                  ? "border-[var(--brand)] text-[var(--brand)]"
                  : "border-[var(--border)] text-[var(--muted)] hover:border-[var(--brand)] hover:text-[var(--text)]",
              ].join(" ")}
            >
              .*
            </button>
            <button
              type="button"
              onClick={() => setIsLive((v) => !v)}
              className={[
                "flex items-center gap-1.5 px-2.5 py-1 text-xs font-bold border transition-colors",
                isLive
                  ? "border-[var(--bad)] text-[var(--bad)]"
                  : "border-[var(--border)] text-[var(--muted)] hover:border-[var(--brand)] hover:text-[var(--text)]",
              ].join(" ")}
              aria-pressed={isLive}
              aria-label={isLive ? "Stop tail mode" : "Start tail mode"}
            >
              {isLive && (
                <span
                  className="inline-block w-1.5 h-1.5 rounded-full bg-[var(--bad)] animate-pulse"
                  aria-hidden="true"
                />
              )}
              {isLive ? "Stop" : "Tail"}
            </button>
          </div>

          {isLive && userQuery?.trim() && (
            <p className="text-[10px] text-[var(--warn)] px-1">
              NLQ query not applied in tail mode — service and severity filters are active.
            </p>
          )}
          {isRegexInvalid && (
            <p className="text-[10px] text-[var(--warn)] px-1">
              Invalid regex — showing all results.
            </p>
          )}

          <TablePanel className="flex-1 min-h-0 flex flex-col">
            <div
              ref={scrollContainerRef}
              className="flex-1 overflow-y-auto min-h-0"
            >
              {!isLive && isLoading ? (
                <LoadingState>Loading logs…</LoadingState>
              ) : isLive && liveTail.error ? (
                <ErrorState title="Live tail error" description={String(liveTail.error)} />
              ) : !isLive && error ? (
                <ErrorState title="Failed to load logs" description={String(error)} />
              ) : displayedLogs.length === 0 ? (
                <EmptyState
                  title={isLive ? "Waiting for logs…" : "No logs found"}
                  description={
                    isLive
                      ? "No log entries since live tail started. New logs will appear automatically."
                      : messageSearch || severityFilter !== "all"
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
                    visibleColumns={visibleColumns}
                    ariaLabel={tableAriaLabel}
                  />
                  {isCapped && (
                    <p className="px-3 py-2 text-xs text-[var(--muted)] border-t border-[var(--border)]">
                      Showing {ROW_LIMIT} results — narrow the time range or add filters to see fewer.
                    </p>
                  )}
                  {isLive && liveTail.logs.length >= 500 && (
                    <p className="px-3 py-2 text-xs text-[var(--muted)] border-t border-[var(--border)]">
                      Showing last 500 live log rows.
                    </p>
                  )}
                </>
              )}
            </div>
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
      className="w-full max-h-[calc(100vh-120px)] overflow-y-auto border border-[var(--border)] bg-[var(--surface)] p-4"
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
      <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
        <DlRow label="level">
          <span className={`font-bold uppercase ${severityTextClass(log.severity_number)}`}>
            {severity.label}
          </span>
        </DlRow>
        {entries.map(([key, value]) => (
          <DlRow key={key} label={key}>
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
            <CopyButton value={value} label={`Copy ${key}`} />
          </DlRow>
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
