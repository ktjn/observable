import { useState } from "react";
import { Link, useParams } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { IncidentEventItem } from "../../api/incidents";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { CopyableText } from "../../components/ui/copy-button";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp, isoToNs } from "../../utils/formatTimestamp";
import { TopologyMap } from "../../components/topology/TopologyMap";
import { useRuntime } from "../../hooks/useRuntime";

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

const LINKED_EVENT_TYPES = new Set(["alert_fired", "alert_resolved"]);

export function IncidentDetailPage() {
  const { tenantId } = useTenantContext();
  const runtime = useRuntime();
  const queryClient = useQueryClient();
  const { format } = useTimeDisplay();
  const { incidentId } = useParams({ from: "/incidents/$incidentId" });
  const [commentText, setCommentText] = useState("");

  const { data, isLoading } = useQuery({
    queryKey: ["incident", tenantId, incidentId],
    queryFn: () => runtime.incidents.get(tenantId, incidentId),
  });

  const acknowledgeMutation = useMutation({
    mutationFn: () => runtime.incidents.acknowledge(tenantId, incidentId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["incident", tenantId, incidentId] });
      queryClient.invalidateQueries({ queryKey: ["incidents", tenantId] });
    },
  });

  const statusMutation = useMutation({
    mutationFn: (newStatus: string) =>
      runtime.incidents.updateStatus(tenantId, incidentId, newStatus),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["incident", tenantId, incidentId] });
      queryClient.invalidateQueries({ queryKey: ["incidents", tenantId] });
    },
  });

  const resolveMutation = useMutation({
    mutationFn: () => runtime.incidents.resolve(tenantId, incidentId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["incident", tenantId, incidentId] });
      queryClient.invalidateQueries({ queryKey: ["incidents", tenantId] });
    },
  });

  const commentMutation = useMutation({
    mutationFn: (message: string) =>
      runtime.incidents.addComment(tenantId, incidentId, message),
    onSuccess: () => {
      setCommentText("");
      queryClient.invalidateQueries({ queryKey: ["incident", tenantId, incidentId] });
    },
  });

  const triggeredAtMs = data ? new Date(data.triggered_at).getTime() : 0;
  const resolvedAtMs = data?.resolved_at ? new Date(data.resolved_at).getTime() : null;

  const {
    data: topologyData,
    isLoading: topoLoading,
    isError: topoError,
  } = useQuery({
    queryKey: ["topology-impact", tenantId, data?.impacted_service, triggeredAtMs, resolvedAtMs],
    queryFn: () =>
      runtime.topology.get(tenantId, {
        service: data!.impacted_service!,
        from: triggeredAtMs,
        to: resolvedAtMs ?? Date.now(),
      }),
    enabled: !!data?.impacted_service,
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
          <h1>
            <CopyableText value={data.title} label="Copy incident title" />
          </h1>
        </div>
        <div className="flex items-center gap-2">
          {data.status === "triggered" && (
            <Button
              variant="secondary"
              disabled={acknowledgeMutation.isPending}
              onClick={() => acknowledgeMutation.mutate()}
            >
              Acknowledge
            </Button>
          )}
          {data.status !== "investigating" && data.status !== "resolved" && (
            <Button
              variant="secondary"
              disabled={statusMutation.isPending}
              onClick={() => statusMutation.mutate("investigating")}
            >
              Investigate
            </Button>
          )}
          {data.status !== "resolved" && (
            <Button
              variant="primary"
              disabled={resolveMutation.isPending}
              onClick={() => resolveMutation.mutate()}
            >
              Resolve
            </Button>
          )}
          <Badge tone={severityColor(data.severity)}>{data.severity}</Badge>
          <Badge tone={statusColor(data.status)}>
            {data.status.charAt(0).toUpperCase() + data.status.slice(1)}
          </Badge>
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
          {data.runbook_url && (
            <div className="col-span-2">
              <div className="field-label">Runbook</div>
              <div className="mt-1">
                <a
                  href={data.runbook_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[var(--brand)] hover:underline"
                >
                  {data.runbook_url}
                </a>
              </div>
            </div>
          )}
        </div>
      </Panel>

      <Panel>
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-sm font-semibold">Timeline</h3>
        </div>
        <form
          className="mb-4 flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (commentText.trim()) {
              commentMutation.mutate(commentText.trim());
            }
          }}
        >
          <input
            type="text"
            className="input-base flex-1 text-sm"
            placeholder="Add a timeline note or comment..."
            value={commentText}
            onChange={(e) => setCommentText(e.target.value)}
            disabled={commentMutation.isPending}
          />
          <Button
            type="submit"
            variant="secondary"
            disabled={!commentText.trim() || commentMutation.isPending}
          >
            Add Note
          </Button>
        </form>
        <div className="space-y-3">
          {data.timeline.map((event: IncidentEventItem, idx: number) => {
            const showLink =
              LINKED_EVENT_TYPES.has(event.event_type) &&
              data.triggered_by_rule_id !== null;
            return (
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
                  <div className="flex items-center gap-3">
                    <p className="text-xs text-[var(--muted)]">by {event.actor}</p>
                    {showLink && (
                      <Link
                        to="/alerts/$ruleId"
                        params={{ ruleId: data.triggered_by_rule_id! }}
                        className="text-xs text-[var(--brand)] hover:underline"
                      >
                        → View rule
                      </Link>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
          {data.timeline.length === 0 && (
            <p className="text-sm text-[var(--muted)]">No timeline events.</p>
          )}
        </div>
      </Panel>

      {data.impacted_service && (
        <Panel>
          <div className="flex items-center justify-between mb-3">
            <div>
              <h3 className="text-sm font-semibold">Impacted Services</h3>
              <CopyableText
                value={data.impacted_service}
                label="Copy impacted service"
                className="mt-0.5 text-xs text-[var(--muted)]"
              />
            </div>
            <div className="flex gap-3 text-xs">
              <Link
                to="/services/$serviceId"
                params={{ serviceId: data.impacted_service }}
                className="text-[var(--brand)] hover:underline"
              >
                → Service Detail
              </Link>
              <Link to="/topology" className="text-[var(--brand)] hover:underline">
                → View in Topology
              </Link>
            </div>
          </div>
          {topoLoading ? (
            <LoadingState>Loading topology…</LoadingState>
          ) : topoError ? (
            <p className="text-sm text-[var(--muted)]">Could not load topology data.</p>
          ) : (
            <div style={{ height: 320 }}>
              <TopologyMap
                edges={topologyData?.edges ?? []}
                allServices={Array.from(
                  new Set([
                    data.impacted_service,
                    ...(topologyData?.edges.flatMap((e) => [e.caller, e.callee]) ?? []),
                  ]),
                )}
                focusedService={data.impacted_service}
                onNodeClick={() => {}}
                onEdgeClick={() => {}}
                onBackgroundClick={() => {}}
              />
              {(topologyData?.edges ?? []).length === 0 && (
                <p className="text-xs text-[var(--muted)] mt-2">
                  No observed call relationships during this incident.
                </p>
              )}
            </div>
          )}
        </Panel>
      )}
    </section>
  );
}
