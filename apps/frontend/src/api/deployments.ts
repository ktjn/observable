const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function tenantHeaders(): HeadersInit {
  return { "X-Tenant-ID": DEV_TENANT_ID };
}

export interface DeploymentMarker {
  deployment_id: string;
  tenant_id: string;
  project_id: string | null;
  service_name: string;
  environment: string;
  service_version: string;
  status: "in_progress" | "success" | "failed" | "rolled_back";
  started_at: string;
  finished_at: string | null;
  deployed_by: string | null;
  commit_sha: string | null;
  rollback_of: string | null;
  metadata: Record<string, unknown> | null;
}

export interface ListDeploymentsResponse {
  items: DeploymentMarker[];
}

export interface ListDeploymentsParams {
  service_name?: string;
  environment?: string;
  start_time?: string;
  end_time?: string;
  limit?: number;
}

export async function listDeployments(
  params: ListDeploymentsParams = {},
): Promise<ListDeploymentsResponse> {
  const url = new URL("/v1/deployments", window.location.origin);
  if (params.service_name) url.searchParams.set("service_name", params.service_name);
  if (params.environment) url.searchParams.set("environment", params.environment);
  if (params.start_time) url.searchParams.set("start_time", params.start_time);
  if (params.end_time) url.searchParams.set("end_time", params.end_time);
  if (params.limit !== undefined) url.searchParams.set("limit", String(params.limit));

  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Deployments fetch failed: ${res.status}`);
  return res.json();
}
