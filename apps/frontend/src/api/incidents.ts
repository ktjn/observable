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

export interface IncidentFindingItem {
  id: string;
  title: string;
  kind: "observed_fact" | "relationship" | "hypothesis" | "automated_suggestion" | "confirmed_cause";
  verified?: boolean;
  evidence_refs?: string[];
  summary?: string;
}

export interface IncidentDetailResponse {
  incident_id: string;
  title: string;
  severity: string;
  status: string;
  dedup_key: string;
  triggered_at: string;
  acknowledged_at?: string | null;
  acknowledged_by?: string | null;
  resolved_at: string | null;
  resolved_by?: string | null;
  triggered_by_rule_id: string | null;
  related_deployment_id?: string | null;
  runbook_url: string | null;
  rule_name: string | null;
  timeline: IncidentEventItem[];
  impacted_service: string | null;
  owner?: string | null;
  impact_summary?: string | null;
  affected_services?: string[];
  findings?: IncidentFindingItem[];
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

export async function acknowledgeIncident(
  tenantId: string,
  incidentId: string,
  payload?: { actor?: string; message?: string },
): Promise<IncidentDetailResponse> {
  const res = await fetch(`/v1/incidents/${incidentId}/acknowledge`, {
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify(payload ?? {}),
  });
  if (!res.ok) throw new Error(`Failed to acknowledge incident: ${res.status}`);
  return res.json();
}

export async function updateIncidentStatus(
  tenantId: string,
  incidentId: string,
  status: string,
  payload?: { actor?: string; message?: string },
): Promise<IncidentDetailResponse> {
  const res = await fetch(`/v1/incidents/${incidentId}/status`, {
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify({ status, ...payload }),
  });
  if (!res.ok) throw new Error(`Failed to update incident status: ${res.status}`);
  return res.json();
}

export async function resolveIncident(
  tenantId: string,
  incidentId: string,
  payload?: { actor?: string; message?: string },
): Promise<IncidentDetailResponse> {
  const res = await fetch(`/v1/incidents/${incidentId}/resolve`, {
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify(payload ?? {}),
  });
  if (!res.ok) throw new Error(`Failed to resolve incident: ${res.status}`);
  return res.json();
}

export async function addIncidentComment(
  tenantId: string,
  incidentId: string,
  message: string,
  actor?: string,
): Promise<IncidentDetailResponse> {
  const res = await fetch(`/v1/incidents/${incidentId}/comments`, {
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify({ message, actor }),
  });
  if (!res.ok) throw new Error(`Failed to add incident comment: ${res.status}`);
  return res.json();
}

export async function linkIncidentDeployment(
  tenantId: string,
  incidentId: string,
  deploymentId: string,
  payload?: { actor?: string; message?: string },
): Promise<IncidentDetailResponse> {
  const res = await fetch(`/v1/incidents/${incidentId}/link-deployment`, {
    method: "POST",
    headers: { ...tenantHeaders(tenantId), "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify({ deployment_id: deploymentId, ...payload }),
  });
  if (!res.ok) throw new Error(`Failed to link deployment: ${res.status}`);
  return res.json();
}
