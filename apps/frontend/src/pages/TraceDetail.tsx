import { useState } from "react";
import { Span } from "../api/traces";
import { LogCorrelatedList } from "../components/LogCorrelatedList";
import { infraLinks, InfraLink } from "../utils/infraLinks";

interface Props {
  traceId: string;
  spans: Span[];
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

export function TraceDetail({ traceId, spans }: Props) {
  const [selectedSpanId, setSelectedSpanId] = useState<string | undefined>();
  const minStart = Math.min(...spans.map((s) => Number(s.start_time_unix_nano)));
  const maxEnd = Math.max(...spans.map((s) => Number(s.end_time_unix_nano)));
  const totalNs = maxEnd - minStart || 1;

  const infraPills = mergedInfraLinks(spans);

  return (
    <div className="grid gap-4">
      <div>
        <h1 className="text-2xl font-bold text-[var(--text-strong)] mt-0">
          Trace {traceId.substring(0, 16)}…
        </h1>
        <p className="m-0 text-sm text-[var(--muted)]">
          Total: {(totalNs / 1e6).toFixed(2)}ms — {spans.length} spans
        </p>
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

      <div className="overflow-x-auto">
        {spans.map((span) => {
          const offset =
            ((Number(span.start_time_unix_nano) - minStart) / totalNs) * 100;
          const width = (span.duration_ns / totalNs) * 100;
          const isSelected = selectedSpanId === span.span_id;
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
              <span className="w-[200px] overflow-hidden text-ellipsis whitespace-nowrap text-xs shrink-0">
                {span.service_name}: {span.operation_name}
              </span>
              <div className="flex-1 relative h-4 bg-[var(--surface-inset)]">
                <div
                  className="absolute h-full"
                  style={{
                    left: `${offset}%`,
                    width: `${Math.max(width, 0.5)}%`,
                    background:
                      span.status_code === "ERROR"
                        ? "var(--bad)"
                        : "var(--brand)",
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
      <LogCorrelatedList traceId={traceId} spanId={selectedSpanId} />
    </div>
  );
}
