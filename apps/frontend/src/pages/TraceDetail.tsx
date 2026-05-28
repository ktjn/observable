import { useState, type ReactNode } from "react";
import { Link } from "@tanstack/react-router";
import { Span, SpanEvent } from "../api/traces";
import { LogCorrelatedList } from "../components/LogCorrelatedList";
import { infraLinks, InfraLink } from "../utils/infraLinks";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { MetricCard } from "../components/ui/metric-card";
import { Panel } from "../components/ui/panel";

interface Props {
  traceId: string;
  spans: Span[];
  events?: SpanEvent[];
}

function mergedInfraLinks(spans: Span[]): InfraLink[] {
  const seen = new Set<string>();
  const result: InfraLink[] = [];
  for (const span of spans) {
    for (const link of infraLinks(span.resource_attributes ?? {})) {
      if (!seen.has(link.href)) {
        seen.add(link.href);
        result.push(link);
      }
    }
  }
  return result;
}

function buildDepthMap(spans: Span[]): Map<string, number> {
  const parentOf = new Map(spans.map((s) => [s.span_id, s.parent_span_id]));
  const memo = new Map<string, number>();
  function depth(id: string): number {
    if (memo.has(id)) return memo.get(id)!;
    const parent = parentOf.get(id);
    const d = parent && parentOf.has(parent) ? depth(parent) + 1 : 0;
    memo.set(id, d);
    return d;
  }
  for (const s of spans) depth(s.span_id);
  return memo;
}

const SERVICE_COLORS = [
  "var(--brand)",
  "#7c3aed",
  "#0891b2",
  "#059669",
  "#d97706",
  "#db2777",
  "#6d28d9",
  "#0284c7",
];

function serviceColor(name: string): string {
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) & 0xffff;
  return SERVICE_COLORS[h % SERVICE_COLORS.length];
}

function TimeRuler({ totalMs }: { totalMs: number }) {
  const ticks = [0, 0.25, 0.5, 0.75, 1.0];
  return (
    <div className="flex items-center mb-1 select-none" aria-hidden="true">
      <span className="w-[200px] shrink-0" />
      <div className="flex-1 relative h-4">
        {ticks.map((t) => (
          <span
            key={t}
            className="absolute text-[10px] text-[var(--muted)] -translate-x-1/2 top-0"
            style={{ left: `${t * 100}%` }}
          >
            {(totalMs * t).toFixed(1)}ms
          </span>
        ))}
      </div>
      <span className="w-[60px] shrink-0" />
    </div>
  );
}

function CopyButton({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      className="ml-1 text-[10px] text-[var(--muted)] hover:text-[var(--brand)]"
      onClick={() => {
        navigator.clipboard.writeText(value);
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      }}
      title="Copy"
    >
      {copied ? "✓" : "⎘"}
    </button>
  );
}

function SectionHeader({ children }: { children: ReactNode }) {
  return (
    <h3 className="m-0 mt-4 mb-2 text-xs font-bold uppercase text-[var(--muted)] border-b border-[var(--border)] pb-1">
      {children}
    </h3>
  );
}

function DlRow({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="contents">
      <dt className="break-all font-bold text-[var(--muted)]">{label}</dt>
      <dd className="m-0 min-w-0 break-all text-[var(--text)]">{children}</dd>
    </div>
  );
}

