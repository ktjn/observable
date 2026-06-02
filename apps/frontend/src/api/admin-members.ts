// Admin console — tenant member management API.
//
// GET    /v1/admin/members                    → MemberListResponse
// POST   /v1/admin/members                    → MemberRecord (201)
// PUT    /v1/admin/members/:userId/role       → 204
// DELETE /v1/admin/members/:userId            → 204
// POST   /v1/admin/members/:userId/revoke-sessions → 204

export interface MemberRecord {
  user_id: string;
  email: string;
  name: string | null;
  role: "tenant_admin" | "member" | "viewer";
  joined_at: string;
}

export interface MemberListResponse {
  members: MemberRecord[];
}

export type TenantRole = "tenant_admin" | "member" | "viewer";

export async function listMembers(tenantId: string): Promise<MemberListResponse> {
  const res = await fetch("/v1/admin/members", {
    credentials: "include",
    headers: { "X-Tenant-ID": tenantId },
  });
  if (!res.ok) throw new Error(`listMembers failed: ${res.status}`);
  return res.json() as Promise<MemberListResponse>;
}

export async function addMember(
  tenantId: string,
  body: { email: string; role: TenantRole },
): Promise<MemberRecord> {
  const res = await fetch("/v1/admin/members", {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json", "X-Tenant-ID": tenantId },
    body: JSON.stringify(body),
  });
  if (res.status === 404) throw new Error("EMAIL_NOT_FOUND");
  if (!res.ok) throw new Error(`addMember failed: ${res.status}`);
  return res.json() as Promise<MemberRecord>;
}

export async function updateMemberRole(
  tenantId: string,
  userId: string,
  role: TenantRole,
): Promise<void> {
  const res = await fetch(`/v1/admin/members/${encodeURIComponent(userId)}/role`, {
    method: "PUT",
    credentials: "include",
    headers: { "Content-Type": "application/json", "X-Tenant-ID": tenantId },
    body: JSON.stringify({ role }),
  });
  if (res.status === 403) throw new Error("SELF_DEMOTION");
  if (!res.ok) throw new Error(`updateMemberRole failed: ${res.status}`);
}

export async function removeMember(tenantId: string, userId: string): Promise<void> {
  const res = await fetch(`/v1/admin/members/${encodeURIComponent(userId)}`, {
    method: "DELETE",
    credentials: "include",
    headers: { "X-Tenant-ID": tenantId },
  });
  if (res.status === 403) throw new Error("LAST_ADMIN");
  if (!res.ok) throw new Error(`removeMember failed: ${res.status}`);
}

export async function revokeMemberSessions(tenantId: string, userId: string): Promise<void> {
  const res = await fetch(
    `/v1/admin/members/${encodeURIComponent(userId)}/revoke-sessions`,
    {
      method: "POST",
      credentials: "include",
      headers: { "X-Tenant-ID": tenantId },
    },
  );
  if (!res.ok) throw new Error(`revokeSessions failed: ${res.status}`);
}
