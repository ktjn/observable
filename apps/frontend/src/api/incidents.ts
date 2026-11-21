import type { Incident as IncidentItem } from "./generated/incidents/incidents.Incident.v1";
import type { IncidentEvent as IncidentEventItem } from "./generated/incidents/incidents.IncidentEvent.v1";
import { dispatch } from "../runtime/requestTransport";

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
  _tenantId: string,
  status?: string,
): Promise<IncidentListResponse> {
  return dispatch<IncidentListResponse>({ method: "GET", path: "/v1/incidents", query: { status } });
}

export async function getIncident(
  _tenantId: string,
  incidentId: string,
): Promise<IncidentDetailResponse> {
  return dispatch<IncidentDetailResponse>({ method: "GET", path: `/v1/incidents/${incidentId}` });
}

export async function acknowledgeIncident(
  _tenantId: string,
  incidentId: string,
  payload?: { actor?: string; message?: string },
): Promise<IncidentDetailResponse> {
  return dispatch<IncidentDetailResponse>({
    method: "POST",
    path: `/v1/incidents/${incidentId}/acknowledge`,
    body: payload ?? {},
  });
}

export async function updateIncidentStatus(
  _tenantId: string,
  incidentId: string,
  status: string,
  payload?: { actor?: string; message?: string },
): Promise<IncidentDetailResponse> {
  return dispatch<IncidentDetailResponse>({
    method: "POST",
    path: `/v1/incidents/${incidentId}/status`,
    body: { status, ...payload },
  });
}

export async function resolveIncident(
  _tenantId: string,
  incidentId: string,
  payload?: { actor?: string; message?: string },
): Promise<IncidentDetailResponse> {
  return dispatch<IncidentDetailResponse>({
    method: "POST",
    path: `/v1/incidents/${incidentId}/resolve`,
    body: payload ?? {},
  });
}

export async function addIncidentComment(
  _tenantId: string,
  incidentId: string,
  message: string,
  actor?: string,
): Promise<IncidentDetailResponse> {
  return dispatch<IncidentDetailResponse>({
    method: "POST",
    path: `/v1/incidents/${incidentId}/comments`,
    body: { message, actor },
  });
}

export async function linkIncidentDeployment(
  _tenantId: string,
  incidentId: string,
  deploymentId: string,
  payload?: { actor?: string; message?: string },
): Promise<IncidentDetailResponse> {
  return dispatch<IncidentDetailResponse>({
    method: "POST",
    path: `/v1/incidents/${incidentId}/link-deployment`,
    body: { deployment_id: deploymentId, ...payload },
  });
}
