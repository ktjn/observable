import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  listAlertRules,
  createAlertRule,
  silenceAlertRule,
  type AlertRuleItem,
  type CreateRuleRequest,
} from "../../api/alerts";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { Select, SelectOption } from "../../components/ui/select";
import { Badge } from "../../components/ui/badge";
import { EmptyState } from "../../components/ui/empty-state";
import { MetricCard } from "../../components/ui/metric-card";
import { Panel } from "../../components/ui/panel";
import { Toolbar } from "../../components/ui/toolbar";
import { useTenantContext } from "../../hooks/useTenantContext";

export function AlertsPage() {
  const queryClient = useQueryClient();
  const { tenantId } = useTenantContext();
  const [isCreating, setIsCreating] = useState(false);
  const [formName, setFormName] = useState("");
  const [formMetric, setFormMetric] = useState("");
  const [formOperator, setFormOperator] = useState("gt");
  const [formThreshold, setFormThreshold] = useState("");
  const [formError, setFormError] = useState<string | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ["alert-rules", tenantId],
    queryFn: () => listAlertRules(tenantId),
  });

  const silenceMutation = useMutation({
    mutationFn: ({ ruleId, silenced }: { ruleId: string; silenced: boolean }) =>
      silenceAlertRule(tenantId, ruleId, silenced),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["alert-rules", tenantId] }),
  });

  const createMutation = useMutation({
    mutationFn: (req: CreateRuleRequest) => createAlertRule(tenantId, req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["alert-rules", tenantId] });
      setIsCreating(false);
      setFormName("");
      setFormMetric("");
      setFormOperator("gt");
      setFormThreshold("");
      setFormError(null);
    },
    onError: (e: Error) => setFormError(e.message),
  });

  const handleCreateSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const threshold = parseFloat(formThreshold);
    if (isNaN(threshold)) {
      setFormError("Threshold must be a number");
      return;
    }
    setFormError(null);
    createMutation.mutate({
      name: formName,
      metric_name: formMetric,
      operator: formOperator,
      threshold,
    });
  };

  const rules = data?.items ?? [];
  const firingCount = rules.filter((r) => r.state === "active").length;
  const pendingCount = rules.filter((r) => r.state === "pending").length;
  const silencedCount = rules.filter((r) => r.silenced).length;

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Reliability</div>
          <h1>Alerts &amp; SLOs</h1>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-4" aria-label="Alert summary">
        <MetricCard label="Total Rules" value={rules.length} tone="info" />
        <MetricCard label="Firing" value={firingCount} tone={firingCount > 0 ? "bad" : "good"} />
        <MetricCard label="Pending" value={pendingCount} tone="warn" />
        <MetricCard label="Silenced" value={silencedCount} tone="warn" />
      </div>

      <Toolbar aria-label="Alert actions" className="justify-end">
        <Button onClick={() => setIsCreating((v) => !v)}>
          {isCreating ? "Cancel" : "New Rule"}
        </Button>
      </Toolbar>

      {isCreating && (
        <Panel title="Create Threshold Rule" eyebrow="Configuration">
          <form
            onSubmit={handleCreateSubmit}
            aria-label="Create alert rule"
            className="flex flex-col gap-3"
          >
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1">
                <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="rule-name">Rule name</label>
                <Input
                  id="rule-name"
                  placeholder="e.g. High Error Rate"
                  value={formName}
                  onChange={(e) => setFormName(e.target.value)}
                  required
                />
              </div>
              <div className="space-y-1">
                <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="metric-name">Metric name</label>
                <Input
                  id="metric-name"
                  placeholder="e.g. error_rate"
                  value={formMetric}
                  onChange={(e) => setFormMetric(e.target.value)}
                  required
                />
              </div>
            </div>

            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1">
                <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="operator">Operator</label>
                <Select
                  id="operator"
                  value={formOperator}
                  onChange={(e) => setFormOperator(e.target.value)}
                >
                  <SelectOption value="gt">&gt; (greater than)</SelectOption>
                  <SelectOption value="gte">&ge; (greater than or equal)</SelectOption>
                  <SelectOption value="lt">&lt; (less than)</SelectOption>
                  <SelectOption value="lte">&le; (less than or equal)</SelectOption>
                  <SelectOption value="eq">= (equal)</SelectOption>
                </Select>
              </div>
              <div className="space-y-1">
                <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="threshold">Threshold value</label>
                <Input
                  id="threshold"
                  type="number"
                  step="any"
                  value={formThreshold}
                  onChange={(e) => setFormThreshold(e.target.value)}
                  required
                />
              </div>
            </div>

            {formError && (
              <div role="alert" className="text-sm font-bold text-[var(--bad)]">
                {formError}
              </div>
            )}

            <div className="flex gap-2 pt-2">
              <Button type="submit" disabled={createMutation.isPending}>
                {createMutation.isPending ? "Creating…" : "Create Rule"}
              </Button>
              <Button
                type="button"
                variant="ghost"
                onClick={() => {
                  setIsCreating(false);
                  setFormError(null);
                }}
              >
                Cancel
              </Button>
            </div>
          </form>
        </Panel>
      )}

      {isLoading ? (
        <Panel>
          <div className="py-8 text-center text-[var(--muted)]">Loading alert rules…</div>
        </Panel>
      ) : rules.length === 0 ? (
        <EmptyState
          title="No alert rules"
          description="Create a threshold rule to start monitoring metrics."
        />
      ) : (
        <Panel title="Active alert rules" eyebrow="Health and performance">
          <div className="overflow-x-auto">
            <table className="w-full text-left">
              <thead>
                <tr>
                  <th className="pb-3 pr-4">Name</th>
                  <th className="pb-3 pr-4">Metric</th>
                  <th className="pb-3 pr-4">Condition</th>
                  <th className="pb-3 pr-4">Severity</th>
                  <th className="pb-3 pr-4">Status</th>
                  <th className="pb-3">Action</th>
                </tr>
              </thead>
              <tbody>
                {rules.map((rule) => (
                  <AlertRuleRow
                    key={rule.rule_id}
                    rule={rule}
                    onToggleSilence={() =>
                      silenceMutation.mutate({
                        ruleId: rule.rule_id,
                        silenced: !rule.silenced,
                      })
                    }
                  />
                ))}
              </tbody>
            </table>
          </div>
        </Panel>
      )}
    </section>
  );
}

