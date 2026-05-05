function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
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
  tenantId: string,
  params: ListDeploymentsParams = {},
): Promise<ListDeploymentsResponse> {
  const url = new URL("/v1/deployments", window.location.origin);
  if (params.service_name) url.searchParams.set("service_name", params.service_name);
  if (params.environment) url.searchParams.set("environment", params.environment);
  if (params.start_time) url.searchParams.set("start_time", params.start_time);
  if (params.end_time) url.searchParams.set("end_time", params.end_time);
  if (params.limit !== undefined) url.searchParams.set("limit", String(params.limit));

  const res = await fetch(url.toString(), { headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Deployments fetch failed: ${res.status}`);
  return res.json();
}
