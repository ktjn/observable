import type {
  CreateSavedViewRequest,
  GrantItem,
  GrantListResponse,
  SavedView,
  SavedViewListResponse,
  SignalKind,
  UpdateSavedViewRequest,
} from "../api/savedViews";

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
  CREATE TABLE IF NOT EXISTS saved_views (
    tenant_id TEXT NOT NULL,
    saved_view_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    signal_kind TEXT NOT NULL,
    visibility TEXT NOT NULL,
    config_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
  );
  CREATE TABLE IF NOT EXISTS saved_view_grants (
    tenant_id TEXT NOT NULL,
    saved_view_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    relation TEXT NOT NULL,
    granted_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, saved_view_id, user_id),
    FOREIGN KEY (saved_view_id) REFERENCES saved_views(saved_view_id) ON DELETE CASCADE
  );
`;

const DEMO_VIEW_ID = "playground-view-1";
const DEMO_USER_ID = "playground-user";

function rows<T>(db: SqliteDatabase, options: SqliteExecOptions): T[] {
  return (db.exec({ ...options, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? [];
}

function mapView(row: Record<string, SqliteValue>): SavedView {
  return {
    saved_view_id: String(row.saved_view_id),
    name: String(row.name),
    signal_kind: String(row.signal_kind) as SignalKind,
    visibility: String(row.visibility) as SavedView["visibility"],
    config: JSON.parse(String(row.config_json)) as SavedView["config"],
    created_at: String(row.created_at),
    updated_at: String(row.updated_at),
  };
}

function mapGrant(row: Record<string, SqliteValue>): GrantItem {
  return {
    user_id: String(row.user_id),
    relation: String(row.relation) as GrantItem["relation"],
    granted_at: String(row.granted_at),
  };
}

function newViewId(): string {
  const randomUuid = globalThis.crypto?.randomUUID?.();
  return `playground-view-${randomUuid ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`}`;
}

/** SQLite-backed control-plane repository for browser-local saved views. */
export class SqliteSavedViewRepository {
  private constructor(private readonly db: SqliteDatabase) {}

  static async create(db: SqliteDatabase): Promise<SqliteSavedViewRepository> {
    const repository = new SqliteSavedViewRepository(db);
    repository.db.exec(SCHEMA_SQL);
    repository.seedDemoView();
    return repository;
  }

  static async open(): Promise<SqliteSavedViewRepository> {
    const { default: sqlite3InitModule } = await import("@sqlite.org/sqlite-wasm");
    const sqlite3 = await sqlite3InitModule();
    const db = new sqlite3.oo1.DB(":memory:", "c") as unknown as SqliteDatabase;
    return SqliteSavedViewRepository.create(db);
  }

  private seedDemoView(): void {
    const existing = rows<{ count: number }>(this.db, {
      sql: "SELECT COUNT(*) AS count FROM saved_views WHERE tenant_id = $tenant_id",
      bind: { $tenant_id: "00000000-0000-0000-0000-000000000001" },
    });
    if (Number(existing[0]?.count ?? 0) > 0) return;
    const now = new Date().toISOString();
    this.db.exec({
      sql: `INSERT INTO saved_views
        (tenant_id, saved_view_id, name, signal_kind, visibility, config_json, created_at, updated_at)
        VALUES ($tenant_id, $saved_view_id, $name, $signal_kind, $visibility, $config_json, $created_at, $updated_at)`,
      bind: {
        $tenant_id: "00000000-0000-0000-0000-000000000001",
        $saved_view_id: DEMO_VIEW_ID,
        $name: "Errors only (demo)",
        $signal_kind: "logs",
        $visibility: "private",
        $config_json: JSON.stringify({
          query: null,
          severity_filter: "ERROR",
          time_range: { mode: "preset", preset: "1h" },
          visible_columns: ["timestamp", "severity", "service", "body"],
        }),
        $created_at: now,
        $updated_at: now,
      },
    });
    this.db.exec({
      sql: `INSERT INTO saved_view_grants
        (tenant_id, saved_view_id, user_id, relation, granted_at)
        VALUES ($tenant_id, $saved_view_id, $user_id, $relation, $granted_at)`,
      bind: {
        $tenant_id: "00000000-0000-0000-0000-000000000001",
        $saved_view_id: DEMO_VIEW_ID,
        $user_id: DEMO_USER_ID,
        $relation: "owner",
        $granted_at: now,
      },
    });
  }

  list(tenantId: string, signalKind: SignalKind): SavedViewListResponse {
    return {
      items: rows<Record<string, SqliteValue>>(this.db, {
        sql: `SELECT saved_view_id, name, signal_kind, visibility, config_json, created_at, updated_at
          FROM saved_views WHERE tenant_id = $tenant_id AND signal_kind = $signal_kind
          ORDER BY created_at, saved_view_id`,
        bind: { $tenant_id: tenantId, $signal_kind: signalKind },
      }).map(mapView),
    };
  }

