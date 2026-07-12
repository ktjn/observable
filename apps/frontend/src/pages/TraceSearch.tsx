import { useMemo, useState } from "react";
import { useGlobalServiceFilter } from "../hooks/useGlobalServiceFilter";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { createDashboard } from "../api/dashboards";
import {
  fetchTraceHistogram,
  Span,
  TraceResponse,
  TraceHistogramBucket as ApiHistogramBucket,
  TraceHistogramResponse,
} from "../api/traces";
import { submitNlqQuery } from "../api/nlq";
import type { NlqIrLike } from "../features/nlq/queryFilters";
import { FacetSidebar } from "../components/FacetSidebar";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { EmptyState } from "../components/ui/empty-state";
import { ErrorState } from "../components/ui/error-state";
import { LoadingState } from "../components/ui/loading-state";
import { PillFilter } from "../components/ui/pill-filter";
import { TablePanel } from "../components/ui/table-panel";
import { Histogram, HistogramBucket } from "../components/ui/histogram";
import { DlRow } from "../components/ui/dl-row";
import { useTimeDisplay } from "../lib/timeDisplay";
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";
import { useTenantContext } from "../hooks/useTenantContext";
import { liveViewQueryOptions } from "../hooks/useLiveRefresh";
import { formatBucketLabel } from "../utils/formatBucketLabel";
import { infraLinks } from "../utils/infraLinks";
import { DEFAULT_TRACE_COLUMNS, FIXED_TRACE_KEYS, traceContextEntries } from "../utils/traceContext";
import { SignalExplorer, SaveStatus } from "../components/shared/SignalExplorer";
import { TraceResultsTable, type TraceTableColumn } from "../features/signals/components/TraceResultsTable";
import { ColumnPickerControl } from "../features/signals/components/ColumnPickerControl";
import { MetricCard } from "../components/ui/metric-card";

