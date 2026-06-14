import type { AlertRule as AlertRuleItem } from "./generated/alerts/alerts.AlertRule.v1";
import type { Firing as FiringItem } from "./generated/alerts/alerts.Firing.v1";

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export type { AlertRuleItem };

export interface AlertRuleListResponse {
  items: AlertRuleItem[];
}

export interface CreateRuleRequest {
  name: string;
  metric_name: string;
  operator: string;
  threshold: number;
  notification_channels?: string[];
  auto_trigger_incident?: boolean;
  runbook_url?: string;
}

export async function listAlertRules(tenantId: string): Promise<AlertRuleListResponse> {
  const res = await fetch("/v1/alerts/rules", { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Failed to list alert rules: ${res.status}`);
  return res.json();
}

export async function createAlertRule(
  tenantId: string,
  req: CreateRuleRequest,
): Promise<AlertRuleItem> {
  const res = await fetch("/v1/alerts/rules", {
    credentials: "include",
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Failed to create alert rule: ${res.status}`);
  return res.json();
}

export async function silenceAlertRule(
  tenantId: string,
  ruleId: string,
  silenced: boolean,
): Promise<AlertRuleItem> {
  const res = await fetch(`/v1/alerts/rules/${ruleId}/silence`, {
    credentials: "include",
    method: "PATCH",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify({ silenced }),
  });
  if (!res.ok) throw new Error(`Failed to update alert rule: ${res.status}`);
  return res.json();
}

export type { FiringItem };

export interface AlertRuleDetailResponse {
  rule_id: string;
  name: string;
  severity: string;
  alert_type: string;
  condition: Record<string, unknown>;
  silenced: boolean;
  firing: boolean;
  firings: FiringItem[];
  runbook_url: string | null;
}

export async function getAlertRule(
  tenantId: string,
  ruleId: string,
): Promise<AlertRuleDetailResponse> {
  const res = await fetch(`/v1/alerts/rules/${ruleId}`, {
    credentials: "include",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Failed to get alert rule: ${res.status}`);
  return res.json();
}

export async function setAlertRuleRunbook(
  tenantId: string,
  ruleId: string,
  runbookUrl: string | null,
): Promise<void> {
  const res = await fetch(`/v1/alerts/rules/${ruleId}/runbook`, {
    credentials: "include",
    method: "PATCH",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify({ runbook_url: runbookUrl }),
  });
  if (!res.ok) throw new Error(`Failed to update runbook URL: ${res.status}`);
}