function SpanContextPanel({
  span,
  events,
  traceStartNs: _traceStartNs,
  onClose,
}: {
  span: Span;
  events: SpanEvent[];
  traceStartNs: number;
  onClose: () => void;
}) {
  const dbSystem = span.attributes?.["db.system"] as string | undefined;
  const dbName = span.attributes?.["db.name"] as string | undefined;
  const dbOp = span.attributes?.["db.operation"] as string | undefined;
  const dbStatement = span.attributes?.["db.statement"] as string | undefined;

  const httpMethod = span.attributes?.["http.method"] as string | undefined;
  const httpUrl = (span.attributes?.["http.url"] ??
    span.attributes?.["http.target"]) as string | undefined;
  const httpStatus = span.attributes?.["http.status_code"] as
    | string
    | number
    | undefined;

  const remainingAttrs = Object.entries(span.attributes ?? {}).filter(
    ([k]) => !k.startsWith("db.") && !k.startsWith("http.")
  );

  const spanInfraLinks = infraLinks(span.resource_attributes ?? {});
  const hasResourceSection =
    Object.keys(span.resource_attributes ?? {}).length > 0 ||
    spanInfraLinks.length > 0;

  const startMs = span.start_time_unix_nano / 1e6;
  const startDate = new Date(startMs).toISOString();

  return (
    <aside
      aria-label="Selected span context"
      className="w-[320px] shrink-0 border border-[var(--border)] bg-[var(--surface)] p-4 max-[900px]:w-full max-h-[calc(100vh-80px)] overflow-y-auto"
    >
      <div className="mb-3 flex items-start justify-between gap-3">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">
            Selected Span
          </div>
          <h2 className="m-0 text-base font-bold text-[var(--text-strong)]">
            Context Properties
          </h2>
        </div>
        <Button
          variant="secondary"
          className="min-h-8 px-2 text-xs"
          onClick={onClose}
        >
          Close
        </Button>
      </div>

      <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
        <DlRow label="trace_id">
          <span title={span.trace_id}>
            {span.trace_id.substring(0, 16)}…
          </span>
          <CopyButton value={span.trace_id} />
        </DlRow>
        <DlRow label="span_id">
          <span title={span.span_id}>
            {span.span_id.substring(0, 16)}
          </span>
          <CopyButton value={span.span_id} />
        </DlRow>
        <DlRow label="service">{span.service_name}</DlRow>
        {span.service_version && (
          <DlRow label="version">{span.service_version}</DlRow>
        )}
        <DlRow label="operation">{span.operation_name}</DlRow>
        <DlRow label="kind">{span.span_kind}</DlRow>
        <DlRow label="status">
          <Badge tone={span.status_code === "ERROR" ? "bad" : "good"}>
            {span.status_code}
          </Badge>
        </DlRow>
        <DlRow label="duration">
          {(span.duration_ns / 1e6).toFixed(2)}ms
        </DlRow>
        <DlRow label="start time">{startDate}</DlRow>
      </dl>

      {dbSystem && (
        <>
          <SectionHeader>DB Operation</SectionHeader>
          <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
            <DlRow label="system">{dbSystem}</DlRow>
            {dbName && <DlRow label="database">{dbName}</DlRow>}
            {dbOp && <DlRow label="operation">{dbOp}</DlRow>}
          </dl>
          {dbStatement && (
            <pre className="mt-2 text-[11px] p-2 bg-[var(--surface-inset)] border border-[var(--border)] overflow-x-auto whitespace-pre-wrap break-all">
              {dbStatement}
            </pre>
          )}
        </>
      )}

      {httpMethod && (
        <>
          <SectionHeader>HTTP</SectionHeader>
          <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
            <DlRow label="method">{httpMethod}</DlRow>
            {httpUrl && <DlRow label="url">{httpUrl}</DlRow>}
            {httpStatus !== undefined && (
              <DlRow label="status_code">{String(httpStatus)}</DlRow>
            )}
          </dl>
        </>
      )}

      {events.length > 0 && (
        <>
          <SectionHeader>Span Events</SectionHeader>
          <div className="space-y-2">
            {events.map((e) => {
              const offsetMs =
                (e.timestamp_unix_nano - span.start_time_unix_nano) / 1e6;
              return (
                <div
                  key={e.event_index}
                  className="text-xs border border-[var(--border)] p-2 bg-[var(--surface-inset)]"
                >
                  <div className="flex justify-between">
                    <span className="font-bold text-[var(--text-strong)]">
                      {e.name}
                    </span>
                    <span className="text-[var(--muted)]">
                      +{offsetMs.toFixed(2)}ms
                    </span>
                  </div>
                  {e.attributes && Object.keys(e.attributes).length > 0 && (
                    <dl className="mt-1 grid grid-cols-[minmax(88px,auto)_1fr] gap-x-2 gap-y-1 text-[11px]">
                      {Object.entries(e.attributes).map(([k, v]) => (
                        <div key={k} className="contents">
                          <dt className="text-[var(--muted)] font-bold break-all">
                            {k}
                          </dt>
                          <dd className="m-0 break-all">{String(v)}</dd>
                        </div>
                      ))}
                    </dl>
                  )}
                </div>
              );
            })}
          </div>
        </>
      )}

      {remainingAttrs.length > 0 && (
        <>
          <SectionHeader>Attributes</SectionHeader>
          <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
            {remainingAttrs.map(([k, v]) => (
              <DlRow key={k} label={k}>
                {String(v)}
              </DlRow>
            ))}
          </dl>
        </>
      )}

      {hasResourceSection && (
        <>
          <SectionHeader>Resource / Infrastructure</SectionHeader>
          {Object.keys(span.resource_attributes ?? {}).length > 0 && (
            <dl className="grid grid-cols-[minmax(88px,auto)_1fr] gap-x-3 gap-y-2 text-xs">
              {Object.entries(span.resource_attributes ?? {}).map(([k, v]) => (
                <DlRow key={k} label={k}>
                  {String(v)}
                </DlRow>
              ))}
            </dl>
          )}
          {spanInfraLinks.length > 0 && (
            <div className="flex flex-wrap gap-2 mt-2">
              {spanInfraLinks.map((link) => (
                <a
                  key={link.href}
                  href={link.href}
                  className="text-xs px-2 py-0.5 bg-[var(--surface-subtle)] text-[var(--text)] border border-[var(--border)] no-underline hover:border-[var(--brand)] hover:text-[var(--brand)]"
                >
                  {link.label}
                </a>
              ))}
            </div>
          )}
        </>
      )}
    </aside>
  );
}