const TRACE_BASE_IR: NlqIrLike = {
  operation: "table",
  signals: ["traces"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};
const ROW_LIMIT = 500;

type StatusFilter = "all" | "ok" | "error";

const STATUS_PILLS: { key: StatusFilter; label: string; test: (status: string) => boolean }[] = [
  { key: "all", label: "All", test: () => true },
  { key: "error", label: "Error", test: (s) => s === "ERROR" },
  { key: "ok", label: "OK", test: (s) => s !== "ERROR" },
];

const STATUS_PILL_ACTIVE_COLOR: Record<StatusFilter, string> = {
  all: "var(--brand)",
  error: "var(--bad)",
  ok: "var(--good)",
};

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
        // This synthetic root span has no real span kind; "INTERNAL" is an inert
        // default since the UI doesn't read span_kind for this synthetic span.
        span_kind: "INTERNAL",
        start_time_unix_nano: Number(row.start_time_unix_nano),
        end_time_unix_nano: Number(row.start_time_unix_nano) + row.duration_ms * 1_000_000,
        duration_ns: row.duration_ms * 1_000_000,
        // status_code comes from the same ClickHouse status_code enum column as
        // Span.status_code, so it's always one of the three variants.
        status_code: row.status_code as Span["status_code"],
        status_message: "",
        attributes: {},
        resource_attributes: {},
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
  const { service } = useGlobalServiceFilter();
  return (
    <TraceExplorer
      initialService={service ?? ""}
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
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [visibleColumns, setVisibleColumns] = useState<TraceTableColumn[]>(() =>
    showServiceColumn
      ? [...DEFAULT_TRACE_COLUMNS]
      : DEFAULT_TRACE_COLUMNS.filter((key) => key !== "service.name"),
  );
  const toggleTraceColumn = (key: string) => {
    setVisibleColumns((current) =>
      current.includes(key) ? current.filter((column) => column !== key) : [...current, key],
    );
  };
  const pickerColumns = useMemo(() => {
    const keys: string[] = [...FIXED_TRACE_KEYS];
    for (const key of visibleColumns) if (!keys.includes(key)) keys.push(key);
    return keys.map((key) => ({ key, label: key }));
  }, [visibleColumns]);

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

  const statusCounts = useMemo(() => {
    const counts: Record<StatusFilter, number> = { all: traces.length, ok: 0, error: 0 };
    for (const t of traces) {
      const root = t.spans[0];
      if (!root) continue;
      if (root.status_code === "ERROR") counts.error++;
      else counts.ok++;
    }
    return counts;
  }, [traces]);

  const displayedTraces = useMemo(() => {
    if (statusFilter === "all") return traces;
    const pill = STATUS_PILLS.find((p) => p.key === statusFilter);
    return pill ? traces.filter((t) => {
      const root = t.spans[0];
      return root ? pill.test(root.status_code) : false;
    }) : traces;
  }, [traces, statusFilter]);

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
      savedViewsControl={
        <ColumnPickerControl
          columns={pickerColumns}
          visibleColumns={visibleColumns}
          onChange={setVisibleColumns}
        />
      }
      histogram={
        canRenderHistogram ? (
          <Histogram
            buckets={histogram}
            categoryOrder={["Traces"]}
            categoryColors={{ Traces: "fill-[var(--brand)]" }}
            format={(ms) => formatBucketLabel(ms, format)}
            onRangeSelect={setCustomRange}
            onBucketCountChange={setBucketCount}
            ariaLabel="Trace volume histogram"
            title="Traces over time"
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
          {showFacets && (
            <FacetSidebar
              facets={undefined}
              onFacetClick={() => {}}
              ariaLabel="Trace facets"
            />
          )}

          <div
            className="grid gap-3"
            style={{ gridTemplateColumns: "repeat(4, minmax(100px, 1fr))" }}
            aria-label="Trace summary"
          >
            <MetricCard label="Total Traces" value={String(statusCounts.all)} tone="info" />
            <MetricCard label="OK" value={String(statusCounts.ok)} tone="good" />
            <MetricCard label="Errors" value={String(statusCounts.error)} tone={statusCounts.error > 0 ? "bad" : "good"} />
            <MetricCard
              label="Error Rate"
              value={statusCounts.all > 0 ? `${((statusCounts.error / statusCounts.all) * 100).toFixed(1)}%` : "—"}
              tone={statusCounts.all > 0 && statusCounts.error / statusCounts.all >= 0.05 ? "bad" : statusCounts.all > 0 && statusCounts.error / statusCounts.all >= 0.01 ? "warn" : "good"}
            />
          </div>

          {/* Status pills */}
          <PillFilter
            pills={STATUS_PILLS.map((pill) => ({
              key: pill.key,
              label: pill.label,
              count: statusCounts[pill.key],
              activeColor: STATUS_PILL_ACTIVE_COLOR[pill.key],
            }))}
            activeKey={statusFilter}
            onSelect={(key) => setStatusFilter(key as StatusFilter)}
            ariaLabel="Filter by status"
          />

          <TablePanel className="flex-1 min-h-0 flex flex-col">
            {isLoading ? (
              <LoadingState>Loading traces…</LoadingState>
            ) : error ? (
              <ErrorState title="Failed to load traces" description={String(error)} />
            ) : displayedTraces.length === 0 ? (
              <EmptyState
                title="No traces found"
                description={
                  statusFilter !== "all"
                    ? "No traces match the current status filter. Try selecting a different status."
                    : "No traces in the selected time range. Try widening the time window or checking your service filter."
                }
              />
            ) : (
              <>
                <TraceResultsTable
                  traces={displayedTraces}
                  selectedTraceId={selectedId ?? undefined}
                  onSelectTrace={(id) => onSelect(id)}
                  mode={tableMode}
                  showServiceColumn={showServiceColumn}
                  visibleColumns={visibleColumns}
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
        </div>
      )}
      renderPanel={(selectedId, onClose) => {
        const trace = traces.find((t) => t.trace_id === selectedId);
        return trace ? (
          <TraceContextSidebar trace={trace} onClose={onClose} visibleColumns={visibleColumns} onToggleColumn={toggleTraceColumn} />
        ) : null;
      }}
    />
  );
}

export function TraceContextSidebar({
  trace,
  onClose,
  visibleColumns,
  onToggleColumn,
}: {
  trace: TraceResponse;
  onClose: () => void;
  visibleColumns: readonly string[];
  onToggleColumn: (key: string) => void;
}) {
  const root = trace.spans[0];
  if (!root) return null;

  const { format } = useTimeDisplay();
  const badges = infraLinks(root.resource_attributes ?? {});
  const entries = traceContextEntries(trace, format);

  return (
    <aside
      aria-label="Selected trace context"
      className="w-full h-full max-[900px]:max-h-[calc(100vh-200px)] overflow-y-auto border border-[var(--border)] bg-[var(--surface)] p-4"
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

      <dl className="grid grid-cols-[minmax(88px,45%)_1fr] gap-x-3 gap-y-2 text-xs">
        {entries.map(([key, value]) => (
          <DlRow key={key} label={key} copyValue={key === "start_time" || key === "duration" ? undefined : value}
            onToggleColumn={() => onToggleColumn(key)} columnVisible={visibleColumns.includes(key)}>
            {key === "status" ? <Badge tone={root.status_code === "ERROR" ? "bad" : "good"}>{value}</Badge> : value}
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