  create(tenantId: string, request: CreateSavedViewRequest): SavedView {
    const now = new Date().toISOString();
    const view: SavedView = {
      saved_view_id: newViewId(),
      name: request.name,
      signal_kind: request.signal_kind,
      visibility: "private",
      config: request.config,
      created_at: now,
      updated_at: now,
    };
    this.db.exec({
      sql: `INSERT INTO saved_views
        (tenant_id, saved_view_id, name, signal_kind, visibility, config_json, created_at, updated_at)
        VALUES ($tenant_id, $saved_view_id, $name, $signal_kind, $visibility, $config_json, $created_at, $updated_at)`,
      bind: {
        $tenant_id: tenantId,
        $saved_view_id: view.saved_view_id,
        $name: view.name,
        $signal_kind: view.signal_kind,
        $visibility: view.visibility,
        $config_json: JSON.stringify(view.config),
        $created_at: view.created_at,
        $updated_at: view.updated_at,
      },
    });
    this.addGrant(tenantId, view.saved_view_id, DEMO_USER_ID, "owner");
    return view;
  }

  update(tenantId: string, savedViewId: string, request: UpdateSavedViewRequest): SavedView {
    const existing = this.get(tenantId, savedViewId);
    if (!existing) throw new Error("Saved view update failed: 404");
    const updated = {
      ...existing,
      name: request.name,
      config: request.config,
      visibility: request.visibility ?? existing.visibility,
      updated_at: new Date().toISOString(),
    } satisfies SavedView;
    this.db.exec({
      sql: `UPDATE saved_views SET name = $name, config_json = $config_json,
        visibility = $visibility, updated_at = $updated_at
        WHERE tenant_id = $tenant_id AND saved_view_id = $saved_view_id`,
      bind: {
        $tenant_id: tenantId,
        $saved_view_id: savedViewId,
        $name: updated.name,
        $config_json: JSON.stringify(updated.config),
        $visibility: updated.visibility,
        $updated_at: updated.updated_at,
      },
    });
    return updated;
  }

  delete(tenantId: string, savedViewId: string): void {
    this.db.exec({
      sql: "DELETE FROM saved_view_grants WHERE tenant_id = $tenant_id AND saved_view_id = $saved_view_id",
      bind: { $tenant_id: tenantId, $saved_view_id: savedViewId },
    });
    this.db.exec({
      sql: "DELETE FROM saved_views WHERE tenant_id = $tenant_id AND saved_view_id = $saved_view_id",
      bind: { $tenant_id: tenantId, $saved_view_id: savedViewId },
    });
  }

  listGrants(tenantId: string, savedViewId: string): GrantListResponse {
    return {
      grants: rows<Record<string, SqliteValue>>(this.db, {
        sql: `SELECT user_id, relation, granted_at FROM saved_view_grants
          WHERE tenant_id = $tenant_id AND saved_view_id = $saved_view_id ORDER BY granted_at, user_id`,
        bind: { $tenant_id: tenantId, $saved_view_id: savedViewId },
      }).map(mapGrant),
    };
  }

  addGrant(tenantId: string, savedViewId: string, userId: string, relation: GrantItem["relation"]): void {
    if (!this.get(tenantId, savedViewId)) throw new Error("Saved view grant add failed: 404");
    this.db.exec({
      sql: `INSERT OR IGNORE INTO saved_view_grants
        (tenant_id, saved_view_id, user_id, relation, granted_at)
        VALUES ($tenant_id, $saved_view_id, $user_id, $relation, $granted_at)`,
      bind: {
        $tenant_id: tenantId,
        $saved_view_id: savedViewId,
        $user_id: userId,
        $relation: relation,
        $granted_at: new Date().toISOString(),
      },
    });
  }

  revokeGrant(tenantId: string, savedViewId: string, userId: string): void {
    this.db.exec({
      sql: "DELETE FROM saved_view_grants WHERE tenant_id = $tenant_id AND saved_view_id = $saved_view_id AND user_id = $user_id",
      bind: { $tenant_id: tenantId, $saved_view_id: savedViewId, $user_id: userId },
    });
  }

  private get(tenantId: string, savedViewId: string): SavedView | undefined {
    const result = rows<Record<string, SqliteValue>>(this.db, {
      sql: `SELECT saved_view_id, name, signal_kind, visibility, config_json, created_at, updated_at
        FROM saved_views WHERE tenant_id = $tenant_id AND saved_view_id = $saved_view_id`,
      bind: { $tenant_id: tenantId, $saved_view_id: savedViewId },
    });
    return result[0] ? mapView(result[0]) : undefined;
  }
}
