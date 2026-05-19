import { LOCAL_DEV_API_KEY } from "./setup";

export interface TokenRecord {
  id: string;
  name: string;
  tenant_name: string;
  environment: string;
  created_at: string;
  revoked: boolean;
}

export interface TokenListResponse {
  tokens: TokenRecord[];
}

export interface CreateTokenRequest {
  name: string;
  environment: string;
}

export interface CreateTokenResponse extends TokenRecord {
  /** Plaintext token — returned once, never stored server-side. */
  plaintext: string;
}

function makeHeaders(tenantId: string): HeadersInit {
  return {
    "Content-Type": "application/json",
    "x-api-key": LOCAL_DEV_API_KEY,
    "X-Tenant-ID": tenantId,
  };
}

export async function listTokens(tenantId: string): Promise<TokenListResponse> {
  const res = await fetch("/v1/tokens", { credentials: "include", headers: makeHeaders(tenantId) });
  if (!res.ok) throw new Error(`listTokens failed: ${res.status}`);
  return res.json() as Promise<TokenListResponse>;
}

export async function createToken(tenantId: string, body: CreateTokenRequest): Promise<CreateTokenResponse> {
  const res = await fetch("/v1/tokens", {
    credentials: "include",
    method: "POST",
    headers: makeHeaders(tenantId),
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`createToken failed: ${res.status}`);
  return res.json() as Promise<CreateTokenResponse>;
}

export async function revokeToken(tenantId: string, id: string): Promise<void> {
  const res = await fetch(`/v1/tokens/${id}`, {
    credentials: "include",
    method: "DELETE",
    headers: makeHeaders(tenantId),
  });
  if (!res.ok && res.status !== 204) throw new Error(`revokeToken failed: ${res.status}`);
}

export async function renewToken(tenantId: string, id: string): Promise<CreateTokenResponse> {
  const res = await fetch(`/v1/tokens/${id}/renew`, {
    credentials: "include",
    method: "POST",
    headers: makeHeaders(tenantId),
  });
  if (!res.ok) throw new Error(`renewToken failed: ${res.status}`);
  return res.json() as Promise<CreateTokenResponse>;
}

export async function restoreToken(tenantId: string, id: string): Promise<void> {
  const res = await fetch(`/v1/tokens/${id}/restore`, {
    credentials: "include",
    method: "POST",
    headers: makeHeaders(tenantId),
  });
  if (!res.ok && res.status !== 204) throw new Error(`restoreToken failed: ${res.status}`);
}

export async function deleteToken(tenantId: string, id: string): Promise<void> {
  const res = await fetch(`/v1/tokens/${id}/permanent`, {
    credentials: "include",
    method: "DELETE",
    headers: makeHeaders(tenantId),
  });
  if (!res.ok && res.status !== 204) throw new Error(`deleteToken failed: ${res.status}`);
}
