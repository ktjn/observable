import type { DeploymentMarker, ListDeploymentsParams } from "../api/deployments";

type SqlValue = string | number | null;
interface SqlOptions { sql: string; bind?: Record<string, SqlValue>; returnValue?: "resultRows"; rowMode?: "object"; }
interface SqliteDb { exec(sql: string): unknown; exec(options: SqlOptions): unknown; }
const TENANT = "00000000-0000-0000-0000-000000000001";
const schema = "CREATE TABLE IF NOT EXISTS deployments (tenant_id TEXT NOT NULL, deployment_id TEXT NOT NULL, deployment_json TEXT NOT NULL, PRIMARY KEY (tenant_id, deployment_id))";
const rows = <T,>(db: SqliteDb, options: SqlOptions): T[] => (db.exec({ ...options, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? [];

/** Embedded browser deployment-marker adapter for the playground runtime. */
export class SqliteDeploymentRepository {
  private constructor(private readonly db: SqliteDb) {}
  static async open(): Promise<SqliteDeploymentRepository> {
    const { default: init } = await import("@sqlite.org/sqlite-wasm");
    const sqlite3 = await init();
    const repository = new SqliteDeploymentRepository(new sqlite3.oo1.DB(":memory:", "c") as unknown as SqliteDb);
    repository.db.exec(schema);
    repository.seed();
    return repository;
  }
  list(tenantId: string, params: ListDeploymentsParams = {}): DeploymentMarker[] {
    const result = rows<Record<string, SqlValue>>(this.db, {
      sql: "SELECT deployment_json FROM deployments WHERE tenant_id = $tenant_id ORDER BY deployment_id DESC",
      bind: { $tenant_id: tenantId },
    }).map((row) => JSON.parse(String(row.deployment_json)) as DeploymentMarker);
    return result.filter((d) => (!params.service_name || d.service_name === params.service_name) && (!params.environment || d.environment === params.environment)).slice(0, params.limit ?? result.length);
  }
  private seed(): void {
    const now = Date.now();
    const values: DeploymentMarker[] = [
      { deployment_id: "playground-deployment-3", tenant_id: TENANT, project_id: null, service_name: "payment", environment: "production", service_version: "2.4.0", status: "in_progress", started_at: new Date(now - 15 * 60_000).toISOString(), finished_at: null, deployed_by: "playground-user", commit_sha: "a1b2c3d", rollback_of: null, metadata: null },
      { deployment_id: "playground-deployment-2", tenant_id: TENANT, project_id: null, service_name: "payment", environment: "production", service_version: "2.3.1", status: "success", started_at: new Date(now - 26 * 3_600_000).toISOString(), finished_at: new Date(now - 26 * 3_600_000 + 4 * 60_000).toISOString(), deployed_by: "playground-user", commit_sha: "d4e5f6a", rollback_of: null, metadata: null },
      { deployment_id: "playground-deployment-1", tenant_id: TENANT, project_id: null, service_name: "checkout", environment: "production", service_version: "1.18.0", status: "success", started_at: new Date(now - 50 * 3_600_000).toISOString(), finished_at: new Date(now - 50 * 3_600_000 + 7 * 60_000).toISOString(), deployed_by: "playground-user", commit_sha: "b8c9d0e", rollback_of: null, metadata: null },
    ];
    for (const deployment of values) this.db.exec({ sql: "INSERT INTO deployments (tenant_id, deployment_id, deployment_json) VALUES ($tenant_id, $deployment_id, $deployment_json)", bind: { $tenant_id: TENANT, $deployment_id: deployment.deployment_id, $deployment_json: JSON.stringify(deployment) } });
  }
}
