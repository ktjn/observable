import type { IncidentItem, IncidentListResponse, IncidentDetailResponse } from "../api/incidents";

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

  getDetail(tenantId: string, incidentId: string): IncidentDetailResponse | null {
    const incident = this.get(tenantId, incidentId);
    if (!incident) return null;
    return {
      ...incident,
      dedup_key: `dedup-${incident.incident_id}`,
      acknowledged_at: (incident as unknown as { acknowledged_at?: string }).acknowledged_at ?? null,
      acknowledged_by: (incident as unknown as { acknowledged_by?: string }).acknowledged_by ?? null,
      resolved_at: incident.resolved_at ?? null,
      resolved_by: (incident as unknown as { resolved_by?: string }).resolved_by ?? null,
      triggered_by_rule_id: incident.triggered_by_rule_id ?? null,
      related_deployment_id: (incident as unknown as { related_deployment_id?: string }).related_deployment_id ?? null,
      runbook_url: null,
      rule_name: incident.triggered_by_rule_id === "playground-rule-1" ? "payment error rate > 5%" : null,
      timeline: (incident as unknown as { timeline?: IncidentDetailResponse["timeline"] }).timeline ?? [{
        event_time: incident.triggered_at,
        event_type: "triggered",
        actor: "alert-evaluator",
        message: `Incident triggered: ${incident.title}`,
      }],
      impacted_service: incident.triggered_by_rule_id === "playground-rule-1" ? "payment" : null,
    };
  }

  acknowledge(
    tenantId: string,
    incidentId: string,
    payload?: { actor?: string; message?: string }
  ): IncidentDetailResponse {
    const detail = this.getDetail(tenantId, incidentId);
    if (!detail) throw new Error("Incident not found");
    const now = new Date().toISOString();
    const actor = payload?.actor ?? "playground-user";
    const updated: IncidentDetailResponse = {
      ...detail,
      status: "acknowledged",
      acknowledged_at: now,
      acknowledged_by: actor,
      timeline: [
        ...detail.timeline,
        {
          event_time: now,
          event_type: "acknowledged",
          actor,
          message: payload?.message ?? "Incident acknowledged",
        },
      ],
    };
    this.insert(tenantId, updated as unknown as IncidentItem);
    return updated;
  }

  updateStatus(
    tenantId: string,
    incidentId: string,
    status: string,
    payload?: { actor?: string; message?: string }
  ): IncidentDetailResponse {
    const detail = this.getDetail(tenantId, incidentId);
    if (!detail) throw new Error("Incident not found");
    const now = new Date().toISOString();
    const actor = payload?.actor ?? "playground-user";
    const resolved_at = status === "resolved" ? (detail.resolved_at ?? now) : detail.resolved_at;
    const resolved_by = status === "resolved" ? (detail.resolved_by ?? actor) : detail.resolved_by;
    const updated: IncidentDetailResponse = {
      ...detail,
      status,
      resolved_at,
      resolved_by,
      timeline: [
        ...detail.timeline,
        {
          event_time: now,
          event_type: "status_change",
          actor,
          message: payload?.message ?? `Status updated to ${status}`,
        },
      ],
    };
    this.insert(tenantId, updated as unknown as IncidentItem);
    return updated;
  }

  resolve(
    tenantId: string,
    incidentId: string,
    payload?: { actor?: string; message?: string }
  ): IncidentDetailResponse {
    const detail = this.getDetail(tenantId, incidentId);
    if (!detail) throw new Error("Incident not found");
    const now = new Date().toISOString();
    const actor = payload?.actor ?? "playground-user";
    const updated: IncidentDetailResponse = {
      ...detail,
      status: "resolved",
      resolved_at: now,
      resolved_by: actor,
      timeline: [
        ...detail.timeline,
        {
          event_time: now,
          event_type: "status_change",
          actor,
          message: payload?.message ?? "Incident marked resolved",
        },
      ],
    };
    this.insert(tenantId, updated as unknown as IncidentItem);
    return updated;
  }

  addComment(
    tenantId: string,
    incidentId: string,
    message: string,
    actor?: string
  ): IncidentDetailResponse {
    const detail = this.getDetail(tenantId, incidentId);
    if (!detail) throw new Error("Incident not found");
    const now = new Date().toISOString();
    const commentActor = actor ?? "playground-user";
    const updated: IncidentDetailResponse = {
      ...detail,
      timeline: [
        ...detail.timeline,
        {
          event_time: now,
          event_type: "comment",
          actor: commentActor,
          message,
        },
      ],
    };
    this.insert(tenantId, updated as unknown as IncidentItem);
    return updated;
  }

  linkDeployment(
    tenantId: string,
    incidentId: string,
    deploymentId: string,
    payload?: { actor?: string; message?: string }
  ): IncidentDetailResponse {
    const detail = this.getDetail(tenantId, incidentId);
    if (!detail) throw new Error("Incident not found");
    const now = new Date().toISOString();
    const actor = payload?.actor ?? "playground-user";
    const updated: IncidentDetailResponse = {
      ...detail,
      related_deployment_id: deploymentId,
      timeline: [
        ...detail.timeline,
        {
          event_time: now,
          event_type: "deployment_linked",
          actor,
          message: payload?.message ?? `Linked deployment ${deploymentId}`,
        },
      ],
    };
    this.insert(tenantId, updated as unknown as IncidentItem);
    return updated;
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
