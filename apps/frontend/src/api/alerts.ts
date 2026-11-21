import type { AlertRule as AlertRuleItem } from "./generated/alerts/alerts.AlertRule.v1";
import type { Firing as FiringItem } from "./generated/alerts/alerts.Firing.v1";
import { dispatch } from "../runtime/requestTransport";

function tenantHeaders(_tenantId: string): HeadersInit {
  return {};
}

export type { AlertRuleItem };

export interface AlertRuleListResponse {
  items: AlertRuleItem[];
}

export interface CreateRuleResponse {
  rule_id: string;
}

export interface CreateRuleRequest {
  name: string;
  metric_name: string;
  operator: string;
  threshold: number;
  notification_channels?: string[];
  auto_trigger_incident?: boolean;
  runbook_url?: string;
  alert_type?: "threshold" | "deadman" | "change_detection";
  service_name?: string;
  window_secs?: number;
  baseline_offset_secs?: number;
  threshold_percent?: number;
}

export async function listAlertRules(_tenantId: string): Promise<AlertRuleListResponse> {
  return dispatch<AlertRuleListResponse>({ method: "GET", path: "/v1/alerts/rules" });
}

export async function createAlertRule(
  _tenantId: string,
  req: CreateRuleRequest,
): Promise<CreateRuleResponse> {
  return dispatch<CreateRuleResponse>({ method: "POST", path: "/v1/admin/alerts/rules", body: req });
}

export async function silenceAlertRule(
  _tenantId: string,
  ruleId: string,
  silenced: boolean,
): Promise<void> {
  await dispatch({
    method: "PATCH",
    path: `/v1/admin/alerts/rules/${ruleId}/silence`,
    body: { silenced },
  });
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
  runbook_reviewed_at?: string | null;
  guidance_status?: "current" | "missing" | "past_review";
  guidance_flagged_to_owner?: boolean;
  owner_kind?: string | null;
  owner_id?: string | null;
}

export function assessRunbookGuidance(rule: {
  severity: string;
  runbook_url: string | null;
  runbook_reviewed_at?: string | null;
  guidance_status?: "current" | "missing" | "past_review";
  guidance_flagged_to_owner?: boolean;
}): { status: "current" | "missing" | "past_review"; flaggedToOwner: boolean } {
  if (rule.guidance_status && typeof rule.guidance_flagged_to_owner === "boolean") {
    return { status: rule.guidance_status, flaggedToOwner: rule.guidance_flagged_to_owner };
  }
  const isCritical = rule.severity.toLowerCase() === "critical";
  if (!rule.runbook_url?.trim()) {
    return { status: "missing", flaggedToOwner: isCritical };
  }
  if (!rule.runbook_reviewed_at) {
    return { status: "past_review", flaggedToOwner: isCritical };
  }
  const ageDays = (Date.now() - Date.parse(rule.runbook_reviewed_at)) / 86_400_000;
  if (!Number.isFinite(ageDays) || ageDays > 90) {
    return { status: "past_review", flaggedToOwner: isCritical };
  }
  return { status: "current", flaggedToOwner: false };
}

export async function getAlertRule(
  _tenantId: string,
  ruleId: string,
): Promise<AlertRuleDetailResponse> {
  return dispatch<AlertRuleDetailResponse>({ method: "GET", path: `/v1/alerts/rules/${ruleId}` });
}

export async function setAlertRuleRunbook(
  _tenantId: string,
  ruleId: string,
  runbookUrl: string | null,
): Promise<void> {
  await dispatch({
    method: "PATCH",
    path: `/v1/admin/alerts/rules/${ruleId}/runbook`,
    body: { runbook_url: runbookUrl },
  });
}

export async function updateAlertRuleServiceName(
  tenantId: string,
  ruleId: string,
  serviceName: string | null,
): Promise<void> {
  const res = await fetch(`/v1/admin/alerts/rules/${ruleId}`, {
    credentials: "include",
    method: "PATCH",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify({ service_name: serviceName }),
  });
  if (!res.ok) throw new Error(`Failed to update alert rule: ${res.status}`);
}
