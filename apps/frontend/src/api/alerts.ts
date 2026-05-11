function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export interface AlertRuleItem {
  rule_id: string;
  name: string;
  metric_name: string;
  operator: "gt" | "gte" | "lt" | "lte" | "eq";
  threshold: number;
  severity: string;
  silenced: boolean;
  state: "ok" | "pending" | "active" | "resolved" | "silenced";
  firing: boolean;
  last_fired_at: string | null;
  notification_channels: string[];
  auto_trigger_incident: boolean;
}

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
}

export async function listAlertRules(tenantId: string): Promise<AlertRuleListResponse> {
  const res = await fetch("/v1/alerts/rules", { headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Failed to list alert rules: ${res.status}`);
  return res.json();
}

export async function createAlertRule(
  tenantId: string,
  req: CreateRuleRequest,
): Promise<AlertRuleItem> {
  const res = await fetch("/v1/alerts/rules", {
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
    method: "PATCH",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    body: JSON.stringify({ silenced }),
  });
  if (!res.ok) throw new Error(`Failed to update alert rule: ${res.status}`);
  return res.json();
}
