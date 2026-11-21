import { dispatch } from "../runtime/requestTransport";

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
  _tenantId: string,
  params: ListDeploymentsParams = {},
): Promise<ListDeploymentsResponse> {
  return dispatch<ListDeploymentsResponse>({
    method: "GET",
    path: "/v1/deployments",
    query: {
      service_name: params.service_name,
      environment: params.environment,
      start_time: params.start_time,
      end_time: params.end_time,
      limit: params.limit,
    },
  });
}
