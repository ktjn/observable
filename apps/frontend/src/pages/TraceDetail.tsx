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
    <div>
      <h2>Trace {traceId.substring(0, 16)}…</h2>
      <p>
        Total: {(totalNs / 1e6).toFixed(2)}ms — {spans.length} spans
      </p>

      {infraPills.length > 0 && (
        <div
          aria-label="Infrastructure"
          style={{ display: "flex", flexWrap: "wrap", gap: 8, marginBottom: 12 }}
        >
          {infraPills.map((link) => (
            <a
              key={link.href}
              href={link.href}
              style={{
                fontSize: 12,
                padding: "2px 8px",
                borderRadius: 12,
                background: "var(--color-bg-subtle, #edf2f7)",
                color: "var(--color-text, #2d3748)",
                textDecoration: "none",
                border: "1px solid var(--color-border, #e2e8f0)",
              }}
            >
              {link.label}
            </a>
          ))}
        </div>
      )}

      <div style={{ overflowX: "auto" }}>
        {spans.map((span) => {
          const offset =
            ((Number(span.start_time_unix_nano) - minStart) / totalNs) * 100;
          const width = (span.duration_ns / totalNs) * 100;
          return (
            <div
              key={span.span_id}
              onClick={() =>
                setSelectedSpanId(
                  span.span_id === selectedSpanId ? undefined : span.span_id
                )
              }
              style={{
                display: "flex",
                alignItems: "center",
                marginBottom: 4,
                cursor: "pointer",
                background:
                  selectedSpanId === span.span_id ? "#edf2f7" : "transparent",
                borderRadius: "4px",
                padding: "2px 0",
              }}
            >
              <span
                style={{
                  width: 200,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                  fontSize: 12,
                }}
              >
                {span.service_name}: {span.operation_name}
              </span>
              <div
                style={{
                  flex: 1,
                  position: "relative",
                  height: 16,
                  background: "#f0f0f0",
                }}
              >
                <div
                  style={{
                    position: "absolute",
                    left: `${offset}%`,
                    width: `${Math.max(width, 0.5)}%`,
                    height: "100%",
                    background:
                      span.status_code === "ERROR" ? "#e53e3e" : "#4299e1",
                  }}
                  title={`${(span.duration_ns / 1e6).toFixed(2)}ms`}
                />
              </div>
              <span style={{ width: 60, textAlign: "right", fontSize: 12 }}>
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
