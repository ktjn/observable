import type { MeResponse } from "../api/auth";

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

const DEMO_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const schema = `
  CREATE TABLE IF NOT EXISTS auth_users (
    user_id TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    memberships_json TEXT NOT NULL
  )
`;
const rows = <T,>(db: SqliteDb, options: SqlOptions): T[] =>
  (db.exec({ ...options, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? [];

/** Embedded browser auth/session adapter for the playground runtime. */
export class SqliteAuthRepository {
  private constructor(private readonly db: SqliteDb) {}

  static async open(): Promise<SqliteAuthRepository> {
    const { default: init } = await import("@sqlite.org/sqlite-wasm");
    const sqlite3 = await init();
    const repository = new SqliteAuthRepository(new sqlite3.oo1.DB(":memory:", "c") as unknown as SqliteDb);
    repository.db.exec(schema);
    repository.seed();
    return repository;
  }

  me(): MeResponse {
    const row = rows<Record<string, SqlValue>>(this.db, {
      sql: "SELECT user_id, email, memberships_json FROM auth_users LIMIT 1",
    })[0];
    if (!row) throw new Error("auth.me failed: 404");
    return {
      user_id: String(row.user_id),
      email: String(row.email),
      tenants: JSON.parse(String(row.memberships_json)) as MeResponse["tenants"],
    };
  }

  private seed(): void {
    this.db.exec({
      sql: "INSERT INTO auth_users (user_id, email, memberships_json) VALUES ($user_id, $email, $memberships_json)",
      bind: {
        $user_id: "playground-user",
        $email: "playground@local",
        $memberships_json: JSON.stringify([{ tenant_id: DEMO_TENANT_ID, role: "admin" }]),
      },
    });
  }
}
