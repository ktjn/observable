// Admin console — tenant member management API.
//
// GET    /v1/admin/members                    → MemberListResponse
// POST   /v1/admin/members                    → MemberRecord (201)
// PUT    /v1/admin/members/:userId/role       → 204
// DELETE /v1/admin/members/:userId            → 204
// POST   /v1/admin/members/:userId/revoke-sessions → 204

import type { Member as MemberRecord } from "./generated/admin/admin.Member.v1";
import { dispatch } from "../runtime/requestTransport";
import { HttpTransportError } from "../runtime/httpTransport";
export type { MemberRecord };

export interface MemberListResponse {
  members: MemberRecord[];
}

export type TenantRole = MemberRecord["role"];

export async function listMembers(_tenantId: string): Promise<MemberListResponse> {
  return dispatch<MemberListResponse>({ method: "GET", path: "/v1/admin/members" });
}

export async function addMember(
  _tenantId: string,
  body: { email: string; role: TenantRole },
): Promise<MemberRecord> {
  try {
    return await dispatch<MemberRecord>({ method: "POST", path: "/v1/admin/members", body });
  } catch (err) {
    if (err instanceof HttpTransportError && err.status === 404) throw new Error("EMAIL_NOT_FOUND");
    throw err;
  }
}

export async function updateMemberRole(
  _tenantId: string,
  userId: string,
  role: TenantRole,
): Promise<void> {
  try {
    await dispatch({
      method: "PUT",
      path: `/v1/admin/members/${encodeURIComponent(userId)}/role`,
      body: { role },
    });
  } catch (err) {
    if (err instanceof HttpTransportError && err.status === 403) throw new Error("SELF_DEMOTION");
    throw err;
  }
}

export async function removeMember(_tenantId: string, userId: string): Promise<void> {
  try {
    await dispatch({ method: "DELETE", path: `/v1/admin/members/${encodeURIComponent(userId)}` });
  } catch (err) {
    if (err instanceof HttpTransportError && err.status === 403) throw new Error("LAST_ADMIN");
    throw err;
  }
}

export async function revokeMemberSessions(_tenantId: string, userId: string): Promise<void> {
  await dispatch({
    method: "POST",
    path: `/v1/admin/members/${encodeURIComponent(userId)}/revoke-sessions`,
  });
}
