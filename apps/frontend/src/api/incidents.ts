function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export interface IncidentItem {
  incident_id: string;
  title: string;
  severity: string;
  status: string;
  triggered_at: string;
  resolved_at: string | null;
  triggered_by_rule_id: string | null;
}

export interface IncidentListResponse {
  items: IncidentItem[];
}

export interface IncidentEventItem {
  event_time: string;
  event_type: string;
  actor: string;
  message: string | null;
}

export interface IncidentDetailResponse {
  incident_id: string;
  title: string;
  severity: string;
  status: string;
  dedup_key: string;
  triggered_at: string;
  resolved_at: string | null;
  triggered_by_rule_id: string | null;
  runbook_url: string | null;
  timeline: IncidentEventItem[];
}

export async function listIncidents(
  tenantId: string,
  status?: string,
): Promise<IncidentListResponse> {
  const url = status ? `/v1/incidents?status=${status}` : "/v1/incidents";
  const res = await fetch(url, { headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Failed to list incidents: ${res.status}`);
  return res.json();
}

export async function getIncident(
  tenantId: string,
  incidentId: string,
): Promise<IncidentDetailResponse> {
  const res = await fetch(`/v1/incidents/${incidentId}`, {
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Failed to get incident: ${res.status}`);
  return res.json();
}
