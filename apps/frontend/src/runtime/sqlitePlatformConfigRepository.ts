import type { PlatformConfig, SaveLlmConfigParams } from "../api/setup";

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

const defaults: PlatformConfig = {
  llm_key_configured: false,
  llm_url: null,
  llm_model: null,
  llm_provider: "remote",
  webllm_model: null,
};
const schema = "CREATE TABLE IF NOT EXISTS platform_config (tenant_id TEXT PRIMARY KEY, config_json TEXT NOT NULL);";
const rows = <T,>(db: SqliteDb, options: SqlOptions): T[] =>
  (db.exec({ ...options, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? [];

export class SqlitePlatformConfigRepository {
  private constructor(private readonly db: SqliteDb) {}

  static async open(): Promise<SqlitePlatformConfigRepository> {
    const { default: init } = await import("@sqlite.org/sqlite-wasm");
    const sqlite3 = await init();
    const repository = new SqlitePlatformConfigRepository(new sqlite3.oo1.DB(":memory:", "c") as unknown as SqliteDb);
    repository.db.exec(schema);
    return repository;
  }

  get(tenantId: string): PlatformConfig {
    const row = rows<Record<string, SqlValue>>(this.db, {
      sql: "SELECT config_json FROM platform_config WHERE tenant_id = $tenant_id",
      bind: { $tenant_id: tenantId },
    })[0];
    return row ? (JSON.parse(String(row.config_json)) as PlatformConfig) : { ...defaults };
  }

  update(tenantId: string, params: SaveLlmConfigParams): PlatformConfig {
    const current = this.get(tenantId);
    const next: PlatformConfig = {
      llm_key_configured:
        params.apiKey === undefined ? current.llm_key_configured : params.apiKey !== "",
      llm_url: params.url === undefined ? current.llm_url : params.url || null,
      llm_model: params.model === undefined ? current.llm_model : params.model || null,
      llm_provider: params.provider === undefined ? current.llm_provider : params.provider,
      webllm_model:
        params.webllmModel === undefined ? current.webllm_model : params.webllmModel || null,
    };
    this.db.exec({
      sql: "INSERT OR REPLACE INTO platform_config (tenant_id, config_json) VALUES ($tenant_id, $config_json)",
      bind: { $tenant_id: tenantId, $config_json: JSON.stringify(next) },
    });
    return next;
  }
}
