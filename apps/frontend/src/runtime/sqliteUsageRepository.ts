import type { TenantUsageReportResponse } from "../api/usage";

type SqlValue = string | number | null;
interface SqlOptions { sql: string; bind?: Record<string, SqlValue>; returnValue?: "resultRows"; rowMode?: "object"; }
interface SqliteDb { exec(sql: string): unknown; exec(options: SqlOptions): unknown; }
const TENANT = "00000000-0000-0000-0000-000000000001";
const schema = "CREATE TABLE IF NOT EXISTS usage_reports (tenant_id TEXT PRIMARY KEY, report_json TEXT NOT NULL)";
const rows = <T,>(db: SqliteDb, options: SqlOptions): T[] => (db.exec({ ...options, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? [];

/** Embedded browser usage-report adapter for the playground runtime. */
export class SqliteUsageRepository {
  private constructor(private readonly db: SqliteDb) {}
  static async open(): Promise<SqliteUsageRepository> {
    const { default: init } = await import("@sqlite.org/sqlite-wasm");
    const sqlite3 = await init();
    const repository = new SqliteUsageRepository(new sqlite3.oo1.DB(":memory:", "c") as unknown as SqliteDb);
    repository.db.exec(schema);
    repository.db.exec({ sql: "INSERT INTO usage_reports (tenant_id, report_json) VALUES ($tenant_id, $report_json)", bind: { $tenant_id: TENANT, $report_json: JSON.stringify({ telemetry_summary: { spans: 240, logs: 40, metric_points: 720, metric_series_created: 12 }, control_plane_summary: { query_reads: 36, query_rows: 1842, credential_checks: 4, credential_allows: 4, credential_denies: 0 }, estimated_cost_index: 512 }) } });
    return repository;
  }
  report(tenantId: string, params: { from?: number; to?: number }): TenantUsageReportResponse {
    const row = rows<Record<string, SqlValue>>(this.db, { sql: "SELECT report_json FROM usage_reports WHERE tenant_id = $tenant_id", bind: { $tenant_id: tenantId } })[0];
    if (!row) throw new Error("usage.report failed: 404");
    const toMs = params.to ?? Date.now();
    const fromMs = params.from ?? toMs - 3_600_000;
    return { tenant_id: tenantId, from: new Date(fromMs).toISOString(), to: new Date(toMs).toISOString(), ...(JSON.parse(String(row.report_json)) as Omit<TenantUsageReportResponse, "tenant_id" | "from" | "to">) };
  }
}
