export interface TenantMembership {
  tenant_id: string;
  role: string;
}

export interface ElevatedAccessState {
  active: boolean;
  scope?: string;
  justification?: string;
  expires_at?: string;
  grant_id?: string;
}

export interface MeResponse {
  user_id: string;
  email: string;
  active_tenant_id?: string;
  role?: string;
  tenants: TenantMembership[];
  elevated_access?: ElevatedAccessState | null;
  impersonating?: string | null;
}

export async function me(): Promise<MeResponse> {
  const res = await fetch("/v1/auth/me", { credentials: "include" });
  if (!res.ok) throw new Error(`me() failed: ${res.status}`);
  return res.json() as Promise<MeResponse>;
}

export interface SwitchTenantResponse {
  tenant_id: string;
  role: string;
}

export async function switchTenant(tenantId: string): Promise<SwitchTenantResponse> {
  const res = await fetch("/v1/auth/switch-tenant", {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ tenant_id: tenantId }),
  });
  if (!res.ok) throw new Error(`switchTenant failed: ${res.status}`);
  return res.json() as Promise<SwitchTenantResponse>;
}

export function initiateLogin(): void {
  window.location.href = "/v1/auth/login";
}

export async function logout(): Promise<void> {
  const res = await fetch("/v1/auth/logout", {
    method: "POST",
    credentials: "include",
  });
  if (!res.ok) throw new Error(`logout failed: ${res.status}`);
}
