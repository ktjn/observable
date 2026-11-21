import { LOCAL_DEV_API_KEY } from "./setup";
import { dispatch } from "../runtime/requestTransport";

export interface TokenRecord {
  id: string;
  name: string;
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

function makeHeaders(_tenantId: string): Record<string, string> {
  return {
    "Content-Type": "application/json",
    "x-api-key": LOCAL_DEV_API_KEY,
  };
}

export async function listTokens(tenantId: string): Promise<TokenListResponse> {
  return dispatch<TokenListResponse>({ method: "GET", path: "/v1/tokens", headers: makeHeaders(tenantId) });
}

export async function createToken(tenantId: string, body: CreateTokenRequest): Promise<CreateTokenResponse> {
  return dispatch<CreateTokenResponse>({
    method: "POST",
    path: "/v1/tokens",
    body,
    headers: makeHeaders(tenantId),
  });
}

export async function revokeToken(tenantId: string, id: string): Promise<void> {
  await dispatch({ method: "DELETE", path: `/v1/tokens/${id}`, headers: makeHeaders(tenantId) });
}

export async function renewToken(tenantId: string, id: string): Promise<CreateTokenResponse> {
  return dispatch<CreateTokenResponse>({
    method: "POST",
    path: `/v1/tokens/${id}/renew`,
    headers: makeHeaders(tenantId),
  });
}

export async function restoreToken(tenantId: string, id: string): Promise<void> {
  await dispatch({ method: "POST", path: `/v1/tokens/${id}/restore`, headers: makeHeaders(tenantId) });
}

export async function deleteToken(tenantId: string, id: string): Promise<void> {
  await dispatch({ method: "DELETE", path: `/v1/tokens/${id}/permanent`, headers: makeHeaders(tenantId) });
}
