import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  type MemberRecord,
  type TenantRole,
  addMember,
  listMembers,
  removeMember,
  revokeMemberSessions,
  updateMemberRole,
} from "../../api/admin-members";
import { Badge } from "../../components/ui/badge";
import { EmptyState } from "../../components/ui/empty-state";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { TablePanel } from "../../components/ui/table-panel";
import { CopyableText } from "../../components/ui/copy-button";
import { useAuth } from "../../hooks/useAuth";
import { useTenantContext } from "../../hooks/useTenantContext";
import { roleLabel, roleTone } from "./admin-utils";
import { AdminSurfaceNav } from "./AdminSurfaceNav";

const ROLES: TenantRole[] = ["tenant_admin", "member", "viewer"];

export function MemberManagementPage() {
  const { tenantId } = useTenantContext();
  const { data: me } = useAuth();
  const qc = useQueryClient();

  const [addEmail, setAddEmail] = useState("");
  const [addRole, setAddRole] = useState<TenantRole>("member");
  const [addError, setAddError] = useState<string | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);

  const { data, isLoading } = useQuery({
    queryKey: ["admin-members", tenantId],
    queryFn: () => listMembers(tenantId),
    enabled: !!tenantId,
  });

  const myMembership = me?.tenants?.find((t) => t.tenant_id === tenantId);
  const isAdmin = myMembership?.role === "tenant_admin";

  const invalidate = () => qc.invalidateQueries({ queryKey: ["admin-members", tenantId] });

  const addMutation = useMutation({
    mutationFn: (vars: { email: string; role: TenantRole }) => addMember(tenantId, vars),
    onSuccess: () => {
      setAddEmail("");
      setAddError(null);
      setStatusMsg("Member added.");
      void invalidate();
    },
    onError: (err: Error) => {
      if (err.message === "EMAIL_NOT_FOUND") {
        setAddError("No account found for that email.");
      } else {
        setAddError("Failed to add member. Please try again.");
      }
    },
  });

  const roleMutation = useMutation({
    mutationFn: (vars: { userId: string; role: TenantRole }) =>
      updateMemberRole(tenantId, vars.userId, vars.role),
    onSuccess: () => {
      setStatusMsg("Role updated.");
      void invalidate();
    },
    onError: (err: Error) => {
      if (err.message === "SELF_DEMOTION") {
        setStatusMsg("You cannot change your own role.");
      } else {
        setStatusMsg("Failed to update role.");
      }
    },
  });

  const removeMutation = useMutation({
    mutationFn: (userId: string) => removeMember(tenantId, userId),
    onSuccess: () => {
      setStatusMsg("Member removed.");
      void invalidate();
    },
    onError: (err: Error) => {
      if (err.message === "LAST_ADMIN") {
        setStatusMsg("Cannot remove the last admin from a tenant.");
      } else {
        setStatusMsg("Failed to remove member.");
      }
    },
  });

  const revokeMutation = useMutation({
    mutationFn: (userId: string) => revokeMemberSessions(tenantId, userId),
    onSuccess: () => {
      setStatusMsg("Sessions revoked.");
      void invalidate();
    },
    onError: () => setStatusMsg("Failed to revoke sessions."),
  });

  function handleAdd(e: React.FormEvent) {
    e.preventDefault();
    setAddError(null);
    if (!addEmail.trim()) {
      setAddError("Email is required.");
      return;
    }
    addMutation.mutate({ email: addEmail.trim(), role: addRole });
  }

  function handleRemove(member: MemberRecord) {
    if (!confirm(`Remove ${member.email} from this tenant?`)) return;
    removeMutation.mutate(member.user_id);
  }

  if (isLoading) return <LoadingState>Loading members…</LoadingState>;

  const members = data?.members ?? [];

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Administration</div>
          <h1>Members</h1>
          <p className="mt-2 max-w-3xl text-sm text-[var(--muted)]">
            Manage who has access to this tenant. Only tenant admins can make changes.
          </p>
        </div>
      </div>

      <AdminSurfaceNav />

      {statusMsg && (
        <p className="text-sm text-[var(--text)]" role="status">
          {statusMsg}
        </p>
      )}

      {isAdmin && (
        <Panel title="Add member" eyebrow="Invite">
          <form onSubmit={handleAdd} className="flex flex-wrap items-end gap-3">
            <div className="flex flex-col gap-1">
              <label className="field-label" htmlFor="add-email">
                Email
              </label>
              <input
                id="add-email"
                type="email"
                className="input w-64"
                placeholder="user@example.com"
                value={addEmail}
                onChange={(e) => setAddEmail(e.target.value)}
                disabled={addMutation.isPending}
              />
            </div>
            <div className="flex flex-col gap-1">
              <label className="field-label" htmlFor="add-role">
                Role
              </label>
              <select
                id="add-role"
                className="input"
                value={addRole}
                onChange={(e) => setAddRole(e.target.value as TenantRole)}
                disabled={addMutation.isPending}
              >
                {ROLES.map((r) => (
                  <option key={r} value={r}>
                    {roleLabel(r)}
                  </option>
                ))}
              </select>
            </div>
            <button type="submit" className="button-primary" disabled={addMutation.isPending}>
              {addMutation.isPending ? "Adding…" : "Add"}
            </button>
            {addError && <p className="w-full text-xs text-[var(--error)]">{addError}</p>}
          </form>
        </Panel>
      )}

      <Panel title={`Members (${members.length})`} eyebrow="RBAC">
        {members.length === 0 ? (
          <EmptyState title="No members" description="No users are assigned to this tenant." />
        ) : (
          <TablePanel>
            <table className="min-w-full border-collapse text-left text-sm">
              <thead className="bg-[var(--surface-muted)] text-[var(--muted)]">
                <tr>
                  <th scope="col" className="px-3 py-2 font-semibold">
                    Name / Email
                  </th>
                  <th scope="col" className="px-3 py-2 font-semibold">
                    Role
                  </th>
                  {isAdmin && (
                    <th scope="col" className="px-3 py-2 font-semibold">
                      Actions
                    </th>
                  )}
                </tr>
              </thead>
              <tbody>
                {members.map((member) => {
                  const isSelf = member.user_id === me?.user_id;
                  return (
                    <tr key={member.user_id}>
                      <td className="px-3 py-2">
                        <div className="font-medium text-[var(--text-strong)]">
                          {member.name ?? member.email}
                          {isSelf && (
                            <span className="ml-2 text-[11px] uppercase tracking-wide text-[var(--muted)]">
                              you
                            </span>
                          )}
                        </div>
                        {member.name && (
                          <div className="text-xs text-[var(--muted)]">{member.email}</div>
                        )}
                        <div className="text-xs text-[var(--muted)]">
                          <CopyableText value={member.user_id} label="Copy user id" mono />
                        </div>
                      </td>
                      <td className="px-3 py-2">
                        {isAdmin && !isSelf ? (
                          <select
                            aria-label={`Role for ${member.email}`}
                            className="input text-sm"
                            value={member.role}
                            onChange={(e) =>
                              roleMutation.mutate({
                                userId: member.user_id,
                                role: e.target.value as TenantRole,
                              })
                            }
                            disabled={roleMutation.isPending}
                          >
                            {ROLES.map((r) => (
                              <option key={r} value={r}>
                                {roleLabel(r)}
                              </option>
                            ))}
                          </select>
                        ) : (
                          <Badge tone={roleTone(member.role)}>{roleLabel(member.role)}</Badge>
                        )}
                      </td>
                      {isAdmin && (
                        <td className="px-3 py-2">
                          <div className="flex items-center gap-3">
                            <button
                              className="text-xs text-[var(--muted)] hover:text-[var(--text)] transition-colors"
                              onClick={() => revokeMutation.mutate(member.user_id)}
                              disabled={revokeMutation.isPending}
                              aria-label={`Revoke sessions for ${member.email}`}
                            >
                              Revoke sessions
                            </button>
                            {!isSelf && (
                              <button
                                className="text-xs text-[var(--error)] hover:opacity-80 transition-opacity"
                                onClick={() => handleRemove(member)}
                                disabled={removeMutation.isPending}
                                aria-label={`Remove ${member.email}`}
                              >
                                ×
                              </button>
                            )}
                          </div>
                        </td>
                      )}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </TablePanel>
        )}
      </Panel>
    </section>
  );
}