function AlertRuleRow({
  rule,
  onToggleSilence,
}: {
  rule: AlertRuleItem;
  onToggleSilence: () => void;
}) {
  const conditionLabel = `${rule.operator} ${rule.threshold}`;
  const status = alertStatus(rule);

  return (
    <tr className="modern-table-row">
      <td className="py-3 pr-4 font-bold text-[var(--text-strong)]">{rule.name}</td>
      <td className="py-3 pr-4">{rule.metric_name}</td>
      <td className="py-3 pr-4">{conditionLabel}</td>
      <td className="py-3 pr-4">
        <Badge tone="neutral">{rule.severity}</Badge>
      </td>
      <td className="py-3 pr-4">
        <Badge tone={status.tone}>{status.label}</Badge>
      </td>
      <td className="py-3">
        <Button variant="ghost" onClick={onToggleSilence} className="h-8 py-0">
          {rule.silenced ? "Unsilence" : "Silence"}
        </Button>
      </td>
    </tr>
  );
}

function alertStatus(rule: AlertRuleItem): {
  label: string;
  tone: "good" | "warn" | "bad" | "info" | "neutral";
} {
  switch (rule.state) {
    case "active":
      return { label: "Firing", tone: "bad" };
    case "pending":
      return { label: "Pending", tone: "warn" };
    case "resolved":
      return { label: "Resolved", tone: "good" };
    case "silenced":
      return { label: "Silenced", tone: "neutral" };
    case "ok":
    default:
      return { label: "OK", tone: "good" };
  }
}
