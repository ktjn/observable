// Tenant and environment discovery API.
//
// These endpoints are outside the tenant-auth middleware on the backend, so
// no X-Api-Key header is required.  When authenticated via session cookie,
// the backend filters results by the caller's identity.
//
// GET /v1/tenants                  → TenantListResponse
// GET /v1/tenants/:id/environments → EnvironmentListResponse

import { dispatch } from "../runtime/requestTransport";

export interface TenantRecord {
  id: string;
  name: string;
}

export interface TenantListResponse {
  tenants: TenantRecord[];
}

export interface EnvironmentRecord {
  environment: string;
}

export interface EnvironmentListResponse {
  environments: EnvironmentRecord[];
}

export async function listTenants(): Promise<TenantListResponse> {
  return dispatch<TenantListResponse>({ method: "GET", path: "/v1/tenants" });
}

export async function listEnvironments(tenantId: string): Promise<EnvironmentListResponse> {
  return dispatch<EnvironmentListResponse>({
    method: "GET",
    path: `/v1/tenants/${encodeURIComponent(tenantId)}/environments`,
  });
}
