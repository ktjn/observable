import { useState } from "react";
import { useParams } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { assessRunbookGuidance, type FiringItem } from "../../api/alerts";

export function buildAlertScopedTarget(rule: {
  condition?: Record<string, unknown>;
  firing_items?: FiringItem[];
  created_at?: string;
  service_name?: string;
  environment?: string;
  incident_id?: string;
}) {
  const cond = (rule.condition ?? {}) as Record<string, unknown>;
  const service =
    rule.service_name ||
    (typeof cond.service_name === "string" ? cond.service_name : undefined) ||
    (typeof cond.service === "string" ? cond.service : undefined);
  const env =
    rule.environment ||
    (typeof cond.environment === "string" ? cond.environment : undefined);
  const windowSecs = typeof cond.window_secs === "number" ? cond.window_secs : 900;
  const firstFiring = rule.firing_items?.[0] as { fired_at?: string; last_evaluated_at?: string } | undefined;
  const firedAtIso = firstFiring?.fired_at ?? firstFiring?.last_evaluated_at ?? rule.created_at;
  const firedAtMs = firedAtIso ? Date.parse(firedAtIso) : Date.now();
  const toMs = Number.isFinite(firedAtMs) ? firedAtMs : Date.now();
  const fromMs = toMs - windowSecs * 1000;

  const params = new URLSearchParams();
  if (service) params.set("service", service);
  if (env) params.set("env", env);
  params.set("from", String(fromMs));
  params.set("to", String(toMs));
  const qs = params.toString();

  const incidentId =
    rule.incident_id ||
    (typeof cond.incident_id === "string" ? cond.incident_id : undefined);

  if (incidentId) {
    return {
      kind: "incident" as const,
      label: "Open scoped incident",
      href: `/incidents/${encodeURIComponent(incidentId)}?${qs}`,
      service,
      env,
      fromMs,
      toMs,
    };
  }
  if (service) {
    return {
      kind: "service" as const,
      label: "Open scoped service view",
      href: `/services/${encodeURIComponent(service)}?${qs}`,
      service,
      env,
      fromMs,
      toMs,
    };
  }
  return {
    kind: "incidents" as const,
    label: "Open scoped incidents",
    href: `/incidents?${qs}`,
    service,
    env,
    fromMs,
    toMs,
  };
}
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import { EmptyState } from "../../components/ui/empty-state";
import { Input } from "../../components/ui/input";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { CopyButton, CopyableText } from "../../components/ui/copy-button";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useRuntime } from "../../hooks/useRuntime";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp, isoToNs } from "../../utils/formatTimestamp";

function severityColor(severity: string): "bad" | "warn" | "neutral" {
  switch (severity) {
    case "critical": return "bad";
    case "warning":  return "warn";
    default:         return "neutral";
  }
}

function stateColor(state: string): "bad" | "warn" | "good" | "neutral" {
  switch (state) {
    case "active":   return "bad";
    case "pending":  return "warn";
    case "resolved": return "good";
    default:         return "neutral";
  }
}

function ConditionSummary({ condition }: { condition: Record<string, unknown> }) {
  const { metric_name, operator, threshold, slo_id } = condition;
  if (slo_id) {
    return (
      <>
        SLO burn-rate (<CopyableText value={String(slo_id)} label="Copy SLO id" mono />)
      </>
    );
  }
  if (metric_name && operator && threshold !== undefined) {
    const opSymbol: Record<string, string> = {
      gt: ">", gte: "≥", lt: "<", lte: "≤", eq: "=",
    };
    return <>{`${metric_name} ${opSymbol[operator as string] ?? operator} ${threshold}`}</>;
  }
  return <>{JSON.stringify(condition)}</>;
}

