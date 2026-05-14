// Tenant and environment discovery API.
//
// These endpoints are outside the tenant-auth middleware on the backend, so
// no X-Tenant-ID or X-Api-Key header is required.  When authentication is
// introduced the backend will start filtering results by the caller's identity;
// the frontend call-sites need no structural change at that point.
//
// GET /v1/tenants                  → TenantListResponse
// GET /v1/tenants/:id/environments → EnvironmentListResponse

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
  const res = await fetch("/v1/tenants", { credentials: "include" });
  if (!res.ok) throw new Error(`listTenants failed: ${res.status}`);
  return res.json() as Promise<TenantListResponse>;
}

export async function listEnvironments(tenantId: string): Promise<EnvironmentListResponse> {
  const res = await fetch(`/v1/tenants/${encodeURIComponent(tenantId)}/environments`, { credentials: "include" });
  if (!res.ok) throw new Error(`listEnvironments failed: ${res.status}`);
  return res.json() as Promise<EnvironmentListResponse>;
}
