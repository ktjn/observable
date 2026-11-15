import type { InfrastructureEntitySummary, InfrastructureEntityType } from "../api/infrastructure";
import { modulateInfrastructureEntity } from "../playground/liveTick";

type SqlValue = string | number | null;
interface SqlOptions { sql: string; bind?: Record<string, SqlValue>; returnValue?: "resultRows"; rowMode?: "object"; }
interface SqliteDb { exec(sql: string): unknown; exec(options: SqlOptions): unknown; }

const TENANT = "00000000-0000-0000-0000-000000000001";
export const DEMO_INFRA_IDS = {
  host1: "1a4b7c8d-3e2f-4a91-8b5c-6d7e8f9a0b01",
  cluster1: "2b5c8d9e-4f3a-4b02-9c6d-7e8f9a0b1c02",
  namespaceProd: "3c6d9e0f-5a4b-4c13-ad7e-8f9a0b1c2d03",
  podCheckout: "4d7e0f1a-6b5c-4d24-be8f-9a0b1c2d3e04",
  containerCheckout: "5e8f1a2b-7c6d-4e35-cf9a-0b1c2d3e4f05",
  podPayment: "6f9a2b3c-8d7e-4f46-8a0b-1c2d3e4f5a06",
  containerPayment: "7a0b3c4d-9e8f-4a57-9b1c-2d3e4f5a6b07",
  podGateway: "8b1c4d5e-0f9a-4b68-ac2d-3e4f5a6b7c08",
  containerGateway: "9c2d5e6f-1a0b-4c79-bd3e-4f5a6b7c8d09",
} as const;
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
    const nowMs = Date.now();
    return result.map((row) =>
      modulateInfrastructureEntity(
        JSON.parse(String(row.entity_json)) as InfrastructureEntitySummary,
        nowMs
      )
    );
  }

  get(tenantId: string, entityType: InfrastructureEntityType, entityId: string): InfrastructureEntitySummary | null {
    const row = rows<Record<string, SqlValue>>(this.db, {
      sql: "SELECT entity_json FROM infrastructure_entities WHERE tenant_id = $tenant_id AND entity_type = $entity_type AND entity_id = $entity_id",
      bind: { $tenant_id: tenantId, $entity_type: entityType, $entity_id: entityId },
    })[0];
    return row
      ? modulateInfrastructureEntity(
          JSON.parse(String(row.entity_json)) as InfrastructureEntitySummary,
          Date.now()
        )
      : null;
  }

  private seed(): void {
    const nowNs = Number(BigInt(Date.now()) * 1_000_000n);
    const fiveMinAgo = nowNs - 5 * 60_000_000_000;
    const base = { environment: "production" as const, health_state: "healthy" as const, last_seen_unix_nano: nowNs };
    const entities: InfrastructureEntitySummary[] = [
      { ...base, entity_type: "host", entity_id: DEMO_INFRA_IDS.host1, display_name: "ip-10-42-1-14.ec2.internal", parent_id: null, parent_display_name: null, related_services: ["checkout", "payment", "web", "api-gateway", "inventory"], log_rate_per_minute: 48, error_rate: 0.02, restart_count: 0, cpu_usage: 38, memory_usage: 61, disk_usage: 47, network_io: 1204 },
      { ...base, entity_type: "cluster", entity_id: DEMO_INFRA_IDS.cluster1, display_name: "prod-us-east-1", parent_id: DEMO_INFRA_IDS.host1, parent_display_name: "ip-10-42-1-14.ec2.internal", related_services: ["checkout", "payment", "api-gateway"], log_rate_per_minute: 36, error_rate: 0.03, restart_count: 0, cpu_usage: 45, memory_usage: 58, disk_usage: 40, network_io: 980 },
      { ...base, entity_type: "namespace", entity_id: DEMO_INFRA_IDS.namespaceProd, display_name: "ecommerce-prod", parent_id: DEMO_INFRA_IDS.cluster1, parent_display_name: "prod-us-east-1", related_services: ["checkout", "payment", "api-gateway"], log_rate_per_minute: 29, error_rate: 0.03, restart_count: 1, cpu_usage: 41, memory_usage: 55, disk_usage: 39, network_io: 860 },
      { ...base, entity_type: "pod", entity_id: DEMO_INFRA_IDS.podCheckout, display_name: "checkout-7f9d8b6c5-x2p4k", parent_id: DEMO_INFRA_IDS.namespaceProd, parent_display_name: "ecommerce-prod", related_services: ["checkout"], log_rate_per_minute: 12, error_rate: 0.01, restart_count: 0, cpu_usage: 33, memory_usage: 48, disk_usage: null, network_io: 310 },
      { ...base, entity_type: "container", entity_id: DEMO_INFRA_IDS.containerCheckout, display_name: "checkout", parent_id: DEMO_INFRA_IDS.podCheckout, parent_display_name: "checkout-7f9d8b6c5-x2p4k", related_services: ["checkout"], log_rate_per_minute: 11, error_rate: 0.01, restart_count: 0, cpu_usage: 31, memory_usage: 46, disk_usage: null, network_io: 300 },
      { ...base, entity_type: "pod", entity_id: DEMO_INFRA_IDS.podPayment, display_name: "payment-6c4d7a9b8-m8wqz", parent_id: DEMO_INFRA_IDS.namespaceProd, parent_display_name: "ecommerce-prod", related_services: ["payment"], health_state: "breach", last_seen_unix_nano: fiveMinAgo, log_rate_per_minute: 18, error_rate: 0.14, restart_count: 2, cpu_usage: 72, memory_usage: 81, disk_usage: null, network_io: 420 },
      { ...base, entity_type: "container", entity_id: DEMO_INFRA_IDS.containerPayment, display_name: "payment", parent_id: DEMO_INFRA_IDS.podPayment, parent_display_name: "payment-6c4d7a9b8-m8wqz", related_services: ["payment"], health_state: "watch", last_seen_unix_nano: fiveMinAgo, log_rate_per_minute: 17, error_rate: 0.13, restart_count: 2, cpu_usage: 70, memory_usage: 79, disk_usage: null, network_io: 410 },
      { ...base, entity_type: "pod", entity_id: DEMO_INFRA_IDS.podGateway, display_name: "api-gateway-5d8f9c7b4-k9n2m", parent_id: DEMO_INFRA_IDS.namespaceProd, parent_display_name: "ecommerce-prod", related_services: ["api-gateway"], log_rate_per_minute: 22, error_rate: 0.01, restart_count: 0, cpu_usage: 29, memory_usage: 44, disk_usage: null, network_io: 540 },
      { ...base, entity_type: "container", entity_id: DEMO_INFRA_IDS.containerGateway, display_name: "api-gateway", parent_id: DEMO_INFRA_IDS.podGateway, parent_display_name: "api-gateway-5d8f9c7b4-k9n2m", related_services: ["api-gateway"], log_rate_per_minute: 21, error_rate: 0.01, restart_count: 0, cpu_usage: 27, memory_usage: 42, disk_usage: null, network_io: 525 },
    ];
    for (const entity of entities) this.db.exec({
      sql: "INSERT INTO infrastructure_entities (tenant_id, entity_type, entity_id, entity_json) VALUES ($tenant_id, $entity_type, $entity_id, $entity_json)",
      bind: { $tenant_id: TENANT, $entity_type: entity.entity_type, $entity_id: entity.entity_id, $entity_json: JSON.stringify(entity) },
    });
  }
}
