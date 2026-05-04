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
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
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

  const from = String(BigInt(Math.floor(fromMs)) * 1_000_000n);
  const to = String(BigInt(Math.floor(toMs)) * 1_000_000n);
  const [bucketCount, setBucketCount] = useState(60);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");

  const { data, isLoading, error } = useQuery({
    queryKey: ["logs", "nlq", userQuery, fromMs, toMs],
    queryFn: async () => {
      const response = await submitNlqQuery({
        base_ir: { ...LOG_BASE_IR, time_range: { from, to } },
        question: userQuery ?? undefined,
        mode: "execute",
      });
      if (response.type !== "frame") return [];
      return response.frame.data as unknown as LogRecord[];
    },
  });

  const { data: histogramData, isError: isHistogramError } = useQuery({
    queryKey: ["logs-histogram", service, fromMs, toMs, bucketCount],
    queryFn: () =>
      fetchLogHistogram({
        service: service || undefined,
        from,
        to,
        buckets: bucketCount,
      }),
    placeholderData: (prev: LogHistogramResponse | undefined) => prev,
  });

  const rawLogs = data ?? [];
  const logs = rawLogs.slice(0, ROW_LIMIT);
  const isCapped = rawLogs.length >= ROW_LIMIT;
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
      await createDashboard({
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
        <TablePanel className="flex-1">
          {isLoading ? (
            <LoadingState>Loading logs…</LoadingState>
          ) : error ? (
            <LoadingState className="text-[var(--bad)]">Error loading logs: {String(error)}</LoadingState>
          ) : logs.length === 0 ? (
            <LoadingState>No logs found.</LoadingState>
          ) : (
            <>
              <LogResultsTable
                logs={logs}
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
      className="w-full border border-[var(--border)] bg-[var(--surface)] p-4"
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
