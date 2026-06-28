/**
 * @modelable domain: admin
 * @modelable name: Member
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface AdminMemberV1 {
  user_id: string;
  email: string;
  name?: string;
  role: 'tenant_admin' | 'member' | 'viewer';
  joined_at: string;
}
export type Member = AdminMemberV1;
