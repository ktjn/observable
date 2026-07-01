import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  listAlertRules,
  createAlertRule,
  silenceAlertRule,
  type AlertRuleItem,
  type CreateRuleRequest,
  type CreateRuleResponse,
} from "../../api/alerts";
import {
  createSlo,
  listSlos,
  type CreateSloRequest,
  type SloDefinitionItem,
} from "../../api/slos";
import {
  listNotificationChannels,
  type NotificationChannelItem,
} from "../../api/notifications";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import { Select, SelectOption } from "../../components/ui/select";
import { Badge } from "../../components/ui/badge";
import { EmptyState } from "../../components/ui/empty-state";
import { MetricCard } from "../../components/ui/metric-card";
import { Panel } from "../../components/ui/panel";
import { Toolbar } from "../../components/ui/toolbar";
import { Tabs } from "../../components/ui/tabs";
import { useTenantContext } from "../../hooks/useTenantContext";
import { NotificationChannelsList } from "./NotificationChannelsList";

export function AlertsPage() {
  const queryClient = useQueryClient();
  const { tenantId } = useTenantContext();
  const [ruleFilter, setRuleFilter] = useState<"all" | "firing" | "silenced" | "suppressed">("all");
  const [isCreating, setIsCreating] = useState(false);
  const [formName, setFormName] = useState("");
  const [formMetric, setFormMetric] = useState("");
  const [formOperator, setFormOperator] = useState("gt");
  const [formThreshold, setFormThreshold] = useState("");
  const [formAlertType, setFormAlertType] = useState<"threshold" | "deadman" | "change_detection">("threshold");
  const [formServiceName, setFormServiceName] = useState("");
  const [formWindowSecs, setFormWindowSecs] = useState("300");
  const [formCdMetric, setFormCdMetric] = useState("");
  const [formBaselineOffsetSecs, setFormBaselineOffsetSecs] = useState("3600");
  const [formThresholdPercent, setFormThresholdPercent] = useState("");
  const [selectedChannels, setSelectedChannels] = useState<string[]>([]);
  const [autoTriggerIncident, setAutoTriggerIncident] = useState(true);
  const [formRunbookUrl, setFormRunbookUrl] = useState("");
  const [formError, setFormError] = useState<string | null>(null);
  const [isCreatingSlo, setIsCreatingSlo] = useState(false);
  const [sloService, setSloService] = useState("");
  const [sloEnvironment, setSloEnvironment] = useState("");
  const [sloTarget, setSloTarget] = useState("99.9");
  const [sloDescription, setSloDescription] = useState("");
  const [sloFormError, setSloFormError] = useState<string | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ["alert-rules", tenantId],
    queryFn: () => listAlertRules(tenantId),
  });

  const { data: sloData, isLoading: isLoadingSlos } = useQuery({
    queryKey: ["slos", tenantId],
    queryFn: () => listSlos(tenantId),
  });

  const { data: channelsData } = useQuery({
    queryKey: ["notification-channels", tenantId],
    queryFn: () => listNotificationChannels(tenantId),
  });

  const silenceMutation = useMutation({
    mutationFn: ({ ruleId, silenced }: { ruleId: string; silenced: boolean }) =>
      silenceAlertRule(tenantId, ruleId, silenced),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["alert-rules", tenantId] }),
  });

  const createMutation = useMutation<CreateRuleResponse, Error, CreateRuleRequest>({
    mutationFn: (req: CreateRuleRequest) => createAlertRule(tenantId, req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["alert-rules", tenantId] });
      setIsCreating(false);
      setFormName("");
      setFormMetric("");
      setFormOperator("gt");
      setFormThreshold("");
      setFormAlertType("threshold");
      setFormServiceName("");
      setFormWindowSecs("300");
      setFormCdMetric("");
      setFormBaselineOffsetSecs("3600");
      setFormThresholdPercent("");
      setSelectedChannels([]);
      setAutoTriggerIncident(true);
      setFormRunbookUrl("");
      setFormError(null);
    },
    onError: (e: Error) => setFormError(e.message),
  });

  const createSloMutation = useMutation({
    mutationFn: (req: CreateSloRequest) => createSlo(tenantId, req),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["slos", tenantId] });
      setIsCreatingSlo(false);
      setSloService("");
      setSloEnvironment("");
      setSloTarget("99.9");
      setSloDescription("");
      setSloFormError(null);
    },
    onError: (e: Error) => setSloFormError(e.message),
  });

  const handleCreateSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (formAlertType === "change_detection") {
      const windowSecs = parseInt(formWindowSecs, 10);
      const baselineOffsetSecs = parseInt(formBaselineOffsetSecs, 10);
      const thresholdPercent = parseFloat(formThresholdPercent);
      if (!formCdMetric.trim()) {
        setFormError("Metric name is required");
        return;
      }
      if (!Number.isFinite(windowSecs) || windowSecs <= 0) {
        setFormError("Window must be a positive number of seconds");
        return;
      }
      if (!Number.isFinite(baselineOffsetSecs) || baselineOffsetSecs <= 0) {
        setFormError("Baseline offset must be a positive number of seconds");
        return;
      }
      if (!Number.isFinite(thresholdPercent) || thresholdPercent <= 0) {
        setFormError("Threshold percent must be a positive number");
        return;
      }
      setFormError(null);
      createMutation.mutate({
        name: formName,
        metric_name: formCdMetric.trim(),
        operator: "",
        threshold: 0,
        notification_channels: selectedChannels,
        auto_trigger_incident: autoTriggerIncident,
        runbook_url: formRunbookUrl || undefined,
        alert_type: "change_detection",
        window_secs: windowSecs,
        baseline_offset_secs: baselineOffsetSecs,
        threshold_percent: thresholdPercent,
      });
      return;
    }

    if (formAlertType === "deadman") {
      const windowSecs = parseInt(formWindowSecs, 10);
      if (!formServiceName.trim()) {
        setFormError("Service name is required");
        return;
      }
      if (isNaN(windowSecs) || windowSecs <= 0) {
        setFormError("Window must be a positive number of seconds");
        return;
      }
      setFormError(null);
      createMutation.mutate({
        name: formName,
        metric_name: "",
        operator: "",
        threshold: 0,
        notification_channels: selectedChannels,
        auto_trigger_incident: autoTriggerIncident,
        runbook_url: formRunbookUrl || undefined,
        alert_type: "deadman",
        service_name: formServiceName.trim(),
        window_secs: windowSecs,
      });
      return;
    }

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
      notification_channels: selectedChannels,
      auto_trigger_incident: autoTriggerIncident,
      runbook_url: formRunbookUrl || undefined,
      alert_type: "threshold",
    });
  };

  const handleCreateSloSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const targetPercentage = parseFloat(sloTarget);
    if (!Number.isFinite(targetPercentage) || targetPercentage <= 0 || targetPercentage >= 100) {
      setSloFormError("Target must be a percentage between 0 and 100");
      return;
    }
    setSloFormError(null);
    createSloMutation.mutate({
      service_name: sloService,
      environment: sloEnvironment,
      target: Number((targetPercentage / 100).toFixed(6)),
      window_days: 30,
      burn_rate_fast_threshold: 14.4,
      burn_rate_slow_threshold: 1.0,
      description: sloDescription || undefined,
    });
  };

  const rules = data?.items ?? [];
  const slos = sloData?.items ?? [];
  const channels = Array.isArray(channelsData) ? channelsData : [];
  const firingCount = rules.filter((r) => r.state === "active").length;
  const silencedCount = rules.filter((r) => r.silenced).length;
  const suppressedCount = rules.filter((r) => r.suppressed).length;
  const sloBreachCount = slos.filter((slo) => slo.firing).length;
  const filteredRules =
    ruleFilter === "firing"
      ? rules.filter((r) => r.state === "active")
      : ruleFilter === "silenced"
        ? rules.filter((r) => r.silenced)
        : ruleFilter === "suppressed"
          ? rules.filter((r) => r.suppressed)
          : rules;

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
        <MetricCard label="Silenced" value={silencedCount} tone={silencedCount > 0 ? "warn" : "info"} />
        <MetricCard label="SLOs" value={slos.length} tone={sloBreachCount > 0 ? "bad" : "info"} />
      </div>

      <Tabs.Root defaultValue="rules">
        <Tabs.List>
          <Tabs.Tab value="rules">Alert Rules</Tabs.Tab>
          <Tabs.Tab value="slos">SLOs</Tabs.Tab>
          <Tabs.Tab value="channels">Notification Channels</Tabs.Tab>
        </Tabs.List>

        <Tabs.Panel value="rules" className="space-y-4 pt-4">
          <div className="flex items-center gap-4">
            <div className="flex items-center gap-1" role="group" aria-label="Filter alert rules">
              {(["all", "firing", "silenced", "suppressed"] as const).map((f) => {
                const count =
                  f === "all" ? rules.length :
                  f === "firing" ? firingCount :
                  f === "silenced" ? silencedCount :
                  suppressedCount;
                const activeColor = f === "firing" ? "var(--bad)" : f === "silenced" ? "var(--warn)" : f === "suppressed" ? "var(--muted)" : "var(--brand)";
                const isActive = ruleFilter === f;
                return (
                  <button
                    key={f}
                    type="button"
                    onClick={() => setRuleFilter(f)}
                    className={[
                      "flex items-center gap-1.5 px-2.5 py-1 text-xs font-bold border rounded transition-colors",
                      isActive
                        ? ""
                        : "border-[var(--border)] text-[var(--muted)] hover:border-[var(--brand)] hover:text-[var(--text)]",
                    ].join(" ")}
                    style={isActive ? { borderColor: activeColor, color: activeColor } : undefined}
                  >
                    <span className="capitalize">{f === "all" ? "All" : f}</span>
                    <span aria-hidden="true">({count})</span>
                  </button>
                );
              })}
            </div>
            <Toolbar aria-label="Alert actions" className="ml-auto">
              <Button onClick={() => setIsCreating((v) => !v)}>
                {isCreating ? "Cancel" : "New Rule"}
              </Button>
            </Toolbar>
          </div>

          {isCreating && (
            <Panel title="Create Threshold Rule" eyebrow="Configuration">
              <form
                onSubmit={handleCreateSubmit}
                aria-label="Create alert rule"
                className="flex flex-col gap-3"
              >
                <div className="space-y-1">
                  <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="alert-type">Alert type</label>
                  <Select
                    id="alert-type"
                    value={formAlertType}
                    onChange={(e) =>
                      setFormAlertType(e.target.value as "threshold" | "deadman" | "change_detection")
                    }
                  >
                    <SelectOption value="threshold">Threshold metric</SelectOption>
                    <SelectOption value="deadman">No data</SelectOption>
                    <SelectOption value="change_detection">Change detection</SelectOption>
                  </Select>
                </div>

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

                {formAlertType === "threshold" ? (
                  <>
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
                  </>
                ) : formAlertType === "deadman" ? (
                  <div className="grid gap-3 sm:grid-cols-2">
                    <div className="space-y-1">
                      <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="deadman-service">Service name</label>
                      <Input
                        id="deadman-service"
                        placeholder="e.g. checkout"
                        value={formServiceName}
                        onChange={(e) => setFormServiceName(e.target.value)}
                        required
                      />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="deadman-window">Window (seconds)</label>
                      <Input
                        id="deadman-window"
                        type="number"
                        step="1"
                        min="1"
                        value={formWindowSecs}
                        onChange={(e) => setFormWindowSecs(e.target.value)}
                        required
                      />
                    </div>
                  </div>
                ) : (
                  <>
                    <div className="space-y-1">
                      <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="cd-metric">Metric name</label>
                      <Input
                        id="cd-metric"
                        placeholder="e.g. error_rate"
                        value={formCdMetric}
                        onChange={(e) => setFormCdMetric(e.target.value)}
                        required
                      />
                    </div>

                    <div className="grid gap-3 sm:grid-cols-2">
                      <div className="space-y-1">
                        <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="cd-window">Window (seconds)</label>
                        <Input
                          id="cd-window"
                          type="number"
                          step="1"
                          min="1"
                          value={formWindowSecs}
                          onChange={(e) => setFormWindowSecs(e.target.value)}
                          required
                        />
                      </div>
                      <div className="space-y-1">
                        <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="cd-baseline-offset">Baseline offset (seconds)</label>
                        <Input
                          id="cd-baseline-offset"
                          type="number"
                          step="1"
                          min="1"
                          value={formBaselineOffsetSecs}
                          onChange={(e) => setFormBaselineOffsetSecs(e.target.value)}
                          required
                        />
                      </div>
                    </div>

                    <div className="space-y-1">
                      <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="cd-threshold-percent">Threshold (%)</label>
                      <Input
                        id="cd-threshold-percent"
                        type="number"
                        step="any"
                        min="0"
                        value={formThresholdPercent}
                        onChange={(e) => setFormThresholdPercent(e.target.value)}
                        required
                      />
                    </div>
                  </>
                )}

                <div className="space-y-1">
                  <label className="text-xs font-bold uppercase text-[var(--muted)]">Notification channels</label>
                  <div className="flex flex-wrap gap-4 pt-1">
                    {channels.map((c) => (
                      <label key={c.channel_id} className="flex items-center gap-2 text-xs">
                        <input
                          type="checkbox"
                          checked={selectedChannels.includes(c.channel_id)}
                          onChange={(e) => {
                            if (e.target.checked) {
                              setSelectedChannels([...selectedChannels, c.channel_id]);
                            } else {
                              setSelectedChannels(selectedChannels.filter((id) => id !== c.channel_id));
                            }
                          }}
                        />
                        {c.name}
                      </label>
                    ))}
                    {channels.length === 0 && <div className="text-xs italic text-[var(--muted)]">No channels configured.</div>}
                  </div>
                </div>

                <label className="flex items-center gap-2 text-xs">
                  <input
                    type="checkbox"
                    checked={autoTriggerIncident}
                    onChange={(e) => setAutoTriggerIncident(e.target.checked)}
                  />
                  Auto-trigger incident on firing
                </label>

                <div className="space-y-1">
                  <label
                    className="text-xs font-bold uppercase text-[var(--muted)]"
                    htmlFor="runbook-url"
                  >
                    Runbook URL{" "}
                    <span className="font-normal normal-case text-[var(--muted)]">(optional)</span>
                  </label>
                  <Input
                    id="runbook-url"
                    type="url"
                    placeholder="https://..."
                    value={formRunbookUrl}
                    onChange={(e) => setFormRunbookUrl(e.target.value)}
                  />
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
          ) : filteredRules.length === 0 ? (
            <EmptyState
              title={`No ${ruleFilter} rules`}
              description="Try selecting a different filter."
            />
          ) : (
            <Panel title="Alert rules" eyebrow="Health and performance">
              <div className="overflow-x-auto">
                <table className="w-full text-left" aria-label="Alert rules">
                  <thead>
                    <tr>
                      <th className="pb-3 pr-4">Name</th>
                      <th className="pb-3 pr-4">Metric</th>
                      <th className="pb-3 pr-4">Condition</th>
                      <th className="pb-3 pr-4">Channels</th>
                      <th className="pb-3 pr-4">Severity</th>
                      <th className="pb-3 pr-4">Status</th>
                      <th className="pb-3">Action</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filteredRules.map((rule) => (
                      <AlertRuleRow
                        key={rule.rule_id}
                        rule={rule}
                        channels={channels}
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
        </Tabs.Panel>

        <Tabs.Panel value="slos" className="space-y-4 pt-4">
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-3" aria-label="SLO summary">
            <MetricCard label="Availability SLOs" value={slos.length} tone="info" />
            <MetricCard label="Burning" value={sloBreachCount} tone={sloBreachCount > 0 ? "bad" : "good"} />
            <MetricCard label="Within Budget" value={slos.length - sloBreachCount} tone="good" />
          </div>

          <Toolbar aria-label="SLO actions" className="justify-end">
            <Button variant="secondary" onClick={() => setIsCreatingSlo((v) => !v)}>
              {isCreatingSlo ? "Cancel SLO" : "New SLO"}
            </Button>
          </Toolbar>

          {isCreatingSlo && (
            <Panel title="Create Availability SLO" eyebrow="Reliability target">
              <form
                onSubmit={handleCreateSloSubmit}
                aria-label="Create SLO"
                className="flex flex-col gap-3"
              >
                <div className="grid gap-3 sm:grid-cols-2">
                  <div className="space-y-1">
                    <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="slo-service">SLO service</label>
                    <Input
                      id="slo-service"
                      value={sloService}
                      onChange={(e) => setSloService(e.target.value)}
                      required
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="slo-environment">SLO environment</label>
                    <Input
                      id="slo-environment"
                      value={sloEnvironment}
                      onChange={(e) => setSloEnvironment(e.target.value)}
                      required
                    />
                  </div>
                </div>

                <div className="grid gap-3 sm:grid-cols-2">
                  <div className="space-y-1">
                    <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="slo-target">SLO target</label>
                    <Input
                      id="slo-target"
                      type="number"
                      step="0.001"
                      value={sloTarget}
                      onChange={(e) => setSloTarget(e.target.value)}
                      required
                    />
                  </div>
                  <div className="space-y-1">
                    <label className="text-xs font-bold uppercase text-[var(--muted)]" htmlFor="slo-description">SLO description</label>
                    <Input
                      id="slo-description"
                      value={sloDescription}
                      onChange={(e) => setSloDescription(e.target.value)}
                    />
                  </div>
                </div>

                {sloFormError && (
                  <div role="alert" className="text-sm font-bold text-[var(--bad)]">
                    {sloFormError}
                  </div>
                )}

                <div className="flex gap-2 pt-2">
                  <Button type="submit" disabled={createSloMutation.isPending}>
                    {createSloMutation.isPending ? "Creating..." : "Create SLO"}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    onClick={() => {
                      setIsCreatingSlo(false);
                      setSloFormError(null);
                    }}
                  >
                    Cancel
                  </Button>
                </div>
              </form>
            </Panel>
          )}

          {isLoadingSlos ? (
            <Panel>
              <div className="py-8 text-center text-[var(--muted)]">Loading SLOs...</div>
            </Panel>
          ) : slos.length > 0 ? (
            <Panel title="SLO health" eyebrow="Error budget">
              <div className="grid gap-3 lg:grid-cols-2">
                {slos.map((slo) => (
                  <SloHealthCard key={slo.slo_id} slo={slo} />
                ))}
              </div>
            </Panel>
          ) : (
            <EmptyState
              title="No SLOs"
              description="Define a service-level objective to track your reliability."
            />
          )}
        </Tabs.Panel>

        <Tabs.Panel value="channels" className="pt-4">
          <NotificationChannelsList />
        </Tabs.Panel>
      </Tabs.Root>
    </section>
  );
}

function SloHealthCard({ slo }: { slo: SloDefinitionItem }) {
  const targetPct = slo.target * 100;
  const target = formatPercent(slo.target);
  const status = slo.firing
    ? { label: "Burning", tone: "bad" as const }
    : { label: "Within budget", tone: "good" as const };
  const barColor = slo.firing
    ? "var(--bad)"
    : targetPct >= 99.9
      ? "var(--good)"
      : targetPct >= 95
        ? "var(--warn)"
        : "var(--bad)";

  return (
    <article className="border border-[var(--border)] bg-[var(--surface)] p-3">
      <div className="mb-2 flex items-start justify-between gap-3">
        <div>
          <div className="text-sm font-bold text-[var(--text-strong)]">
            {slo.description || `${slo.service_name} availability`}
          </div>
          <div className="mt-1 flex flex-wrap gap-1 text-xs text-[var(--muted)]">
            <span>{slo.service_name}</span>
            <span aria-hidden="true">·</span>
            <span>{slo.environment}</span>
          </div>
        </div>
        <Badge tone={status.tone}>{status.label}</Badge>
      </div>
      <div className="mb-3 flex items-center gap-2">
        <div
          className="h-1.5 flex-1 overflow-hidden rounded-full"
          style={{ backgroundColor: "var(--border)" }}
          role="progressbar"
          aria-valuenow={targetPct}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label={`SLO target ${target}`}
        >
          <div
            className="h-full rounded-full"
            style={{ width: `${targetPct}%`, backgroundColor: barColor }}
          />
        </div>
        <span className="text-xs tabular-nums font-bold" style={{ color: barColor }}>
          {target}
        </span>
      </div>
      <dl className="grid grid-cols-2 gap-3 text-xs">
        <div>
          <dt className="field-label">Window</dt>
          <dd className="m-0 font-bold text-[var(--text-strong)]">{slo.window_days}d</dd>
        </div>
        <div>
          <dt className="field-label">Fast burn</dt>
          <dd className="m-0 font-bold text-[var(--text-strong)]">{slo.burn_rate_fast_threshold}x</dd>
        </div>
        <div>
          <dt className="field-label">Slow burn</dt>
          <dd className="m-0 font-bold text-[var(--text-strong)]">{slo.burn_rate_slow_threshold}x</dd>
        </div>
      </dl>
    </article>
  );
}

function formatPercent(value: number) {
  return `${Number((value * 100).toFixed(3))}%`;
}

function AlertRuleRow({
  rule,
  channels,
  onToggleSilence,
}: {
  rule: AlertRuleItem;
  channels: NotificationChannelItem[];
  onToggleSilence: () => void;
}) {
  const conditionLabel =
    (rule.operator as string) === "no_data"
      ? `No data for ${rule.threshold}s from ${rule.metric_name}`
      : `${rule.operator} ${rule.threshold}`;
  const status = alertStatus(rule);
  const channelNames = (rule.notification_channels ?? [])
    .map((id) => channels.find((c) => c.channel_id === id)?.name)
    .filter(Boolean)
    .join(", ");

  const rowClass = [
    "modern-table-row border-l-2",
    rule.state === "active"
      ? "border-l-[var(--bad)]"
      : rule.silenced
        ? "border-l-[var(--warn)]"
        : "border-l-transparent",
  ].join(" ");

  return (
    <tr className={rowClass}>
      <td className="py-3 pr-4 font-bold text-[var(--text-strong)]">{rule.name}</td>
      <td className="py-3 pr-4">{rule.metric_name}</td>
      <td className="py-3 pr-4">{conditionLabel}</td>
      <td className="py-3 pr-4 text-xs text-[var(--muted)]">
        {channelNames || "None"}
      </td>
      <td className="py-3 pr-4">
        <Badge tone={severityTone(rule.severity)}>{rule.severity}</Badge>
      </td>
      <td className="py-3 pr-4">
        <div className="flex items-center gap-1">
          <Badge tone={status.tone}>{status.label}</Badge>
          {rule.suppressed && (
            <Badge tone="neutral">Suppressed</Badge>
          )}
        </div>
      </td>
      <td className="py-3">
        <Button variant="ghost" onClick={onToggleSilence} className="h-8 py-0">
          {rule.silenced ? "Unsilence" : "Silence"}
        </Button>
      </td>
    </tr>
  );
}

function severityTone(severity: string): "good" | "warn" | "bad" | "info" | "neutral" {
  switch (severity?.toLowerCase()) {
    case "critical":
      return "bad";
    case "warning":
    case "warn":
      return "warn";
    case "info":
      return "info";
    default:
      return "neutral";
  }
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
    case "suppressed":
      return { label: "Suppressed", tone: "neutral" };
    case "ok":
    default:
      return { label: "OK", tone: "good" };
  }
}
