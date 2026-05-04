import { LOCAL_DEV_API_KEY, LOCAL_DEV_TENANT_ID } from "./setup";

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

const HEADERS = {
  "Content-Type": "application/json",
  "x-api-key": LOCAL_DEV_API_KEY,
  "X-Tenant-ID": LOCAL_DEV_TENANT_ID,
};

export async function listTokens(): Promise<TokenListResponse> {
  const res = await fetch("/v1/tokens", { headers: HEADERS });
  if (!res.ok) throw new Error(`listTokens failed: ${res.status}`);
  return res.json() as Promise<TokenListResponse>;
}

export async function createToken(body: CreateTokenRequest): Promise<CreateTokenResponse> {
  const res = await fetch("/v1/tokens", {
    method: "POST",
    headers: HEADERS,
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`createToken failed: ${res.status}`);
  return res.json() as Promise<CreateTokenResponse>;
}

export async function revokeToken(id: string): Promise<void> {
  const res = await fetch(`/v1/tokens/${id}`, {
    method: "DELETE",
    headers: HEADERS,
  });
  if (!res.ok && res.status !== 204) throw new Error(`revokeToken failed: ${res.status}`);
}

export async function renewToken(id: string): Promise<CreateTokenResponse> {
  const res = await fetch(`/v1/tokens/${id}/renew`, {
    method: "POST",
    headers: HEADERS,
  });
  if (!res.ok) throw new Error(`renewToken failed: ${res.status}`);
  return res.json() as Promise<CreateTokenResponse>;
}

export async function restoreToken(id: string): Promise<void> {
  const res = await fetch(`/v1/tokens/${id}/restore`, {
    method: "POST",
    headers: HEADERS,
  });
  if (!res.ok && res.status !== 204) throw new Error(`restoreToken failed: ${res.status}`);
}

export async function deleteToken(id: string): Promise<void> {
  const res = await fetch(`/v1/tokens/${id}/permanent`, {
    method: "DELETE",
    headers: HEADERS,
  });
  if (!res.ok && res.status !== 204) throw new Error(`deleteToken failed: ${res.status}`);
}
