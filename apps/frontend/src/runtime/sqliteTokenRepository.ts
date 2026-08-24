import type {
  CreateTokenRequest,
  CreateTokenResponse,
  TokenListResponse,
  TokenRecord,
} from "../api/tokens";

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

const schema = `
  CREATE TABLE IF NOT EXISTS api_tokens (
    tenant_id TEXT NOT NULL,
    token_id TEXT PRIMARY KEY,
    token_json TEXT NOT NULL
  )
`;
const rows = <T,>(db: SqliteDb, options: SqlOptions): T[] =>
  (db.exec({ ...options, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? [];

const id = () =>
  `playground-token-${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`}`;

export class SqliteTokenRepository {
  private constructor(private readonly db: SqliteDb) {}

  static async open(): Promise<SqliteTokenRepository> {
    const { default: init } = await import("@sqlite.org/sqlite-wasm");
    const sqlite3 = await init();
    const repository = new SqliteTokenRepository(new sqlite3.oo1.DB(":memory:", "c") as unknown as SqliteDb);
    repository.db.exec(schema);
    repository.seed();
    return repository;
  }

  list(tenantId: string): TokenListResponse {
    return {
      tokens: rows<Record<string, SqlValue>>(this.db, {
        sql: "SELECT token_json FROM api_tokens WHERE tenant_id = $tenant_id ORDER BY token_id",
        bind: { $tenant_id: tenantId },
      }).map((row) => JSON.parse(String(row.token_json)) as TokenRecord),
    };
  }

  create(tenantId: string, tenantName: string, request: CreateTokenRequest): CreateTokenResponse {
    const token: TokenRecord = {
      id: id(),
      name: request.name,
      tenant_name: tenantName,
      environment: request.environment,
      created_at: new Date().toISOString(),
      revoked: false,
    };
    this.insert(tenantId, token);
    return { ...token, plaintext: `demo-${token.id}-0000` };
  }

  setRevoked(tenantId: string, tokenId: string, revoked: boolean): void {
    const token = this.find(tenantId, tokenId);
    if (token) {
      this.insert(tenantId, { ...token, revoked });
    }
  }

  find(tenantId: string, tokenId: string): TokenRecord | null {
    const row = rows<Record<string, SqlValue>>(this.db, {
      sql: "SELECT token_json FROM api_tokens WHERE tenant_id = $tenant_id AND token_id = $token_id",
      bind: { $tenant_id: tenantId, $token_id: tokenId },
    })[0];
    return row ? (JSON.parse(String(row.token_json)) as TokenRecord) : null;
  }

  delete(tenantId: string, tokenId: string): void {
    this.db.exec({
      sql: "DELETE FROM api_tokens WHERE tenant_id = $tenant_id AND token_id = $token_id",
      bind: { $tenant_id: tenantId, $token_id: tokenId },
    });
  }

  private insert(tenantId: string, token: TokenRecord): void {
    this.db.exec({
      sql: "INSERT OR REPLACE INTO api_tokens (tenant_id, token_id, token_json) VALUES ($tenant_id, $token_id, $token_json)",
      bind: {
        $tenant_id: tenantId,
        $token_id: token.id,
        $token_json: JSON.stringify(token),
      },
    });
  }

  private seed(): void {
    this.insert("00000000-0000-0000-0000-000000000001", {
      id: "playground-token-1",
      name: "demo ingest token",
      tenant_name: "observable",
      environment: "production",
      created_at: new Date(Date.now() - 3_600_000).toISOString(),
      revoked: false,
    });
  }
}
