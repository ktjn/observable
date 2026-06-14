import type { Incident as IncidentItem } from "./generated/incidents/incidents.Incident.v1";
import type { IncidentEvent as IncidentEventItem } from "./generated/incidents/incidents.IncidentEvent.v1";

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export type { IncidentItem };

export interface IncidentListResponse {
  items: IncidentItem[];
}

export type { IncidentEventItem };

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
  rule_name: string | null;
  timeline: IncidentEventItem[];
  impacted_service: string | null;
}

export async function listIncidents(
  tenantId: string,
  status?: string,
): Promise<IncidentListResponse> {
  const url = status ? `/v1/incidents?status=${status}` : "/v1/incidents";
  const res = await fetch(url, { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Failed to list incidents: ${res.status}`);
  return res.json();
}

export async function getIncident(
  tenantId: string,
  incidentId: string,
): Promise<IncidentDetailResponse> {
  const res = await fetch(`/v1/incidents/${incidentId}`, {
    credentials: "include",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Failed to get incident: ${res.status}`);
  return res.json();
}
