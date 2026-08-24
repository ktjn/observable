import type { MemberRecord, TenantRole } from "../api/admin-members";

type SqlValue = string | number | null;
interface SqlOptions {
  sql: string;
  bind?: Record<string, SqlValue>;
  returnValue?: "resultRows";
  rowMode?: "object";
}
interface SqliteDb {
  exec(sql: string): unknown;
  exec(options: SqlOptions): unknown;
}

const TENANT = "00000000-0000-0000-0000-000000000001";
const schema = `
  CREATE TABLE IF NOT EXISTS tenant_members (
    tenant_id TEXT NOT NULL,
    user_id TEXT PRIMARY KEY,
    member_json TEXT NOT NULL
  )
`;
const rows = <T,>(db: SqliteDb, options: SqlOptions): T[] =>
  (db.exec({ ...options, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? [];

export class SqliteMemberRepository {
  private constructor(private readonly db: SqliteDb) {}

  static async open(): Promise<SqliteMemberRepository> {
    const { default: init } = await import("@sqlite.org/sqlite-wasm");
    const sqlite3 = await init();
    const repository = new SqliteMemberRepository(new sqlite3.oo1.DB(":memory:", "c") as unknown as SqliteDb);
    repository.db.exec(schema);
    repository.seed();
    return repository;
  }

  list(tenantId: string): MemberRecord[] {
    return rows<Record<string, SqlValue>>(this.db, {
      sql: "SELECT member_json FROM tenant_members WHERE tenant_id = $tenant_id ORDER BY user_id",
      bind: { $tenant_id: tenantId },
    }).map((row) => JSON.parse(String(row.member_json)) as MemberRecord);
  }

  add(tenantId: string, email: string, role: TenantRole): MemberRecord {
    if (this.findByEmail(tenantId, email)) throw new Error("addMember failed: conflict");
    const member: MemberRecord = {
      user_id: `playground-member-${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`}`,
      email,
      role,
      joined_at: new Date().toISOString(),
    };
    this.insert(tenantId, member);
    return member;
  }

  updateRole(tenantId: string, userId: string, role: TenantRole): void {
    const member = this.find(tenantId, userId);
    if (!member) throw new Error("updateMemberRole failed: 404");
    this.insert(tenantId, { ...member, role });
  }

  remove(tenantId: string, userId: string): void {
    this.db.exec({
      sql: "DELETE FROM tenant_members WHERE tenant_id = $tenant_id AND user_id = $user_id",
      bind: { $tenant_id: tenantId, $user_id: userId },
    });
  }

  private find(tenantId: string, userId: string): MemberRecord | null {
    const row = rows<Record<string, SqlValue>>(this.db, {
      sql: "SELECT member_json FROM tenant_members WHERE tenant_id = $tenant_id AND user_id = $user_id",
      bind: { $tenant_id: tenantId, $user_id: userId },
    })[0];
    return row ? (JSON.parse(String(row.member_json)) as MemberRecord) : null;
  }

  private findByEmail(tenantId: string, email: string): MemberRecord | null {
    return this.list(tenantId).find((member) => member.email === email) ?? null;
  }

  private insert(tenantId: string, member: MemberRecord): void {
    this.db.exec({
      sql: "INSERT OR REPLACE INTO tenant_members (tenant_id, user_id, member_json) VALUES ($tenant_id, $user_id, $member_json)",
      bind: {
        $tenant_id: tenantId,
        $user_id: member.user_id,
        $member_json: JSON.stringify(member),
      },
    });
  }

  private seed(): void {
    this.insert(TENANT, {
      user_id: "playground-user",
      email: "playground@local",
      name: "Playground User",
      role: "tenant_admin",
      joined_at: new Date(Date.now() - 7 * 86_400_000).toISOString(),
    });
  }
}