export function TraceDetail({ traceId, spans, events }: Props) {
  const [selectedSpanId, setSelectedSpanId] = useState<string | undefined>();
  const minStart = Math.min(...spans.map((s) => Number(s.start_time_unix_nano)));
  const maxEnd = Math.max(...spans.map((s) => Number(s.end_time_unix_nano)));
  const totalNs = maxEnd - minStart || 1;
  const totalMs = totalNs / 1e6;

  const infraPills = mergedInfraLinks(spans);
  const depthMap = buildDepthMap(spans);
  const selectedSpan = spans.find((s) => s.span_id === selectedSpanId);

  const uniqueServices = [...new Set(spans.map((s) => s.service_name))];
  const errorCount = spans.filter((s) => s.status_code === "ERROR").length;
  const logPanelTitle = selectedSpanId
    ? `Exact span logs (${selectedSpanId.substring(0, 8)}…) and trace-level logs`
    : "Trace-correlated logs";

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Traces</div>
          <h1>{traceId.substring(0, 16)}…</h1>
        </div>
        <Link to="/traces" className="secondary-link">Back to traces</Link>
      </div>

      <div className="grid grid-cols-4 gap-3 max-[700px]:grid-cols-2">
        <MetricCard label="Total Spans" value={spans.length} tone="info" />
        <MetricCard label="Duration" value={`${totalMs.toFixed(2)}ms`} tone="info" />
        <MetricCard label="Services" value={uniqueServices.length} tone="info" />
        <MetricCard label="Errors" value={errorCount} tone={errorCount > 0 ? "bad" : "good"} />
      </div>

      <div className="flex flex-wrap gap-x-4 gap-y-1" aria-label="Service color legend">
        {uniqueServices.map((name) => (
          <span key={name} className="flex items-center gap-1.5 text-xs text-[var(--muted)]">
            <span
              aria-hidden="true"
              className="inline-block h-2 w-2 rounded-full shrink-0"
              style={{ background: serviceColor(name) }}
            />
            {name}
          </span>
        ))}
      </div>

      {infraPills.length > 0 && (
        <div aria-label="Infrastructure" className="flex flex-wrap gap-2">
          {infraPills.map((link) => (
            <a
              key={link.href}
              href={link.href}
              className="text-xs px-2 py-0.5 bg-[var(--surface-subtle)] text-[var(--text)] border border-[var(--border)] no-underline hover:border-[var(--brand)] hover:text-[var(--brand)]"
            >
              {link.label}
            </a>
          ))}
        </div>
      )}

      <Panel eyebrow="Waterfall" title="Spans">
        <div className="flex items-start gap-3 max-[900px]:flex-col">
          <div className="flex-1 min-w-0 overflow-x-auto">
            <TimeRuler totalMs={totalMs} />
              {spans.map((span) => {
                const offset =
                  ((Number(span.start_time_unix_nano) - minStart) / totalNs) * 100;
                const width = (span.duration_ns / totalNs) * 100;
                const isSelected = selectedSpanId === span.span_id;
                const depth = depthMap.get(span.span_id) ?? 0;
                const color =
                  span.status_code === "ERROR"
                    ? "var(--bad)"
                    : serviceColor(span.service_name);
                return (
                  <div
                    key={span.span_id}
                    role="button"
                    tabIndex={0}
                    onClick={() =>
                      setSelectedSpanId(isSelected ? undefined : span.span_id)
                    }
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        setSelectedSpanId(isSelected ? undefined : span.span_id);
                      }
                    }}
                    className={`flex items-center mb-1 cursor-pointer px-0 py-0.5 ${
                      isSelected ? "bg-[var(--surface-subtle)]" : "bg-transparent"
                    }`}
                  >
                    <span
                      className="w-[200px] overflow-hidden text-ellipsis whitespace-nowrap text-xs shrink-0"
                      style={{ paddingLeft: `${depth * 12}px` }}
                    >
                      {span.service_name}: {span.operation_name}
                      <span className="ml-1 text-[10px] text-[var(--muted)] font-mono">
                        [{span.span_kind}]
                      </span>
                    </span>
                    <div className="flex-1 relative h-4 bg-[var(--surface-inset)]">
                      <div
                        className="absolute h-full"
                        style={{
                          left: `${offset}%`,
                          width: `${Math.max(width, 0.5)}%`,
                          background: color,
                        }}
                        title={`${(span.duration_ns / 1e6).toFixed(2)}ms`}
                      />
                    </div>
                    <span className="w-[60px] text-right text-xs shrink-0">
                      {(span.duration_ns / 1e6).toFixed(2)}ms
                    </span>
                  </div>
                );
              })}
            </div>
            {selectedSpan && (
              <SpanContextPanel
                span={selectedSpan}
                events={(events ?? []).filter(
                  (e) => e.span_id === selectedSpan.span_id
                )}
                traceStartNs={minStart}
                onClose={() => setSelectedSpanId(undefined)}
              />
            )}
          </div>
      </Panel>

      <Panel eyebrow="Correlation" title={logPanelTitle}>
        <LogCorrelatedList traceId={traceId} spanId={selectedSpanId} />
      </Panel>
    </section>
  );
}