export function AlertRuleDetailPage() {
  const { tenantId } = useTenantContext();
  const runtime = useRuntime();
  const { format } = useTimeDisplay();
  const { ruleId } = useParams({ from: "/alerts/$ruleId" });

  const { data, isLoading } = useQuery({
    queryKey: ["alertRule", tenantId, ruleId],
    queryFn: () => runtime.alerts.get(tenantId, ruleId),
  });

  const queryClient = useQueryClient();
  const [editingRunbook, setEditingRunbook] = useState(false);
  const [runbookDraft, setRunbookDraft] = useState("");

  const runbookMutation = useMutation({
    mutationFn: (url: string | null) => runtime.alerts.setRunbook(tenantId, ruleId, url),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["alertRule", tenantId, ruleId] });
      setEditingRunbook(false);
    },
  });

  if (isLoading) {
    return <LoadingState>Loading alert rule...</LoadingState>;
  }

  if (!data) {
    return <Panel>Alert rule not found.</Panel>;
  }

  const scopedTarget = buildAlertScopedTarget(data);

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Alert Rule</div>
          <h1>{data.name}</h1>
        </div>
        <div className="flex items-center gap-2">
          <a
            href={`#${scopedTarget.href}`}
            data-testid="alert-scoped-target-link"
            className="text-xs font-semibold text-[var(--brand)] hover:underline"
          >
            {scopedTarget.label} →
          </a>
          <Badge tone={severityColor(data.severity)}>{data.severity}</Badge>
          {data.silenced && <Badge tone="neutral">silenced</Badge>}
          {data.firing && <Badge tone="bad">firing</Badge>}
        </div>
      </div>

      <Panel>
        <div className="grid grid-cols-2 gap-4 text-sm">
          <div>
            <div className="field-label">Type</div>
            <div className="mt-1 font-mono">{data.alert_type}</div>
          </div>
          <div>
            <div className="field-label">Condition</div>
            <div className="mt-1 font-mono">
              <ConditionSummary condition={data.condition} />
            </div>
          </div>
          <div className="col-span-2">
            <div className="field-label">Runbook</div>
            <div className="mt-1 flex items-center gap-2">
              {editingRunbook ? (
                <>
                  <Input
                    type="url"
                    className="flex-1"
                    value={runbookDraft}
                    onChange={(e) => setRunbookDraft(e.target.value)}
                    placeholder="https://..."
                    aria-label="Runbook URL"
                  />
                  <Button
                    onClick={() => runbookMutation.mutate(runbookDraft || null)}
                    disabled={runbookMutation.isPending}
                  >
                    Save
                  </Button>
                  <Button
                    variant="ghost"
                    onClick={() => {
                      setRunbookDraft(data.runbook_url ?? "");
                      setEditingRunbook(false);
                    }}
                  >
                    Cancel
                  </Button>
                </>
              ) : (
                <>
                  {data.runbook_url ? (
                    <span className="group inline-flex items-center gap-1">
                      <a
                        href={data.runbook_url}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-[var(--brand)] hover:underline"
                      >
                        {data.runbook_url}
                      </a>
                      <CopyButton value={data.runbook_url} label="Copy runbook URL" />
                    </span>
                  ) : (
                    <span className="text-[var(--muted)]">—</span>
                  )}
                  <button
                    type="button"
                    aria-label="Edit runbook URL"
                    onClick={() => {
                      setRunbookDraft(data.runbook_url ?? "");
                      setEditingRunbook(true);
                    }}
                    className="text-xs text-[var(--muted)] hover:text-[var(--text)]"
                  >
                    ✎
                  </button>
                  {(() => {
                    const guidance = assessRunbookGuidance(data);
                    if (!guidance.flaggedToOwner) return null;
                    return (
                      <span
                        data-testid="runbook-guidance-flag"
                        role="status"
                        className="ml-2 text-xs font-semibold text-[var(--warn)]"
                      >
                        {guidance.status === "missing"
                          ? "Guidance missing — flagged to owner"
                          : "Guidance past review — flagged to owner"}
                      </span>
                    );
                  })()}
                </>
              )}
            </div>
          </div>
        </div>
      </Panel>

      <Panel eyebrow="Last 20 firings">
        {data.firings.length === 0 ? (
          <EmptyState title="No firings recorded." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm" aria-label="Firing history">
              <thead>
                <tr className="border-b text-left">
                  <th className="pb-2 pr-4">State</th>
                  <th className="pb-2 pr-4">Value</th>
                  <th className="pb-2 pr-4">Occurred At</th>
                  <th className="pb-2 pr-4">Resolved At</th>
                </tr>
              </thead>
              <tbody>
                {data.firings.map((firing: FiringItem) => (
                  <tr key={firing.firing_id} className="modern-table-row">
                    <td className="py-2 pr-4">
                      <div className="flex flex-col gap-0.5">
                        <Badge tone={stateColor(firing.state)}>{firing.state}</Badge>
                        {firing.suppressed_by_rule_name && (
                          <span className="text-xs text-[var(--muted)]">
                            Suppressed by: {firing.suppressed_by_rule_name}
                          </span>
                        )}
                      </div>
                    </td>
                    <td className="py-2 pr-4 font-mono">
                      {firing.value != null ? firing.value.toFixed(2) : "—"}
                    </td>
                    <td className="py-2 pr-4 text-[var(--muted)]">
                      {formatTimestamp(isoToNs(firing.occurred_at), format)}
                    </td>
                    <td className="py-2 pr-4 text-[var(--muted)]">
                      {firing.resolved_at
                        ? formatTimestamp(isoToNs(firing.resolved_at), format)
                        : "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Panel>
    </section>
  );
}
