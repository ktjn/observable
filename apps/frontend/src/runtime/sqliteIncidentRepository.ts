import type { IncidentItem, IncidentListResponse } from "../api/incidents";

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
const schema = "CREATE TABLE IF NOT EXISTS incidents (tenant_id TEXT NOT NULL, incident_id TEXT PRIMARY KEY, incident_json TEXT NOT NULL);";
const rows = <T,>(db: SqliteDb, options: SqlOptions): T[] =>
  (db.exec({ ...options, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? [];

export class SqliteIncidentRepository {
  private constructor(private readonly db: SqliteDb) {}

  static async open(): Promise<SqliteIncidentRepository> {
    const { default: init } = await import("@sqlite.org/sqlite-wasm");
    const sqlite3 = await init();
    const repository = new SqliteIncidentRepository(new sqlite3.oo1.DB(":memory:", "c") as unknown as SqliteDb);
    repository.db.exec(schema);
    repository.seed();
    return repository;
  }

  list(tenantId: string, status?: string): IncidentListResponse {
    const params: SqlOptions = {
      sql: status
        ? "SELECT incident_json FROM incidents WHERE tenant_id = $tenant_id AND json_extract(incident_json, '$.status') = $status ORDER BY incident_id"
        : "SELECT incident_json FROM incidents WHERE tenant_id = $tenant_id ORDER BY incident_id",
      bind: { $tenant_id: tenantId, ...(status ? { $status: status } : {}) },
    };
    return {
      items: rows<Record<string, SqlValue>>(this.db, params).map(
        (row) => JSON.parse(String(row.incident_json)) as IncidentItem,
      ),
    };
  }

  get(tenantId: string, incidentId: string): IncidentItem | null {
    const row = rows<Record<string, SqlValue>>(this.db, {
      sql: "SELECT incident_json FROM incidents WHERE tenant_id = $tenant_id AND incident_id = $incident_id",
      bind: { $tenant_id: tenantId, $incident_id: incidentId },
    })[0];
    return row ? (JSON.parse(String(row.incident_json)) as IncidentItem) : null;
  }

  private insert(tenantId: string, incident: IncidentItem): void {
    this.db.exec({
      sql: "INSERT OR REPLACE INTO incidents (tenant_id, incident_id, incident_json) VALUES ($tenant_id, $incident_id, $incident_json)",
      bind: {
        $tenant_id: tenantId,
        $incident_id: incident.incident_id,
        $incident_json: JSON.stringify(incident),
      },
    });
  }

  private seed(): void {
    this.insert(TENANT, {
      incident_id: "playground-incident-1",
      title: "High error rate on payment",
      severity: "critical",
      status: "triggered",
      triggered_at: new Date(Date.now() - 40 * 60_000).toISOString(),
      triggered_by_rule_id: "playground-rule-1",
    });
  }
}
