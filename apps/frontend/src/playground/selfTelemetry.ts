/**
 * The playground engine's own self-instrumentation (ADR-047 §4, Phase 3):
 * logs about its own behavior (startup, live-tick writes, query cost),
 * written into the same local `logs` table every other log goes into, just
 * under a fixed system tenant instead of the demo tenant. Deliberately not
 * exposed through the tenant switcher (ADR-047 §5's "one implicit tenant"
 * still holds for the user-visible demo data) — reachable only through the
 * dev-only `/playground-self-telemetry` route.
 */

export const SYSTEM_TENANT_ID = "00000000-0000-0000-0000-000000000000";

function escapeSqlString(value: string): string {
  return value.replace(/'/g, "''");
}

export interface SystemLogEntry {
  logId: string;
  nowNs: bigint;
  severityNumber: number;
  severityText: string;
  body: string;
}

/** Same `logs` table/column order every other log insert uses. */
export function insertSystemLogSql(entry: SystemLogEntry): string {
  return (
    `INSERT INTO logs VALUES ('${SYSTEM_TENANT_ID}', '${escapeSqlString(entry.logId)}', ` +
    `${entry.nowNs}, ${entry.nowNs}, ${entry.severityNumber}, '${entry.severityText}', ` +
    `'${escapeSqlString(entry.body)}', '', '', 'playground-engine', 'playground', '')`
  );
}

export interface SystemLogRow {
  tenant_id: string;
  log_id: string;
  timestamp_unix_nano: string;
  observed_timestamp_unix_nano: string;
  severity_number: number;
  severity_text: string;
  body: string;
  service_name: string;
}

export function mapSystemLogRows(raw: Record<string, unknown>[]): SystemLogRow[] {
  return raw.map((row) => ({
    tenant_id: SYSTEM_TENANT_ID,
    log_id: String(row.log_id),
    timestamp_unix_nano: String(row.timestamp_unix_nano),
    observed_timestamp_unix_nano: String(row.observed_timestamp_unix_nano),
    severity_number: Number(row.severity_number),
    severity_text: String(row.severity_text),
    body: String(row.body),
    service_name: String(row.service_name),
  }));
}
