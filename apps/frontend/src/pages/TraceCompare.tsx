import { useEffect, useMemo, useState, type FormEvent } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { getTrace, type Span, type TraceResponse } from "../api/traces";
import { useTenantContext } from "../hooks/useTenantContext";
import { useTimeDisplay, type TimeFormat } from "../lib/timeDisplay";
import { formatTimestamp } from "../utils/formatTimestamp";
import { formatStatusLabel } from "../utils/traceStatus";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/empty-state";
import { Input } from "../components/ui/input";
import { LoadingState } from "../components/ui/loading-state";
import { MetricCard } from "../components/ui/metric-card";
import { Panel } from "../components/ui/panel";
import { CopyableText } from "../components/ui/copy-button";
import { DlRow } from "../components/ui/dl-row";

export interface TraceCompareProps {
  initialLeftTraceId?: string;
  initialRightTraceId?: string;
}

export interface TraceSummary {
  traceId: string;
  spanCount: number;
  errorCount: number;
  durationMs: number;
  rootService: string;
  rootOperation: string;
  statusCode: string;
  startTimeNs: number;
  path: string[];
}

export interface TracePathDiff {
  shared: string[];
  leftOnly: string[];
  rightOnly: string[];
}

function sortedSpans(trace: TraceResponse): Span[] {
  return [...trace.spans].sort((a, b) =>
    Number(a.start_time_unix_nano) - Number(b.start_time_unix_nano) ||
    Number(a.end_time_unix_nano) - Number(b.end_time_unix_nano)
  );
}

function spanSignature(span: Span): string {
  return `${span.service_name} · ${span.operation_name}`;
}

export function collectTracePath(trace: TraceResponse): string[] {
  const seen = new Set<string>();
  const path: string[] = [];
  for (const span of sortedSpans(trace)) {
    const signature = spanSignature(span);
    if (!seen.has(signature)) {
      seen.add(signature);
      path.push(signature);
    }
  }
  return path;
}

export function summarizeTrace(trace: TraceResponse): TraceSummary {
  const spans = sortedSpans(trace);
  const root = spans[0] ?? trace.spans[0];
  const durationNs = root ? Number(root.end_time_unix_nano) - Number(root.start_time_unix_nano) : 0;
  return {
    traceId: trace.trace_id,
    spanCount: trace.spans.length,
    errorCount: trace.spans.filter((span) => span.status_code === "ERROR").length,
    durationMs: durationNs / 1e6,
    rootService: root?.service_name ?? "Unknown service",
    rootOperation: root?.operation_name ?? "Unknown operation",
    statusCode: root ? formatStatusLabel(root.status_code) : "UNKNOWN",
    startTimeNs: root ? Number(root.start_time_unix_nano) : 0,
    path: collectTracePath(trace),
  };
}

export function compareTracePaths(left: TraceResponse, right: TraceResponse): TracePathDiff {
  const leftPath = collectTracePath(left);
  const rightPath = collectTracePath(right);
  const rightSet = new Set(rightPath);
  const leftSet = new Set(leftPath);
  return {
    shared: leftPath.filter((item) => rightSet.has(item)),
    leftOnly: leftPath.filter((item) => !rightSet.has(item)),
    rightOnly: rightPath.filter((item) => !leftSet.has(item)),
  };
}

function formatSignedDuration(deltaMs: number): string {
  const sign = deltaMs > 0 ? "+" : "";
  return `${sign}${deltaMs.toFixed(2)}ms`;
}

function compareStatus(left: string, right: string): string {
  return left === right ? left : `${left} -> ${right}`;
}

function diffListLabel(items: string[]): string {
  return items.length === 0 ? "None" : items.slice(0, 4).join(", ");
}

