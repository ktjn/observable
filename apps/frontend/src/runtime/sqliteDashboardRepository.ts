import type {
  CreateDashboardRequest,
  Dashboard,
  DashboardListResponse,
  DashboardPanel,
  UpdateDashboardRequest,
} from "../api/dashboards";

type SqliteValue = string | number | null;

interface SqliteExecOptions {
  sql: string;
  bind?: Record<string, SqliteValue>;
  returnValue?: "resultRows";
  rowMode?: "object";
}

interface SqliteDatabase {
  exec(sql: string): unknown;
  exec(options: SqliteExecOptions): unknown;
}

const SCHEMA_SQL = `
  CREATE TABLE IF NOT EXISTS dashboards (
    tenant_id TEXT NOT NULL,
    dashboard_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    visibility TEXT NOT NULL,
    panels_json TEXT NOT NULL,
    created_at TEXT NOT NULL
  );
`;

function rows<T>(db: SqliteDatabase, options: SqliteExecOptions): T[] {
  return (db.exec({ ...options, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? [];
}

function mapDashboard(row: Record<string, SqliteValue>): Dashboard {
  return {
    dashboard_id: String(row.dashboard_id),
    name: String(row.name),
    visibility: String(row.visibility) as Dashboard["visibility"],
    panels: JSON.parse(String(row.panels_json)) as DashboardPanel[],
    created_at: String(row.created_at),
  };
}

function newId(prefix: string): string {
  const randomUuid = globalThis.crypto?.randomUUID?.();
  return `playground-${prefix}-${randomUuid ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`}`;
}

function panelFromCreateRequest(panel: CreateDashboardRequest["panels"][number]): DashboardPanel {
  return {
    panel_id: newId("panel"),
    title: panel.title,
    panel_kind: panel.panel_kind ?? "query",
    query_kind: panel.query_kind ?? undefined,
    service: panel.service,
    preset: panel.preset ?? undefined,
    filters: panel.filters,
    query_text: panel.query_text ?? undefined,
    content: panel.content ?? undefined,
    layout: panel.layout ?? { x: 0, y: 0, w: 6, h: 4 },
    time_range: panel.time_range ?? { mode: "global" },
  };
}

function panelFromUpdateRequest(panel: UpdateDashboardRequest["panels"][number]): DashboardPanel {
  return {
    panel_id: panel.panel_id ?? newId("panel"),
    title: panel.title,
    panel_kind: panel.panel_kind,
    query_kind: panel.query_kind ?? undefined,
    service: panel.service ?? undefined,
    preset: panel.preset ?? undefined,
    filters: panel.filters,
    query_text: panel.query_text ?? undefined,
    content: panel.content ?? undefined,
    layout: panel.layout,
    time_range: panel.time_range,
  };
}

/** SQLite-backed control-plane repository for browser-local dashboards. */
export class SqliteDashboardRepository {
  private constructor(private readonly db: SqliteDatabase) {}

  static async create(db: SqliteDatabase): Promise<SqliteDashboardRepository> {
    const repository = new SqliteDashboardRepository(db);
    repository.db.exec(SCHEMA_SQL);
    return repository;
  }

  static async open(): Promise<SqliteDashboardRepository> {
    const { default: sqlite3InitModule } = await import("@sqlite.org/sqlite-wasm");
    const sqlite3 = await sqlite3InitModule();
    const db = new sqlite3.oo1.DB(":memory:", "c") as unknown as SqliteDatabase;
    return SqliteDashboardRepository.create(db);
  }

  list(tenantId: string): DashboardListResponse {
    return {
      items: rows<Record<string, SqliteValue>>(this.db, {
        sql: `SELECT dashboard_id, name, visibility, panels_json, created_at
          FROM dashboards WHERE tenant_id = $tenant_id ORDER BY created_at, dashboard_id`,
        bind: { $tenant_id: tenantId },
      }).map(mapDashboard),
    };
  }

  get(tenantId: string, dashboardId: string): Dashboard {
    const result = rows<Record<string, SqliteValue>>(this.db, {
      sql: `SELECT dashboard_id, name, visibility, panels_json, created_at
        FROM dashboards WHERE tenant_id = $tenant_id AND dashboard_id = $dashboard_id`,
      bind: { $tenant_id: tenantId, $dashboard_id: dashboardId },
    });
    const dashboard = result[0] ? mapDashboard(result[0]) : undefined;
    if (!dashboard) throw new Error(`Dashboard not found: ${dashboardId}`);
    return dashboard;
  }

  create(tenantId: string, request: CreateDashboardRequest): Dashboard {
    const dashboard: Dashboard = {
      dashboard_id: newId("dashboard"),
      name: request.name,
      visibility: "private",
      panels: request.panels.map(panelFromCreateRequest),
      created_at: new Date().toISOString(),
    };
    this.insert(tenantId, dashboard);
    return dashboard;
  }

  update(tenantId: string, dashboardId: string, request: UpdateDashboardRequest): Dashboard {
    const existing = this.get(tenantId, dashboardId);
    const updated: Dashboard = {
      ...existing,
      name: request.name,
      panels: request.panels.map(panelFromUpdateRequest),
    };
    this.db.exec({
      sql: `UPDATE dashboards SET name = $name, panels_json = $panels_json
        WHERE tenant_id = $tenant_id AND dashboard_id = $dashboard_id`,
      bind: {
        $tenant_id: tenantId,
        $dashboard_id: dashboardId,
        $name: updated.name,
        $panels_json: JSON.stringify(updated.panels),
      },
    });
    return updated;
  }

  delete(tenantId: string, dashboardId: string): void {
    this.db.exec({
      sql: "DELETE FROM dashboards WHERE tenant_id = $tenant_id AND dashboard_id = $dashboard_id",
      bind: { $tenant_id: tenantId, $dashboard_id: dashboardId },
    });
  }

  private insert(tenantId: string, dashboard: Dashboard): void {
    this.db.exec({
      sql: `INSERT INTO dashboards
        (tenant_id, dashboard_id, name, visibility, panels_json, created_at)
        VALUES ($tenant_id, $dashboard_id, $name, $visibility, $panels_json, $created_at)`,
      bind: {
        $tenant_id: tenantId,
        $dashboard_id: dashboard.dashboard_id,
        $name: dashboard.name,
        $visibility: dashboard.visibility,
        $panels_json: JSON.stringify(dashboard.panels),
        $created_at: dashboard.created_at,
      },
    });
  }
}
