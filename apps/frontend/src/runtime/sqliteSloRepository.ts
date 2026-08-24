import type { CreateSloRequest, SloDefinitionItem, SloListResponse } from "../api/slos";

type SqliteValue = string | number | null;
interface SqliteExecOptions { sql: string; bind?: Record<string, SqliteValue>; returnValue?: "resultRows"; rowMode?: "object"; }
interface SqliteDatabase { exec(sql: string): unknown; exec(options: SqliteExecOptions): unknown; }
const TENANT_ID = "00000000-0000-0000-0000-000000000001";
const SCHEMA = "CREATE TABLE IF NOT EXISTS slos (tenant_id TEXT NOT NULL, slo_id TEXT PRIMARY KEY, slo_json TEXT NOT NULL);";
function rows<T>(db: SqliteDatabase, options: SqliteExecOptions): T[] { return (db.exec({ ...options, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? []; }
function id(): string { return `playground-slo-${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`}`; }
function map(row: Record<string, SqliteValue>): SloDefinitionItem { return JSON.parse(String(row.slo_json)) as SloDefinitionItem; }

export class SqliteSloRepository {
  private constructor(private readonly db: SqliteDatabase) {}
  static async open(): Promise<SqliteSloRepository> {
    const { default: init } = await import("@sqlite.org/sqlite-wasm");
    const sqlite3 = await init();
    const repository = new SqliteSloRepository(new sqlite3.oo1.DB(":memory:", "c") as unknown as SqliteDatabase);
    repository.db.exec(SCHEMA); repository.seed(); return repository;
  }
  list(tenantId: string): SloListResponse { return { items: rows<Record<string, SqliteValue>>(this.db, { sql: "SELECT slo_json FROM slos WHERE tenant_id = $tenant_id ORDER BY slo_id", bind: { $tenant_id: tenantId } }).map(map) }; }
  create(tenantId: string, request: CreateSloRequest): SloDefinitionItem {
    const now = new Date().toISOString();
    const slo: SloDefinitionItem = { slo_id: id(), service_name: request.service_name, environment: request.environment, sli_type: "availability", target: request.target, window_days: request.window_days, burn_rate_fast_threshold: request.burn_rate_fast_threshold, burn_rate_slow_threshold: request.burn_rate_slow_threshold, description: request.description ?? "", firing: false, created_at: now, updated_at: now };
    this.db.exec({ sql: "INSERT INTO slos (tenant_id, slo_id, slo_json) VALUES ($tenant_id, $slo_id, $slo_json)", bind: { $tenant_id: tenantId, $slo_id: slo.slo_id, $slo_json: JSON.stringify(slo) } }); return slo;
  }
  private seed(): void { this.db.exec({ sql: "INSERT INTO slos (tenant_id, slo_id, slo_json) VALUES ($tenant_id, $slo_id, $slo_json)", bind: { $tenant_id: TENANT_ID, $slo_id: "playground-slo-1", $slo_json: JSON.stringify({ slo_id: "playground-slo-1", service_name: "checkout", environment: "production", sli_type: "availability", target: 99.9, window_days: 30, burn_rate_fast_threshold: 14.4, burn_rate_slow_threshold: 6, description: "checkout availability SLO", firing: false, created_at: new Date(Date.now() - 7 * 86_400_000).toISOString(), updated_at: new Date(Date.now() - 7 * 86_400_000).toISOString() }) } }); }
}
