import { useParams } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { getIncident, type IncidentEventItem } from "../../api/incidents";
import { Badge } from "../../components/ui/badge";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp, isoToNs } from "../../utils/formatTimestamp";

function severityColor(severity: string): "bad" | "warn" | "neutral" {
  switch (severity) {
    case "critical": return "bad";
    case "warning":  return "warn";
    default:         return "neutral";
  }
}

function statusColor(status: string): "bad" | "warn" | "good" | "neutral" {
  switch (status) {
    case "triggered":    return "bad";
    case "acknowledged": return "warn";
    case "resolved":     return "good";
    default:             return "neutral";
  }
}

function eventGlyph(eventType: string): string {
  switch (eventType) {
    case "triggered":         return "▸";
    case "alert_fired":       return "!";
    case "alert_resolved":    return "✓";
    case "acknowledged":      return "◎";
    case "comment":           return "·";
    case "status_change":     return "→";
    case "deployment_linked": return "↑";
    default:                  return "·";
  }
}

export function IncidentDetailPage() {
  const { tenantId } = useTenantContext();
  const { format } = useTimeDisplay();
  const { incidentId } = useParams({ from: "/incidents/$incidentId" });

  const { data, isLoading } = useQuery({
    queryKey: ["incident", tenantId, incidentId],
    queryFn: () => getIncident(tenantId, incidentId),
  });

  if (isLoading) {
    return <LoadingState>Loading incident...</LoadingState>;
  }

  if (!data) {
    return <Panel>Incident not found.</Panel>;
  }

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Incident</div>
          <h1>{data.title}</h1>
        </div>
        <div className="flex items-center gap-2">
          <Badge tone={severityColor(data.severity)}>{data.severity}</Badge>
          <Badge tone={statusColor(data.status)}>{data.status}</Badge>
        </div>
      </div>

      <Panel>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <div>
            <div className="field-label">Triggered</div>
            <div className="mt-1">{formatTimestamp(isoToNs(data.triggered_at), format)}</div>
          </div>
          <div>
            <div className="field-label">Resolved</div>
            <div className="mt-1">
              {data.resolved_at
                ? formatTimestamp(isoToNs(data.resolved_at), format)
                : "—"}
            </div>
          </div>
        </div>
      </Panel>

      <Panel>
        <h3 className="text-sm font-semibold mb-3">Timeline</h3>
        <div className="space-y-3">
          {data.timeline.map((event: IncidentEventItem, idx: number) => (
            <div key={idx} className="flex gap-3">
              <div className="font-mono text-base leading-none text-[var(--muted)] w-4 flex-shrink-0">
                {eventGlyph(event.event_type)}
              </div>
              <div className="flex-1">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium capitalize">
                    {event.event_type.replace(/_/g, " ")}
                  </span>
                  <span className="text-xs text-[var(--muted)]">
                    {formatTimestamp(isoToNs(event.event_time), format)}
                  </span>
                </div>
                {event.message && (
                  <p className="text-sm text-[var(--muted)] mt-0.5">{event.message}</p>
                )}
                <p className="text-xs text-[var(--muted)]">by {event.actor}</p>
              </div>
            </div>
          ))}
          {data.timeline.length === 0 && (
            <p className="text-sm text-[var(--muted)]">No timeline events.</p>
          )}
        </div>
      </Panel>
    </section>
  );
}
