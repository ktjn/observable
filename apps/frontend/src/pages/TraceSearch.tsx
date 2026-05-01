import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { createDashboard } from "../api/dashboards";
import { searchTraces, fetchTraceHistogram, TraceResponse, TraceHistogramBucket as ApiHistogramBucket, TraceHistogramResponse } from "../api/traces";
import { FacetSidebar } from "../components/FacetSidebar";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { LoadingState } from "../components/ui/loading-state";
import { Select, SelectOption } from "../components/ui/select";
import { TablePanel } from "../components/ui/table-panel";
import { Histogram, HistogramBucket } from "../components/ui/histogram";
import { useTimeDisplay } from "../lib/timeDisplay";
import { infraLinks } from "../utils/infraLinks";
import { formatContextValue } from "../utils/logFormatting";

const timeRangeOptions = [
  { label: "15m", value: 15 },
  { label: "1h", value: 60 },
  { label: "6h", value: 360 },
  { label: "24h", value: 1440 },
];

export default function TraceSearch() {
  const [service, setService] = useState(() => new URLSearchParams(window.location.search).get("service") ?? "");
  const { format } = useTimeDisplay();
  const [lookbackMinutes, setLookbackMinutes] = useState(60);
  const [selectedTraceId, setSelectedTraceId] = useState<string | undefined>();
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
    queryKey: ["traces", service, from, to],
    queryFn: () => searchTraces({
      service: service || undefined,
      from,
      to,
      limit: 50,
      facets: ["service_name", "status_code", "span_kind"]
    }),
  });

  const { data: histogramData, isError: isHistogramError } = useQuery({
    queryKey: ["traces-histogram", service, from, to, bucketCount],
    queryFn: () =>
      fetchTraceHistogram({
        service: service || undefined,
        from,
        to: new Date(histogramToMs).toISOString(),
        buckets: bucketCount,
      }),
    placeholderData: (prev: TraceHistogramResponse | undefined) => prev,
  });

  const traces = data?.traces ?? [];
  const selectedTrace = traces.find((t) => t.trace_id === selectedTraceId);
  const histogram = useMemo(
    () => {
      if (histogramData?.buckets.length) {
        return histogramFromApi(histogramData.buckets);
      }
      return buildTraceHistogram(traces, histogramFromMs, histogramToMs);
    },
    [histogramData, histogramFromMs, histogramToMs, traces],
  );
  const canRenderHistogram = Boolean(histogramData) || traces.length > 0;

  const handleFacetClick = (field: string, value: string) => {
    if (field === "service_name") {
      setService(value);
      setSelectedTraceId(undefined);
    }
  };

  function handleHistogramRangeSelect(fromMs: number, toMs: number) {
    setCustomRangeMs({ fromMs, toMs });
    setSelectedTraceId(undefined);
  }

  function handleClearRange() {
    setCustomRangeMs(null);
    setSelectedTraceId(undefined);
  }

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
            lookback_minutes: lookbackMinutes,
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
    <div className="page-stack">
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
        {customRangeMs ? (
          <>
            <span className="text-xs whitespace-nowrap font-mono text-[var(--text-strong)]">
              {formatBucketLabel(customRangeMs.fromMs, format)} – {formatBucketLabel(customRangeMs.toMs, format)}
            </span>
            <Button variant="secondary" onClick={handleClearRange}>
              Reset range
            </Button>
          </>
        ) : (
          <Select
            aria-label="Trace time range"
            className="max-w-[120px]"
            value={String(lookbackMinutes)}
            onChange={(event) => {
              setLookbackMinutes(Number(event.target.value));
              setSelectedTraceId(undefined);
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

      {canRenderHistogram ? (
        <Histogram
          buckets={histogram}
          categoryOrder={["Traces"]}
          categoryColors={{ Traces: "bg-[var(--brand)]" }}
          format={(ms) => formatBucketLabel(ms, format)}
          onRangeSelect={handleHistogramRangeSelect}
          onBucketCountChange={setBucketCount}
          ariaLabel="Trace volume histogram"
          title="Traces over time"
          subtitle="Volume"
        />
      ) : !isHistogramError && (
        <div
          aria-hidden="true"
          className="border border-[var(--border)] bg-[var(--surface)] p-3 h-[168px] animate-pulse"
        />
      )}
      {isHistogramError && !canRenderHistogram && (
        <p className="text-xs text-[var(--muted)]">Histogram unavailable</p>
      )}

      <div className="flex items-start gap-3 max-[900px]:flex-col">
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
          ) : traces.length === 0 ? (
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
                {traces.map((t) => (
                  <TraceRow
                    key={t.trace_id}
                    trace={t}
                    selected={selectedTraceId === t.trace_id}
                    onSelect={() => setSelectedTraceId(t.trace_id)}
                  />
                ))}
              </tbody>
            </table>
          )}
        </TablePanel>

        {selectedTrace && (
          <TraceContextSidebar trace={selectedTrace} onClose={() => setSelectedTraceId(undefined)} />
        )}
      </div>
    </div>
  );
}

