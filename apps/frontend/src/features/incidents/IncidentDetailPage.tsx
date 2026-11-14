import { useState } from "react";
import { Link, useParams } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { IncidentEventItem, IncidentFindingItem } from "../../api/incidents";
import { ActionFeedback, type ActionFeedbackStatus } from "../../components/ui/action-feedback";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import { ConfirmDialog } from "../../components/ui/confirm-dialog";
import { EmptyState } from "../../components/ui/empty-state";
import { ErrorState } from "../../components/ui/error-state";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { CopyableText } from "../../components/ui/copy-button";
import { useAccessContext } from "../../hooks/useAccessContext";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp, isoToNs } from "../../utils/formatTimestamp";
import { TopologyMap } from "../../components/topology/TopologyMap";
import { useRuntime } from "../../hooks/useRuntime";
import { readReturnContext } from "../../components/ScopeBar";
import type { RootSearch } from "../../router";

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
    case "escalated":         return "!";
    case "mitigated":         return "✓";
    case "milestone":         return "◆";
    case "exception":         return "!";
    case "comment":           return "·";
    case "status_change":     return "→";
    case "deployment_linked": return "↑";
    default:                  return "·";
  }
}

const LINKED_EVENT_TYPES = new Set(["alert_fired", "alert_resolved"]);

