export interface TenantMembership {
  tenant_id: string;
  role: string;
}

export interface MeResponse {
  user_id: string;
  email: string;
  tenants: TenantMembership[];
}

export async function me(): Promise<MeResponse> {
  const res = await fetch("/v1/auth/me", { credentials: "include" });
  if (!res.ok) throw new Error(`me() failed: ${res.status}`);
  return res.json() as Promise<MeResponse>;
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