function TraceRow({
  trace,
  selected,
  onSelect,
}: {
  trace: TraceResponse;
  selected: boolean;
  onSelect: () => void;
}) {
  const root = trace.spans[0];
  if (!root) return null;
  return (
    <tr className={`modern-table-row ${selected ? "bg-[var(--surface-subtle)]" : ""}`}>
      <td className="strong-cell">
        <button
          type="button"
          className="text-left text-[var(--brand)] bg-transparent border-0 p-0 font-inherit cursor-pointer hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)]"
          onClick={onSelect}
        >
          {trace.trace_id.substring(0, 16)}…
        </button>
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
}

function TraceContextSidebar({
  trace,
  onClose,
}: {
  trace: TraceResponse;
  onClose: () => void;
}) {
  const root = trace.spans[0];
  if (!root) return null;

  const badges = infraLinks(root.resource_attributes ?? {});

  return (
    <aside
      aria-label="Selected trace context"
      className="w-[320px] shrink-0 border border-[var(--border)] bg-[var(--surface)] p-4 max-[900px]:w-full"
    >
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Selected Trace</div>
          <h2 className="m-0 text-base font-bold text-[var(--text-strong)]">Root Span Details</h2>
        </div>
        <Button variant="secondary" className="min-h-8 px-2 text-xs" onClick={onClose}>
          Close
        </Button>
      </div>

      <div className="mb-4">
        <Link
          to="/traces/$traceId"
          params={{ traceId: trace.trace_id }}
          className="text-sm font-bold text-[var(--brand)] hover:underline"
        >
          View Full Trace Explorer
        </Link>
      </div>

      <Badge tone={root.status_code === "ERROR" ? "bad" : "good"} className="mb-3">
        {root.status_code}
      </Badge>

      <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">trace_id</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{trace.trace_id}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">service.name</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{root.service_name}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">operation</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{root.operation_name}</dd>
        </div>
        <div className="contents">
          <dt className="break-all font-bold text-[var(--muted)]">duration</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">{(root.duration_ns / 1e6).toFixed(2)}ms</dd>
        </div>
        {Object.entries(root.resource_attributes ?? {}).sort(([a], [b]) => a.localeCompare(b)).map(([key, value]) => (
          <div key={key} className="contents">
            <dt className="break-all font-bold text-[var(--muted)]">{key}</dt>
            <dd className="m-0 min-w-0 break-all text-[var(--text)]">{formatContextValue(value)}</dd>
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

function histogramFromApi(buckets: ApiHistogramBucket[]): HistogramBucket<"Traces">[] {
  return buckets.map((b) => ({
    startMs: b.start_ms,
    endMs: b.end_ms,
    total: b.count,
    categories: { Traces: b.count },
  }));
}

export function buildTraceHistogram(_traces: TraceResponse[], fromMs: number, toMs: number): HistogramBucket<"Traces">[] {
  const bucketCount = 30;
  const rangeMs = Math.max(1, toMs - fromMs);
  const bucketMs = rangeMs / bucketCount;
  const buckets = Array.from({ length: bucketCount }, (_, index) => ({
    startMs: fromMs + index * bucketMs,
    endMs: fromMs + (index + 1) * bucketMs,
    total: 0,
    categories: { Traces: 0 },
  }));

  for (const trace of _traces) {
    const root = trace.spans[0];
    if (!root) continue;
    const startMs = Number(root.start_time_unix_nano) / 1_000_000;
    if (!Number.isFinite(startMs)) continue;
    const rawIndex = Math.floor((startMs - fromMs) / bucketMs);
    const index = Math.min(bucketCount - 1, Math.max(0, rawIndex));
    buckets[index].total += 1;
    buckets[index].categories.Traces += 1;
  }

  return buckets;
}

function formatBucketLabel(ms: number, format: import("../lib/timeDisplay").TimeFormat): string {
  const utc = format === "iso-utc-ms" || format === "iso-utc-ns" || format === "unix-ms" || format === "unix-ns";
  return utc ? new Date(ms).toISOString() : new Date(ms).toLocaleTimeString();
}
