import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { createDashboard } from "../api/dashboards";
import { searchLogs, fetchLogHistogram, LogRecord, LogHistogramBucket as ApiHistogramBucket, LogHistogramResponse } from "../api/logs";
import { infraLinks } from "../utils/infraLinks";
import { formatTimestamp } from "../utils/formatTimestamp";
import { OTelLevel, otelSeverity, formatLogMessage, formatContextValue } from "../utils/logFormatting";
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
  const [customRangeMs, setCustomRangeMs] = useState<{ fromMs: number; toMs: number } | null>(null);
  const [bucketCount, setBucketCount] = useState(60);

  const { from, to, histogramFromMs, histogramToMs } = useMemo(() => {
    if (customRangeMs) {
      return {
        from: new Date(customRangeMs.fromMs).toISOString(),
        to: new Date(customRangeMs.toMs).toISOString(),
        histogramFromMs: customRangeMs.fromMs,
        histogramToMs: customRangeMs.toMs,
      };
    }
    const toMs = Date.now();
    const fromMs = toMs - lookbackMinutes * 60 * 1000;
    return {
      from: new Date(fromMs).toISOString(),
      to: undefined as string | undefined,
      histogramFromMs: fromMs,
      histogramToMs: toMs,
    };
  }, [customRangeMs, lookbackMinutes]);

  const { data, isLoading, error } = useQuery({
    queryKey: ["logs", service, from, to],
    queryFn: () =>
      searchLogs({
        service: service || undefined,
        from,
        to,
        limit: 50,
        facets: ["service_name", "severity_number", "environment", "host_id"],
      }),
  });

  const { data: histogramData, isError: isHistogramError } = useQuery({
    queryKey: ["logs-histogram", service, from, to, bucketCount],
    queryFn: () =>
      fetchLogHistogram({
        service: service || undefined,
        from,
        to: new Date(histogramToMs).toISOString(),
        buckets: bucketCount,
      }),
    placeholderData: (prev: LogHistogramResponse | undefined) => prev,
  });

  const logs = data?.logs ?? [];
  const selectedLog = logs.find((log) => log.log_id === selectedLogId);
  const histogram = useMemo(
    () => histogramData ? histogramFromApi(histogramData.buckets) : buildLogHistogram([], histogramFromMs, histogramToMs),
    [histogramData, histogramFromMs, histogramToMs],
  );

  function handleHistogramRangeSelect(fromMs: number, toMs: number) {
    setCustomRangeMs({ fromMs, toMs });
    setSelectedLogId(undefined);
  }

  function handleClearRange() {
    setCustomRangeMs(null);
    setSelectedLogId(undefined);
  }

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
        {customRangeMs ? (
          <>
            <span className="text-xs whitespace-nowrap font-mono text-[var(--text-strong)]">
              {formatBucketLabel(customRangeMs.fromMs, utc)} – {formatBucketLabel(customRangeMs.toMs, utc)}
            </span>
            <Button variant="secondary" onClick={handleClearRange}>
              Reset range
            </Button>
          </>
        ) : (
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
        )}
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

      {histogramData && (
        <LogHistogram
          buckets={histogram}
          utc={utc}
          onRangeSelect={handleHistogramRangeSelect}
          onBucketCountChange={setBucketCount}
        />
      )}
      {isHistogramError && (
        <p className="text-xs text-[var(--muted)]">Histogram unavailable</p>
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

function LogHistogram({
  buckets,
  utc,
  onRangeSelect,
  onBucketCountChange,
}: {
  buckets: HistogramBucket[];
  utc: boolean;
  onRangeSelect?: (fromMs: number, toMs: number) => void;
  onBucketCountChange?: (count: number) => void;
}) {
  const max = Math.max(1, ...buckets.map((bucket) => bucket.total));
  const gridRef = useRef<HTMLDivElement>(null);
  const sectionRef = useRef<HTMLElement>(null);

  const onBucketCountChangeRef = useRef(onBucketCountChange);
  useEffect(() => { onBucketCountChangeRef.current = onBucketCountChange; });

  useEffect(() => {
    const el = sectionRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const w = entry.contentRect.width;
      // Steps of 5 to avoid burst API calls on continuous resize
      const count = Math.round(Math.floor(w / 10) / 5) * 5;
      onBucketCountChangeRef.current?.(Math.max(12, Math.min(100, count)));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []); // stable: uses ref to read latest callback
  // ref holds current drag coords for synchronous reads in event handlers
  const dragRef = useRef<{ start: number; end: number } | null>(null);
  // state drives the visual highlight (re-renders on move)
  const [dragDisplay, setDragDisplay] = useState<{ start: number; end: number } | null>(null);

  const selStart = dragDisplay ? Math.min(dragDisplay.start, dragDisplay.end) : -1;
  const selEnd = dragDisplay ? Math.max(dragDisplay.start, dragDisplay.end) : -1;

  function getBucketIndex(clientX: number): number {
    const el = gridRef.current;
    if (!el) return 0;
    const rect = el.getBoundingClientRect();
    const ratio = (clientX - rect.left) / rect.width;
    return Math.min(buckets.length - 1, Math.max(0, Math.floor(ratio * buckets.length)));
  }

  function handlePointerDown(e: React.PointerEvent<HTMLDivElement>) {
    if (!onRangeSelect) return;
    try { e.currentTarget.setPointerCapture(e.pointerId); } catch { /* jsdom */ }
    const idx = getBucketIndex(e.clientX);
    dragRef.current = { start: idx, end: idx };
    setDragDisplay({ start: idx, end: idx });
  }

  function handlePointerMove(e: React.PointerEvent<HTMLDivElement>) {
    if (!dragRef.current) return;
    const idx = getBucketIndex(e.clientX);
    dragRef.current = { ...dragRef.current, end: idx };
    setDragDisplay({ ...dragRef.current });
  }

  function handlePointerUp() {
    const drag = dragRef.current;
    if (drag && onRangeSelect) {
      const start = Math.min(drag.start, drag.end);
      const end = Math.max(drag.start, drag.end);
      onRangeSelect(buckets[start].startMs, buckets[end].endMs);
    }
    dragRef.current = null;
    setDragDisplay(null);
  }

  return (
    <section
      ref={sectionRef}
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
      <p className="sr-only">Drag over bars to zoom into a time range.</p>
      <div
        ref={gridRef}
        className="grid h-28 items-end gap-1 select-none cursor-crosshair"
        style={{ gridTemplateColumns: `repeat(${buckets.length}, 1fr)` }}
        aria-hidden="true"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={() => { dragRef.current = null; setDragDisplay(null); }}
      >
        {buckets.map((bucket, i) => {
          const isSelected = dragDisplay !== null && i >= selStart && i <= selEnd;
          return (
            <div
              key={bucket.startMs}
              className={`flex h-full flex-col justify-end gap-px rounded-sm ${isSelected ? "bg-[var(--surface-subtle)]" : "bg-[var(--surface-inset)]"}`}
            >
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
          );
        })}
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

function histogramFromApi(buckets: ApiHistogramBucket[]): HistogramBucket[] {
  return buckets.map((b) => {
    const levels = emptyLevels();
    let total = 0;
    for (const [sev, count] of Object.entries(b.counts)) {
      const level = otelSeverity(Number(sev)).label;
      levels[level] += count;
      total += count;
    }
    return { startMs: b.start_ms, endMs: b.end_ms, total, levels };
  });
}

export function buildLogHistogram(logs: LogRecord[], fromMs: number, toMs: number): HistogramBucket[] {
  const bucketCount = 30;
  const rangeMs = toMs - fromMs;
  const bucketMs = rangeMs / bucketCount;
  const buckets = Array.from({ length: bucketCount }, (_, index) => ({
    startMs: fromMs + index * bucketMs,
    endMs: fromMs + (index + 1) * bucketMs,
    total: 0,
    levels: emptyLevels(),
  }));

  for (const log of logs) {
    const timestampMs = Number(log.timestamp_unix_nano) / 1_000_000;
    if (!Number.isFinite(timestampMs)) continue;
    const rawIndex = Math.floor((timestampMs - fromMs) / bucketMs);
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

function formatBucketLabel(ms: number, utc: boolean): string {
  return utc ? new Date(ms).toISOString() : new Date(ms).toLocaleTimeString();
}

export { otelSeverity, formatLogMessage } from "../utils/logFormatting";
