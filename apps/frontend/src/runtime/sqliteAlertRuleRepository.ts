import type {
  AlertRuleItem,
  AlertRuleListResponse,
  CreateRuleRequest,
} from "../api/alerts";

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

const DEMO_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const SCHEMA_SQL = `
  CREATE TABLE IF NOT EXISTS alert_rules (
    tenant_id TEXT NOT NULL,
    rule_id TEXT PRIMARY KEY,
    rule_json TEXT NOT NULL,
    runbook_url TEXT
  );
`;

function rows<T>(db: SqliteDatabase, options: SqliteExecOptions): T[] {
  return (db.exec({ ...options, returnValue: "resultRows", rowMode: "object" }) as T[]) ?? [];
}

function newRuleId(): string {
  const randomUuid = globalThis.crypto?.randomUUID?.();
  return `playground-rule-${randomUuid ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`}`;
}

function mapRow(row: Record<string, SqliteValue>): AlertRuleItem {
  return JSON.parse(String(row.rule_json)) as AlertRuleItem;
}

function seedRules(): AlertRuleItem[] {
  const now = new Date().toISOString();
  return [
    { rule_id: "playground-rule-1", name: "payment error rate > 5%", metric_name: "http.server.errors.rate", operator: "gt", threshold: 5, severity: "critical", silenced: false, state: "active", firing: true, last_fired_at: new Date(Date.now() - 600_000).toISOString(), notification_channels: [], auto_trigger_incident: true, service_name: "payment", suppressed: false },
    { rule_id: "playground-rule-2", name: "checkout p95 latency > 500ms", metric_name: "http.server.duration", operator: "gt", threshold: 500, severity: "warning", silenced: false, state: "ok", firing: false, notification_channels: [], auto_trigger_incident: false, service_name: "checkout", suppressed: false },
    { rule_id: "playground-rule-3", name: "web request rate drop", metric_name: "http.server.request.rate", operator: "lt", threshold: 1, severity: "warning", silenced: false, state: "ok", firing: false, last_fired_at: now, notification_channels: [], auto_trigger_incident: false, service_name: "web", suppressed: false },
  ];
}

/** SQLite-backed control-plane repository for browser-local alert rules. */
export class SqliteAlertRuleRepository {
  private constructor(private readonly db: SqliteDatabase) {}

  static async open(): Promise<SqliteAlertRuleRepository> {
    const { default: sqlite3InitModule } = await import("@sqlite.org/sqlite-wasm");
    const sqlite3 = await sqlite3InitModule();
    const db = new sqlite3.oo1.DB(":memory:", "c") as unknown as SqliteDatabase;
    const repository = new SqliteAlertRuleRepository(db);
    db.exec(SCHEMA_SQL);
    repository.seed();
    return repository;
  }

  list(tenantId: string): AlertRuleListResponse {
    return { items: rows<Record<string, SqliteValue>>(this.db, { sql: "SELECT rule_json FROM alert_rules WHERE tenant_id = $tenant_id ORDER BY rule_id", bind: { $tenant_id: tenantId } }).map(mapRow) };
  }

  get(tenantId: string, ruleId: string): AlertRuleItem {
    const result = rows<Record<string, SqliteValue>>(this.db, { sql: "SELECT rule_json FROM alert_rules WHERE tenant_id = $tenant_id AND rule_id = $rule_id", bind: { $tenant_id: tenantId, $rule_id: ruleId } });
    if (!result[0]) throw new Error("Failed to get alert rule: 404");
    return mapRow(result[0]);
  }

  getRunbook(tenantId: string, ruleId: string): string | null {
    const result = rows<Record<string, SqliteValue>>(this.db, { sql: "SELECT runbook_url FROM alert_rules WHERE tenant_id = $tenant_id AND rule_id = $rule_id", bind: { $tenant_id: tenantId, $rule_id: ruleId } });
    if (!result[0]) throw new Error("Failed to get alert rule: 404");
    return result[0].runbook_url == null ? null : String(result[0].runbook_url);
  }

  create(tenantId: string, request: CreateRuleRequest): string {
    const rule: AlertRuleItem = { rule_id: newRuleId(), name: request.name, metric_name: request.metric_name, operator: (request.operator as AlertRuleItem["operator"]) ?? "gt", threshold: request.threshold, severity: "warning", silenced: false, state: "ok", firing: false, notification_channels: request.notification_channels ?? [], auto_trigger_incident: request.auto_trigger_incident ?? false, service_name: request.service_name, suppressed: false };
    this.insert(tenantId, rule, request.runbook_url ?? null);
    return rule.rule_id;
  }

  setSilenced(tenantId: string, ruleId: string, silenced: boolean): void {
    const rule = this.get(tenantId, ruleId);
    rule.silenced = silenced;
    rule.state = silenced ? "silenced" : rule.firing ? "active" : "ok";
    this.update(tenantId, ruleId, rule);
  }

  setRunbook(tenantId: string, ruleId: string, runbookUrl: string | null): void {
    this.get(tenantId, ruleId);
    this.db.exec({ sql: "UPDATE alert_rules SET runbook_url = $runbook_url WHERE tenant_id = $tenant_id AND rule_id = $rule_id", bind: { $tenant_id: tenantId, $rule_id: ruleId, $runbook_url: runbookUrl } });
  }

  private seed(): void {
    for (const rule of seedRules()) this.insert(DEMO_TENANT_ID, rule, null);
  }

  private insert(tenantId: string, rule: AlertRuleItem, runbookUrl: string | null): void {
    this.db.exec({ sql: "INSERT OR IGNORE INTO alert_rules (tenant_id, rule_id, rule_json, runbook_url) VALUES ($tenant_id, $rule_id, $rule_json, $runbook_url)", bind: { $tenant_id: tenantId, $rule_id: rule.rule_id, $rule_json: JSON.stringify(rule), $runbook_url: runbookUrl } });
  }

  private update(tenantId: string, ruleId: string, rule: AlertRuleItem): void {
    this.db.exec({ sql: "UPDATE alert_rules SET rule_json = $rule_json WHERE tenant_id = $tenant_id AND rule_id = $rule_id", bind: { $tenant_id: tenantId, $rule_id: ruleId, $rule_json: JSON.stringify(rule) } });
  }
}
