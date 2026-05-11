import { useParams } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { getIncident, type IncidentEventItem } from "../../api/incidents";
import { Badge } from "../../components/ui/badge";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { Toolbar } from "../../components/ui/toolbar";
import { useTenantContext } from "../../hooks/useTenantContext";

function severityColor(severity: string): "bad" | "warn" | "neutral" {
  switch (severity) {
    case "critical":
      return "bad";
    case "warning":
      return "warn";
    default:
      return "neutral";
  }
}

function statusColor(status: string): "bad" | "warn" | "good" | "neutral" {
  switch (status) {
    case "triggered":
      return "bad";
    case "acknowledged":
      return "warn";
    case "resolved":
      return "good";
    default:
      return "neutral";
  }
}

function eventIcon(eventType: string): string {
  switch (eventType) {
    case "triggered":
      return "🔴";
    case "alert_fired":
      return "⚡";
    case "alert_resolved":
      return "✅";
    case "acknowledged":
      return "👤";
    case "comment":
      return "💬";
    case "status_change":
      return "🔄";
    case "deployment_linked":
      return "🚀";
    default:
      return "•";
  }
}

export function IncidentDetailPage() {
  const { tenantId } = useTenantContext();
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
    <div className="space-y-4">
      <Toolbar>{data.title}</Toolbar>

      <Panel>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <div>
            <span className="text-muted-foreground">Severity</span>
            <div className="mt-1">
              <Badge tone={severityColor(data.severity)}>{data.severity}</Badge>
            </div>
          </div>
          <div>
            <span className="text-muted-foreground">Status</span>
            <div className="mt-1">
              <Badge tone={statusColor(data.status)}>{data.status}</Badge>
            </div>
          </div>
          <div>
            <span className="text-muted-foreground">Triggered</span>
            <div className="mt-1">{new Date(data.triggered_at).toLocaleString()}</div>
          </div>
          <div>
            <span className="text-muted-foreground">Resolved</span>
            <div className="mt-1">
              {data.resolved_at ? new Date(data.resolved_at).toLocaleString() : "—"}
            </div>
          </div>
        </div>
      </Panel>

      <Panel>
        <h3 className="text-sm font-semibold mb-3">Timeline</h3>
        <div className="space-y-3">
          {data.timeline.map((event: IncidentEventItem, idx: number) => (
            <div key={idx} className="flex gap-3">
              <div className="text-lg leading-none">{eventIcon(event.event_type)}</div>
              <div className="flex-1">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium capitalize">{event.event_type.replace(/_/g, " ")}</span>
                  <span className="text-xs text-muted-foreground">
                    {new Date(event.event_time).toLocaleString()}
                  </span>
                </div>
                {event.message && (
                  <p className="text-sm text-muted-foreground mt-0.5">{event.message}</p>
                )}
                <p className="text-xs text-muted-foreground">by {event.actor}</p>
              </div>
            </div>
          ))}
          {data.timeline.length === 0 && (
            <p className="text-sm text-muted-foreground">No timeline events.</p>
          )}
        </div>
      </Panel>
    </div>
  );
}
