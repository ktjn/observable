import type { InfrastructureEntitySummary, InfrastructureEntityType } from "../api/infrastructure";

type SqlValue = string | number | null;
interface SqlOptions { sql: string; bind?: Record<string, SqlValue>; returnValue?: "resultRows"; rowMode?: "object"; }
interface SqliteDb { exec(sql: string): unknown; exec(options: SqlOptions): unknown; }

const TENANT = "00000000-0000-0000-0000-000000000001";
const schema = `CREATE TABLE IF NOT EXISTS infrastructure_entities (
  tenant_id TEXT NOT NULL, entity_type TEXT NOT NULL, entity_id TEXT NOT NULL,
  entity_json TEXT NOT NULL, PRIMARY KEY (tenant_id, entity_type, entity_id)
)`;
const rows = <T,>(db: SqliteDb, options: SqlOptions): T[] =>
  (db.exec({ ...options, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? [];

/** Embedded browser inventory adapter for the playground runtime. */
export class SqliteInfrastructureRepository {
  private constructor(private readonly db: SqliteDb) {}

  static async open(): Promise<SqliteInfrastructureRepository> {
    const { default: init } = await import("@sqlite.org/sqlite-wasm");
    const sqlite3 = await init();
    const repository = new SqliteInfrastructureRepository(new sqlite3.oo1.DB(":memory:", "c") as unknown as SqliteDb);
    repository.db.exec(schema);
    repository.seed();
    return repository;
  }

  list(tenantId: string, entityType?: InfrastructureEntityType): InfrastructureEntitySummary[] {
    const result = rows<Record<string, SqlValue>>(this.db, {
      sql: entityType
        ? "SELECT entity_json FROM infrastructure_entities WHERE tenant_id = $tenant_id AND entity_type = $entity_type ORDER BY entity_id"
        : "SELECT entity_json FROM infrastructure_entities WHERE tenant_id = $tenant_id ORDER BY entity_type, entity_id",
      bind: entityType ? { $tenant_id: tenantId, $entity_type: entityType } : { $tenant_id: tenantId },
    });
    return result.map((row) => JSON.parse(String(row.entity_json)) as InfrastructureEntitySummary);
  }

  get(tenantId: string, entityType: InfrastructureEntityType, entityId: string): InfrastructureEntitySummary | null {
    const row = rows<Record<string, SqlValue>>(this.db, {
      sql: "SELECT entity_json FROM infrastructure_entities WHERE tenant_id = $tenant_id AND entity_type = $entity_type AND entity_id = $entity_id",
      bind: { $tenant_id: tenantId, $entity_type: entityType, $entity_id: entityId },
    })[0];
    return row ? (JSON.parse(String(row.entity_json)) as InfrastructureEntitySummary) : null;
  }

  private seed(): void {
    const nowNs = Number(BigInt(Date.now()) * 1_000_000n);
    const fiveMinAgo = nowNs - 5 * 60_000_000_000;
    const base = { environment: "production" as const, health_state: "healthy" as const, last_seen_unix_nano: nowNs };
    const entities: InfrastructureEntitySummary[] = [
      { ...base, entity_type: "host", entity_id: "playground-host-1", display_name: "demo-node-1", parent_id: null, parent_display_name: null, related_services: ["checkout", "payment", "web", "api-gateway"], log_rate_per_minute: 42, error_rate: 0.02, restart_count: 0, cpu_usage: 38, memory_usage: 61, disk_usage: 47, network_io: 1204 },
      { ...base, entity_type: "cluster", entity_id: "playground-cluster-1", display_name: "demo-cluster", parent_id: "playground-host-1", parent_display_name: "demo-node-1", related_services: ["checkout", "payment"], log_rate_per_minute: 30, error_rate: 0.03, restart_count: 0, cpu_usage: 45, memory_usage: 58, disk_usage: 40, network_io: 980 },
      { ...base, entity_type: "namespace", entity_id: "playground-ns-demo", display_name: "demo", parent_id: "playground-cluster-1", parent_display_name: "demo-cluster", related_services: ["checkout", "payment"], log_rate_per_minute: 25, error_rate: 0.03, restart_count: 1, cpu_usage: 41, memory_usage: 55, disk_usage: 39, network_io: 860 },
      { ...base, entity_type: "pod", entity_id: "playground-pod-checkout", display_name: "checkout-7f9d8b6c5-x2p4k", parent_id: "playground-ns-demo", parent_display_name: "demo", related_services: ["checkout"], log_rate_per_minute: 12, error_rate: 0.01, restart_count: 0, cpu_usage: 33, memory_usage: 48, disk_usage: null, network_io: 310 },
      { ...base, entity_type: "container", entity_id: "playground-container-checkout", display_name: "checkout", parent_id: "playground-pod-checkout", parent_display_name: "checkout-7f9d8b6c5-x2p4k", related_services: ["checkout"], log_rate_per_minute: 11, error_rate: 0.01, restart_count: 0, cpu_usage: 31, memory_usage: 46, disk_usage: null, network_io: 300 },
      { ...base, entity_type: "pod", entity_id: "playground-pod-payment", display_name: "payment-6c4d7a9b8-m8wqz", parent_id: "playground-ns-demo", parent_display_name: "demo", related_services: ["payment"], health_state: "breach", last_seen_unix_nano: fiveMinAgo, log_rate_per_minute: 18, error_rate: 0.14, restart_count: 2, cpu_usage: 72, memory_usage: 81, disk_usage: null, network_io: 420 },
      { ...base, entity_type: "container", entity_id: "playground-container-payment", display_name: "payment", parent_id: "playground-pod-payment", parent_display_name: "payment-6c4d7a9b8-m8wqz", related_services: ["payment"], health_state: "watch", last_seen_unix_nano: fiveMinAgo, log_rate_per_minute: 17, error_rate: 0.13, restart_count: 2, cpu_usage: 70, memory_usage: 79, disk_usage: null, network_io: 410 },
    ];
    for (const entity of entities) this.db.exec({
      sql: "INSERT INTO infrastructure_entities (tenant_id, entity_type, entity_id, entity_json) VALUES ($tenant_id, $entity_type, $entity_id, $entity_json)",
      bind: { $tenant_id: TENANT, $entity_type: entity.entity_type, $entity_id: entity.entity_id, $entity_json: JSON.stringify(entity) },
    });
  }
}
