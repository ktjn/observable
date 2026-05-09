import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { createDashboard } from "../api/dashboards";
import {
  fetchTraceHistogram,
  TraceResponse,
  TraceHistogramBucket as ApiHistogramBucket,
  TraceHistogramResponse,
} from "../api/traces";
import { submitNlqQuery } from "../api/nlq";
import type { NlqIrLike } from "../features/nlq/queryFilters";
import { FacetSidebar } from "../components/FacetSidebar";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { LoadingState } from "../components/ui/loading-state";
import { TablePanel } from "../components/ui/table-panel";
import { Histogram, HistogramBucket } from "../components/ui/histogram";
import { useTimeDisplay } from "../lib/timeDisplay";
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";
import { useTenantContext } from "../hooks/useTenantContext";
import { liveViewQueryOptions } from "../hooks/useLiveRefresh";
import { formatBucketLabel } from "../utils/formatBucketLabel";
import { formatTimestamp } from "../utils/formatTimestamp";
import { formatContextValue } from "../utils/logFormatting";
import { infraLinks } from "../utils/infraLinks";
import { SignalExplorer, SaveStatus } from "../components/shared/SignalExplorer";
import { TraceResultsTable } from "../features/signals/components/TraceResultsTable";

const TRACE_BASE_IR: NlqIrLike = {
  operation: "table",
  signals: ["traces"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};
const ROW_LIMIT = 500;

/** Shape of a row returned by the NLQ trace execute query. */
interface NlqTraceRow {
  trace_id: string;
  root_service: string;
  root_operation: string;
  duration_ms: number;
  status_code: string;
  environment?: string;
  start_time_unix_nano: number | string;
}

/** Maps a flat NLQ trace row to the TraceResponse shape used by TraceResultsTable. */
function nlqRowToTraceResponse(row: NlqTraceRow): TraceResponse {
  return {
    trace_id: row.trace_id,
    spans: [
      {
        tenant_id: "",
        trace_id: row.trace_id,
        span_id: "",
        service_name: row.root_service,
        service_namespace: "",
        service_version: "",
        operation_name: row.root_operation,
        span_kind: "",
        start_time_unix_nano: Number(row.start_time_unix_nano),
        end_time_unix_nano: Number(row.start_time_unix_nano) + row.duration_ms * 1_000_000,
        duration_ns: row.duration_ms * 1_000_000,
        status_code: row.status_code,
        status_message: "",
        environment: row.environment ?? "",
        host_id: "",
        workload: "",
        deployment_id: "",
      },
    ],
    events: [],
  };
}

export type TraceExplorerProps = {
  initialService?: string;
  lockedService?: boolean;
  showHeader?: boolean;
  showServiceColumn?: boolean;
  showPromote?: boolean;
  showFacets?: boolean;
  tableAriaLabel?: string;
  tableMode?: "select" | "link";
  /** When set, pre-filters traces to this service. */
  serviceName?: string;
};

export default function TraceSearch() {
  return (
    <TraceExplorer
      initialService={new URLSearchParams(window.location.search).get("service") ?? ""}
    />
  );
}

export function TraceExplorer({
  initialService = "",
  lockedService = false,
  showHeader = true,
  showServiceColumn = true,
  showPromote = true,
  showFacets = false,
  tableAriaLabel,
  tableMode = "select",
  serviceName,
}: TraceExplorerProps) {
  const { format } = useTimeDisplay();
  const { fromMs, toMs, setCustomRange } = useGlobalDateRange();
  const { tenantId } = useTenantContext();

  const initialQuery = serviceName
    ? JSON.stringify({
        ...TRACE_BASE_IR,
        filters: [{ field: "service_name", op: "=", value: serviceName }],
      })
    : null;

  const [userQuery, setUserQuery] = useState<string | null>(initialQuery);
  const [service, setService] = useState(initialService);

  const from = String(BigInt(Math.floor(fromMs)) * 1_000_000n);
  const to = String(BigInt(Math.floor(toMs)) * 1_000_000n);
  const [bucketCount, setBucketCount] = useState(60);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>("idle");

  const { data, isLoading, error } = useQuery({
    queryKey: ["traces", "nlq", tenantId, userQuery, fromMs, toMs],
    queryFn: async () => {
      const response = await submitNlqQuery(tenantId, {
        base_ir: { ...TRACE_BASE_IR, time_range: { from, to } },
        question: userQuery ?? undefined,
        mode: "execute",
      });
      if (response.type !== "frame") return [];
      return (response.frame.data as unknown as NlqTraceRow[]).map(nlqRowToTraceResponse);
    },
    ...liveViewQueryOptions,
  });

  const { data: histogramData, isError: isHistogramError } = useQuery({
    queryKey: ["traces-histogram", tenantId, service, fromMs, toMs, bucketCount],
    queryFn: () =>
      fetchTraceHistogram(tenantId, {
        service: service || undefined,
        from,
        to,
        buckets: bucketCount,
      }),
    placeholderData: (prev: TraceHistogramResponse | undefined) => prev,
    ...liveViewQueryOptions,
  });

  const rawTraces = data ?? [];
  const traces = rawTraces.slice(0, ROW_LIMIT);
  const isCapped = rawTraces.length > ROW_LIMIT;
  const canRenderHistogram = Boolean(histogramData) || traces.length > 0;
  const histogram = useMemo(
    () =>
      histogramData?.buckets?.length
        ? histogramFromApi(histogramData.buckets)
        : buildTraceHistogram(traces, fromMs, toMs),
    [histogramData, fromMs, toMs, traces],
  );

  const handlePromote = async () => {
    setSaveStatus("saving");
    try {
      await createDashboard(tenantId, {
        name: service ? `Traces for ${service}` : "Promoted trace query",
        panels: [
          {
            title: service ? `Traces for ${service}` : "Trace search",
            query_kind: "traces",
            service: service || undefined,
            preset: null,
            filters: { facets: ["service_name", "status_code", "span_kind"] },
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
      title="Traces"
      service={service}
      onServiceChange={(s) => {
        setService(s);
      }}
      lockedService={lockedService}
      showHeader={showHeader}
      showPromote={showPromote}
      baseIr={TRACE_BASE_IR}
      onQuerySubmit={(text) => {
        setUserQuery(text || null);
        setService("");
      }}
      saveStatus={saveStatus}
      onPromote={handlePromote}
      histogram={
        canRenderHistogram ? (
          <Histogram
            buckets={histogram}
            categoryOrder={["Traces"]}
            categoryColors={{ Traces: "bg-[var(--brand)]" }}
            format={(ms) => formatBucketLabel(ms, format)}
            onRangeSelect={setCustomRange}
            onBucketCountChange={setBucketCount}
            ariaLabel="Trace volume histogram"
            title="Traces over time"
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
        <>
          {showFacets && (
            <FacetSidebar
              facets={undefined}
              onFacetClick={() => {}}
              ariaLabel="Trace facets"
            />
          )}
          <TablePanel className="flex-1 min-h-0 flex flex-col">
            {isLoading ? (
              <LoadingState>Loading traces…</LoadingState>
            ) : error ? (
              <LoadingState className="text-[var(--bad)]">Error loading traces: {String(error)}</LoadingState>
            ) : traces.length === 0 ? (
              <LoadingState>No traces found.</LoadingState>
            ) : (
              <>
                <TraceResultsTable
                  traces={traces}
                  selectedTraceId={selectedId ?? undefined}
                  onSelectTrace={(id) => onSelect(id)}
                  mode={tableMode}
                  showServiceColumn={showServiceColumn}
                  timeFormat={format}
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
        </>
      )}
      renderPanel={(selectedId, onClose) => {
        const trace = traces.find((t) => t.trace_id === selectedId);
        return trace ? <TraceContextSidebar trace={trace} onClose={onClose} /> : null;
      }}
    />
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

  const { format } = useTimeDisplay();
  const badges = infraLinks(root.resource_attributes ?? {});

  return (
    <aside
      aria-label="Selected trace context"
      className="w-full border border-[var(--border)] bg-[var(--surface)] p-4"
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
          <dt className="break-all font-bold text-[var(--muted)]">start_time</dt>
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">
            {formatTimestamp(root.start_time_unix_nano, format)}
          </dd>
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
          <dd className="m-0 min-w-0 break-all text-[var(--text)]">
            {(root.duration_ns / 1e6).toFixed(2)}ms
          </dd>
        </div>
        {Object.entries(root.resource_attributes ?? {})
          .sort(([a], [b]) => a.localeCompare(b))
          .map(([key, value]) => (
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

export function buildTraceHistogram(
  _traces: TraceResponse[],
  fromMs: number,
  toMs: number,
): HistogramBucket<"Traces">[] {
  const bucketCount = 30;
  const rangeMs = Math.max(1, toMs - fromMs);
  const bucketMs = rangeMs / bucketCount;
  const buckets = Array.from({ length: bucketCount }, (_, i) => ({
    startMs: fromMs + i * bucketMs,
    endMs: fromMs + (i + 1) * bucketMs,
    total: 0,
    categories: { Traces: 0 },
  }));
  for (const trace of _traces) {
    const root = trace.spans[0];
    if (!root) continue;
    const startMs = Number(root.start_time_unix_nano) / 1_000_000;
    if (!Number.isFinite(startMs)) continue;
    const idx = Math.min(bucketCount - 1, Math.max(0, Math.floor((startMs - fromMs) / bucketMs)));
    buckets[idx].total += 1;
    buckets[idx].categories.Traces += 1;
  }
  return buckets;
}
