const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function tenantHeaders(): HeadersInit {
  return { "X-Tenant-ID": DEV_TENANT_ID };
}

export interface AlertRuleItem {
  rule_id: string;
  name: string;
  metric_name: string;
  operator: "gt" | "gte" | "lt" | "lte" | "eq";
  threshold: number;
  severity: string;
  silenced: boolean;
  firing: boolean;
  last_fired_at: string | null;
}

export interface AlertRuleListResponse {
  items: AlertRuleItem[];
}

export interface CreateRuleRequest {
  name: string;
  metric_name: string;
  operator: string;
  threshold: number;
}

export async function listAlertRules(): Promise<AlertRuleListResponse> {
  const res = await fetch("/v1/alerts/rules", { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Failed to list alert rules: ${res.status}`);
  return res.json();
}

export async function createAlertRule(
  req: CreateRuleRequest,
): Promise<AlertRuleItem> {
  const res = await fetch("/v1/alerts/rules", {
    method: "POST",
    headers: { ...tenantHeaders(), "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
  if (!res.ok) throw new Error(`Failed to create alert rule: ${res.status}`);
  return res.json();
}

export async function silenceAlertRule(
  ruleId: string,
  silenced: boolean,
): Promise<AlertRuleItem> {
  const res = await fetch(`/v1/alerts/rules/${ruleId}/silence`, {
    method: "PATCH",
    headers: { ...tenantHeaders(), "Content-Type": "application/json" },
    body: JSON.stringify({ silenced }),
  });
  if (!res.ok) throw new Error(`Failed to update alert rule: ${res.status}`);
  return res.json();
}