export function IncidentDetailPage() {
  const { tenantId } = useTenantContext();
  const { canMutateIncidents, isElevated, elevation } = useAccessContext();
  const runtime = useRuntime();
  const queryClient = useQueryClient();
  const { format } = useTimeDisplay();
  const { incidentId } = useParams({ from: "/incidents/$incidentId" });
  const [commentText, setCommentText] = useState("");
  const [assigneeText, setAssigneeText] = useState("");
  const [confirmResolveOpen, setConfirmResolveOpen] = useState(false);
  const [feedback, setFeedback] = useState<{
    status: ActionFeedbackStatus;
    message: string;
    detail?: string;
    retry?: () => void;
  }>({ status: "idle", message: "" });

  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: ["incident", tenantId, incidentId],
    queryFn: () => runtime.incidents.get(tenantId, incidentId),
    retry: (failureCount, err) =>
      !/(?:401|403|404|unauthorized|forbidden|not found)/i.test(String(err)) && failureCount < 3,
  });

  const acknowledgeMutation = useMutation({
    mutationFn: () => runtime.incidents.acknowledge(tenantId, incidentId),
    onMutate: () => {
      setFeedback({ status: "pending", message: "Acknowledging incident…" });
    },
    onSuccess: () => {
      setFeedback({ status: "success", message: "Incident acknowledged" });
      queryClient.invalidateQueries({ queryKey: ["incident", tenantId, incidentId] });
      queryClient.invalidateQueries({ queryKey: ["incidents", tenantId] });
    },
    onError: (err) => {
      setFeedback({
        status: "error",
        message: "Acknowledgement failed",
        detail: err instanceof Error ? err.message : String(err),
        retry: () => acknowledgeMutation.mutate(),
      });
    },
  });

  const statusMutation = useMutation({
    mutationFn: (newStatus: string) =>
      runtime.incidents.updateStatus(tenantId, incidentId, newStatus),
    onMutate: (newStatus) => {
      setFeedback({ status: "pending", message: `Updating incident status to ${newStatus}…` });
    },
    onSuccess: (_data, newStatus) => {
      setFeedback({ status: "success", message: `Incident marked ${newStatus}` });
      queryClient.invalidateQueries({ queryKey: ["incident", tenantId, incidentId] });
      queryClient.invalidateQueries({ queryKey: ["incidents", tenantId] });
    },
    onError: (err, newStatus) => {
      setFeedback({
        status: "error",
        message: "Status update failed",
        detail: err instanceof Error ? err.message : String(err),
        retry: () => statusMutation.mutate(newStatus),
      });
    },
  });

  const resolveMutation = useMutation({
    mutationFn: () => runtime.incidents.resolve(tenantId, incidentId),
    onMutate: () => {
      setFeedback({ status: "pending", message: "Resolving incident…" });
    },
    onSuccess: () => {
      setConfirmResolveOpen(false);
      setFeedback({
        status: "success",
        message: isElevated
          ? "Incident resolved (recorded under elevated access)"
          : "Incident resolved",
      });
      queryClient.invalidateQueries({ queryKey: ["incident", tenantId, incidentId] });
      queryClient.invalidateQueries({ queryKey: ["incidents", tenantId] });
    },
    onError: (err) => {
      setConfirmResolveOpen(false);
      setFeedback({
        status: "error",
        message: "Resolution failed",
        detail: err instanceof Error ? err.message : String(err),
        retry: () => setConfirmResolveOpen(true),
      });
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

  const assignMutation = useMutation({
    mutationFn: (actor: string) =>
      runtime.incidents.acknowledge(tenantId, incidentId, {
        actor,
        message: `Assigned incident to ${actor}`,
      }),
    onMutate: (actor) => {
      setFeedback({ status: "pending", message: `Assigning incident to ${actor}…` });
    },
    onSuccess: (_data, actor) => {
      setAssigneeText("");
      setFeedback({ status: "success", message: `Assigned to ${actor}` });
      queryClient.invalidateQueries({ queryKey: ["incident", tenantId, incidentId] });
      queryClient.invalidateQueries({ queryKey: ["incidents", tenantId] });
    },
    onError: (err, actor) => {
      setFeedback({
        status: "error",
        message: "Assignment failed",
        detail: err instanceof Error ? err.message : String(err),
        retry: () => assignMutation.mutate(actor),
      });
    },
  });

  const escalateMutation = useMutation({
    mutationFn: async () => {
      const res = await fetch(`/v1/incidents/${incidentId}/escalate`, {
        method: "POST",
        headers: { "X-Tenant-ID": tenantId, "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({ target: "secondary-oncall", reason: "Responder escalation" }),
      });
      if (!res.ok) throw new Error(`Failed to escalate incident: ${res.status}`);
      return res.json();
    },
    onMutate: () => {
      setFeedback({ status: "pending", message: "Escalating incident…" });
    },
    onSuccess: () => {
      setFeedback({ status: "success", message: "Incident escalated to secondary-oncall" });
      queryClient.invalidateQueries({ queryKey: ["incident", tenantId, incidentId] });
    },
    onError: (err) => {
      setFeedback({
        status: "error",
        message: "Escalation failed",
        detail: err instanceof Error ? err.message : String(err),
        retry: () => escalateMutation.mutate(),
      });
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

  if (isError) {
    const errText = error instanceof Error ? error.message : String(error);
    if (/(?:401|403|404|unauthorized|forbidden|not found)/i.test(errText)) {
      return (
        <EmptyState
          data-testid="incident-withheld"
          title="Incident unavailable"
          description="This incident does not exist or you do not have permission to view it."
        />
      );
    }
    return (
      <ErrorState
        title="Failed to load incident"
        description="The incident details could not be retrieved."
        error={errText}
        onRetry={() => refetch()}
      />
    );
  }

  if (!data) {
    return (
      <EmptyState
        data-testid="incident-withheld"
        title="Incident unavailable"
        description="This incident does not exist or you do not have permission to view it."
      />
    );
  }

  const returnTo = readReturnContext();

  return (
    <section className="page-stack">
      <div className="page-header flex-wrap gap-3">
        <div>
          <div className="mb-1">
            {returnTo ? (
              <a href={returnTo} className="text-xs text-[var(--brand)] hover:underline">
                ← Back to previous view
              </a>
            ) : (
              <Link to="/incidents" className="text-xs text-[var(--brand)] hover:underline">
                ← All incidents
              </Link>
            )}
          </div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Incident</div>
          <h1>
            <CopyableText value={data.title} label="Copy incident title" />
          </h1>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          {canMutateIncidents && data.status === "triggered" && (
            <Button
              variant="secondary"
              disabled={acknowledgeMutation.isPending}
              onClick={() => acknowledgeMutation.mutate()}
            >
              {acknowledgeMutation.isPending ? "Acknowledging…" : "Acknowledge"}
            </Button>
          )}
          {canMutateIncidents && data.status !== "investigating" && data.status !== "resolved" && (
            <Button
              variant="secondary"
              disabled={statusMutation.isPending}
              onClick={() => statusMutation.mutate("investigating")}
            >
              Investigate
            </Button>
          )}
          {canMutateIncidents && data.status !== "resolved" && (
            <Button
              variant="secondary"
              disabled={escalateMutation.isPending}
              onClick={() => escalateMutation.mutate()}
            >
              {escalateMutation.isPending ? "Escalating…" : "Escalate"}
            </Button>
          )}
          {canMutateIncidents && data.status !== "resolved" && (
            <Button
              variant="primary"
              disabled={resolveMutation.isPending}
              onClick={() => setConfirmResolveOpen(true)}
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

      <ActionFeedback
        status={feedback.status}
        message={feedback.message}
        detail={feedback.detail}
        onRetry={feedback.retry}
        onDismiss={
          feedback.status === "success" || feedback.status === "error"
            ? () => setFeedback({ status: "idle", message: "" })
            : undefined
        }
      />

      <ConfirmDialog
        open={confirmResolveOpen}
        title="Resolve incident"
        description={`Resolve "${data.title}"?`}
        consequences="Resolving this incident marks it closed and stops active incident tracking. Ensure customer impact is mitigated before continuing."
        elevatedContext={isElevated ? { scope: elevation?.scope, justification: elevation?.justification } : null}
        confirmLabel="Confirm resolve"
        cancelLabel="Cancel"
        variant="destructive"
        isPending={resolveMutation.isPending}
        onConfirm={() => resolveMutation.mutate()}
        onCancel={() => setConfirmResolveOpen(false)}
      />

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
          <div>
            <div className="field-label">Owner</div>
            <div className="mt-1 flex flex-wrap items-center gap-2">
              {data.owner || data.acknowledged_by ? (
                <span>{data.owner ?? data.acknowledged_by}</span>
              ) : (
                <Badge tone={data.severity === "critical" ? "bad" : "neutral"}>
                  Unassigned
                </Badge>
              )}
              {canMutateIncidents && data.status !== "resolved" && (
                <form
                  className="flex items-center gap-1"
                  onSubmit={(e) => {
                    e.preventDefault();
                    if (assigneeText.trim()) assignMutation.mutate(assigneeText.trim());
                  }}
                >
                  <input
                    type="text"
                    aria-label="Assign owner"
                    className="input-base text-xs py-0.5 px-2 w-32"
                    placeholder="Assign owner…"
                    value={assigneeText}
                    onChange={(e) => setAssigneeText(e.target.value)}
                    disabled={assignMutation.isPending}
                  />
                  <Button
                    type="submit"
                    variant="secondary"
                    disabled={!assigneeText.trim() || assignMutation.isPending}
                  >
                    Assign
                  </Button>
                </form>
              )}
            </div>
          </div>
          <div>
            <div className="field-label">Impacted services</div>
            <div className="mt-1 flex flex-wrap gap-2">
              {(data.affected_services?.length
                ? data.affected_services
                : data.impacted_service
                  ? [data.impacted_service]
                  : []
              ).length > 0 ? (
                (data.affected_services?.length
                  ? data.affected_services
                  : [data.impacted_service!]
                ).map((svc) => (
                  <Link
                    key={svc}
                    to="/services/$serviceId"
                    params={{ serviceId: svc }}
                    search={(prev: RootSearch) => ({
                      ...prev,
                      from: data.triggered_at,
                      to: data.resolved_at ?? prev.to,
                    })}
                    className="text-[var(--brand)] hover:underline"
                  >
                    {svc}
                  </Link>
                ))
              ) : (
                <span className="text-[var(--muted)]">Unknown</span>
              )}
            </div>
          </div>
          <div className="col-span-2">
            <div className="field-label">Business impact</div>
            <div className="mt-1" data-testid="incident-business-impact">
              {data.impact_summary?.trim() ? (
                <span>{data.impact_summary}</span>
              ) : (
                <span className="text-[var(--muted)]">Unknown</span>
              )}
            </div>
          </div>
          <div className="col-span-2">
            <div className="field-label">Escalation route</div>
            <div className="mt-1">
              {data.runbook_url ? (
                <a
                  href={data.runbook_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[var(--brand)] hover:underline"
                >
                  {data.runbook_url}
                </a>
              ) : (
                <span>Primary on-call escalation</span>
              )}
            </div>
          </div>
        </div>
      </Panel>

      <Panel>
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-sm font-semibold">Timeline</h3>
        </div>
        {canMutateIncidents && (
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
              aria-label="Add a timeline note or comment"
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
        )}
        <div className="space-y-3">
          {[...data.timeline]
            .sort((a, b) => {
              const dt = Date.parse(a.event_time) - Date.parse(b.event_time);
              if (dt !== 0) return dt;
              return a.event_type.localeCompare(b.event_type) || a.actor.localeCompare(b.actor);
            })
            .map((event: IncidentEventItem & { clock_uncertain?: boolean }, idx: number) => {
              const showLink =
                LINKED_EVENT_TYPES.has(event.event_type) &&
                data.triggered_by_rule_id !== null;
              const isClockUncertain =
                Boolean(event.clock_uncertain) ||
                Date.parse(event.event_time) > Date.now() + 60_000;
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
                      {isClockUncertain && <Badge tone="warn">Clock uncertain</Badge>}
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

      {(data.findings ?? []).length > 0 && (
        <Panel>
          <div className="flex items-center justify-between mb-3">
            <h3 className="text-sm font-semibold">Investigation Findings &amp; Cause Assessment</h3>
          </div>
          <div className="space-y-2.5" data-testid="incident-findings">
            {(data.findings ?? []).map((finding: IncidentFindingItem) => {
              const isConfirmed =
                finding.kind === "confirmed_cause" &&
                finding.verified === true &&
                (finding.evidence_refs?.length ?? 0) > 0;
              const tone = isConfirmed
                ? "good"
                : finding.kind === "observed_fact" || finding.kind === "relationship"
                  ? "neutral"
                  : "warn";
              const label = isConfirmed
                ? "Confirmed cause"
                : finding.kind === "observed_fact"
                  ? "Observed evidence"
                  : finding.kind === "relationship"
                    ? "Relationship"
                    : "Unverified hypothesis";
              return (
                <div
                  key={finding.id}
                  data-finding-kind={isConfirmed ? "confirmed_cause" : finding.kind}
                  data-finding-verified={String(isConfirmed)}
                  className="flex flex-col gap-1 rounded border border-[var(--border)] p-2.5 text-sm"
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-medium">{finding.title}</span>
                    <Badge tone={tone}>{label}</Badge>
                  </div>
                  {finding.summary && (
                    <p className="text-xs text-[var(--muted)]">{finding.summary}</p>
                  )}
                  {(finding.evidence_refs?.length ?? 0) > 0 && (
                    <p className="text-xs text-[var(--muted)]">
                      Evidence: {finding.evidence_refs!.join(", ")}
                    </p>
                  )}
                </div>
              );
            })}
          </div>
        </Panel>
      )}

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
                search={(prev: RootSearch) => ({
                  ...prev,
                  from: data.triggered_at,
                  to: data.resolved_at ?? prev.to,
                })}
                className="text-[var(--brand)] hover:underline"
              >
                → Service Detail
              </Link>
              <Link
                to="/logs"
                search={(prev: RootSearch) => ({
                  ...prev,
                  from: data.triggered_at,
                  to: data.resolved_at ?? prev.to,
                })}
                className="text-[var(--brand)] hover:underline"
              >
                → Observed Errors
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