export default function TraceCompare({
  initialLeftTraceId = "",
  initialRightTraceId = "",
}: TraceCompareProps) {
  const { tenantId } = useTenantContext();
  const navigate = useNavigate();
  const { format } = useTimeDisplay();
  const [leftTraceId, setLeftTraceId] = useState(initialLeftTraceId);
  const [rightTraceId, setRightTraceId] = useState(initialRightTraceId);

  useEffect(() => {
    setLeftTraceId(initialLeftTraceId);
    setRightTraceId(initialRightTraceId);
  }, [initialLeftTraceId, initialRightTraceId]);

  const leftId = leftTraceId.trim();
  const rightId = rightTraceId.trim();

  const leftQuery = useQuery({
    queryKey: ["trace-compare", tenantId, "left", leftId],
    queryFn: () => getTrace(tenantId, leftId),
    enabled: Boolean(leftId),
  });
  const rightQuery = useQuery({
    queryKey: ["trace-compare", tenantId, "right", rightId],
    queryFn: () => getTrace(tenantId, rightId),
    enabled: Boolean(rightId),
  });

  const leftTrace = leftQuery.data;
  const rightTrace = rightQuery.data;
  const leftSummary = useMemo(() => (leftTrace ? summarizeTrace(leftTrace) : null), [leftTrace]);
  const rightSummary = useMemo(() => (rightTrace ? summarizeTrace(rightTrace) : null), [rightTrace]);
  const comparison = useMemo(
    () => (leftTrace && rightTrace ? compareTracePaths(leftTrace, rightTrace) : null),
    [leftTrace, rightTrace],
  );

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    void navigate({
      to: "/traces/compare",
      search: {
        left: leftId || undefined,
        right: rightId || undefined,
      },
    });
  };

  const handleSwap = () => {
    setLeftTraceId(rightTraceId);
    setRightTraceId(leftTraceId);
    void navigate({
      to: "/traces/compare",
      search: {
        left: rightId || undefined,
        right: leftId || undefined,
      },
    });
  };

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Traces</div>
          <h1>Trace comparison</h1>
        </div>
        <Link to="/traces" className="secondary-link">
          Back to traces
        </Link>
      </div>

      <Panel eyebrow="Compare" title="Choose two traces">
        <form onSubmit={handleSubmit} className="grid gap-3 lg:grid-cols-[1fr_1fr_auto]">
          <label className="flex flex-col gap-1">
            <span className="field-label">Left trace</span>
            <Input
              value={leftTraceId}
              onChange={(event) => setLeftTraceId(event.target.value)}
              placeholder="trace id"
              aria-label="Left trace id"
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="field-label">Right trace</span>
            <Input
              value={rightTraceId}
              onChange={(event) => setRightTraceId(event.target.value)}
              placeholder="trace id"
              aria-label="Right trace id"
            />
          </label>
          <div className="flex items-end gap-2">
            <Button type="submit" disabled={!leftId || !rightId}>
              Compare
            </Button>
            <Button type="button" variant="secondary" onClick={handleSwap} disabled={!leftId && !rightId}>
              Swap
            </Button>
          </div>
        </form>
      </Panel>

      {!leftId || !rightId ? (
        <EmptyState
          title="Enter two trace IDs"
          description="Paste a baseline trace on the left and the candidate trace on the right to see the diff."
        />
      ) : leftQuery.isLoading || rightQuery.isLoading ? (
        <LoadingState>Loading trace comparison…</LoadingState>
      ) : leftQuery.error || rightQuery.error ? (
        <EmptyState
          title="Trace comparison unavailable"
          description="One of the traces could not be loaded. Check both trace IDs and try again."
        />
      ) : leftTrace && rightTrace ? (
        <>
          <Panel eyebrow="Delta" title="Comparison summary">
            <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
              <MetricCard
                label="Duration delta"
                value={formatSignedDuration(
                  rightSummary && leftSummary ? rightSummary.durationMs - leftSummary.durationMs : 0,
                )}
                tone="info"
              />
              <MetricCard
                label="Span delta"
                value={rightTrace.spans.length - leftTrace.spans.length}
                tone="info"
              />
              <MetricCard
                label="Root service"
                value={`${leftSummary?.rootService ?? "Unknown service"} → ${rightSummary?.rootService ?? "Unknown service"}`}
                tone="info"
              />
              <MetricCard
                label="Status"
                value={compareStatus(leftSummary?.statusCode ?? "UNKNOWN", rightSummary?.statusCode ?? "UNKNOWN")}
                tone={rightSummary?.statusCode === "ERROR" ? "bad" : "good"}
              />
            </div>
            <div className="mt-4 grid gap-3 md:grid-cols-3">
              <div className="rounded border border-[var(--border)] bg-[var(--surface-inset)] p-3">
                <div className="field-label mb-1">Shared path</div>
                <p className="m-0 text-xs text-[var(--text)]">{diffListLabel(comparison?.shared ?? [])}</p>
              </div>
              <div className="rounded border border-[var(--border)] bg-[var(--surface-inset)] p-3">
                <div className="field-label mb-1">Left only</div>
                <p className="m-0 text-xs text-[var(--text)]">{diffListLabel(comparison?.leftOnly ?? [])}</p>
              </div>
              <div className="rounded border border-[var(--border)] bg-[var(--surface-inset)] p-3">
                <div className="field-label mb-1">Right only</div>
                <p className="m-0 text-xs text-[var(--text)]">{diffListLabel(comparison?.rightOnly ?? [])}</p>
              </div>
            </div>
          </Panel>

          <div className="grid gap-3 lg:grid-cols-2">
            <TraceSummaryPanel
              label="Baseline"
              trace={leftTrace}
              format={format}
              tone="info"
            />
            <TraceSummaryPanel
              label="Comparison"
              trace={rightTrace}
              format={format}
              tone={rightTrace.spans[0]?.status_code === "ERROR" ? "bad" : "good"}
            />
          </div>
        </>
      ) : null}
    </section>
  );
}

function TraceSummaryPanel({
  label,
  trace,
  format,
  tone,
}: {
  label: string;
  trace: TraceResponse;
  format: TimeFormat;
  tone: "info" | "good" | "bad" | "warn";
}) {
  const summary = summarizeTrace(trace);
  const root = trace.spans[0];

  return (
    <Panel
      eyebrow={label}
      title={
        <CopyableText value={trace.trace_id} label="Copy trace id" mono>
          <span title={trace.trace_id}>{trace.trace_id.substring(0, 16)}…</span>
        </CopyableText>
      }
      actions={
        <Link
          to="/traces/$traceId"
          params={{ traceId: trace.trace_id }}
          className="secondary-link"
        >
          Open full trace
        </Link>
      }
    >
      <div className="mb-3 flex items-center gap-2">
        <Badge tone={tone}>{summary.statusCode}</Badge>
        <span className="text-xs text-[var(--muted)]">
          {summary.spanCount} spans, {summary.errorCount} errors
        </span>
      </div>

      <div className="grid gap-2 md:grid-cols-2">
        <MetricCard label="Duration" value={`${summary.durationMs.toFixed(2)}ms`} tone="info" />
        <MetricCard label="Services" value={new Set(trace.spans.map((span) => span.service_name)).size} tone="info" />
      </div>

      <dl className="mt-4 grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
        <DlRow label="root service" copyValue={summary.rootService}>
          {summary.rootService}
        </DlRow>
        <DlRow label="root operation" copyValue={summary.rootOperation}>
          {summary.rootOperation}
        </DlRow>
        <DlRow label="start time">
          {root ? formatTimestamp(root.start_time_unix_nano, format) : "Unknown"}
        </DlRow>
        <DlRow label="trace id" copyValue={trace.trace_id}>
          {trace.trace_id}
        </DlRow>
      </dl>
    </Panel>
  );
}
