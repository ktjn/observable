import type { DeploymentMarker } from "./deployments";
import type { IncidentItem } from "./incidents";
import type { SloDefinitionItem } from "./slos";

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

function msToIso(ms?: number): string | undefined {
  return ms != null ? new Date(ms).toISOString() : undefined;
}

export interface IncidentSummary {
  total: number;
  open: number;
  resolved: number;
  mean_time_to_resolve_minutes: number | null;
}

export interface SloSummary {
  total: number;
  firing: number;
}

export interface DeploymentSummary {
  total: number;
}

export interface ServiceReliabilityReportResponse {
  service_name: string;
  environment: string | null;
  from: string;
  to: string;
  incident_summary: IncidentSummary;
  slo_summary: SloSummary;
  deployment_summary: DeploymentSummary;
  incidents: IncidentItem[];
  slos: SloDefinitionItem[];
  deployments: DeploymentMarker[];
}

export async function getServiceReliabilityReport(
  tenantId: string,
  serviceName: string,
  params: {
    environment?: string;
    from?: number;
    to?: number;
  },
): Promise<ServiceReliabilityReportResponse> {
  const url = new URL(
    `/v1/services/${encodeURIComponent(serviceName)}/reliability-report`,
    window.location.origin,
  );
  if (params.environment) url.searchParams.set("environment", params.environment);
  const fromIso = msToIso(params.from);
  const toIso = msToIso(params.to);
  if (fromIso) url.searchParams.set("from", fromIso);
  if (toIso) url.searchParams.set("to", toIso);

  const res = await fetch(url.toString(), {
    credentials: "include",
    headers: tenantHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
