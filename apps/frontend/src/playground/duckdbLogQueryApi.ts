export interface DuckDbLogQueryConnection {
  query: (sql: string) => Promise<{ toArray: () => Record<string, unknown>[] }>;
}

const LOG_COLUMNS =
  "log_id, timestamp_unix_nano, observed_timestamp_unix_nano, severity_number, severity_text, body, trace_id, span_id, service_name, environment, host_id";

function escapeSqlString(value: string): string {
  return value.replace(/'/g, "''");
}

/** Browser query-api adapter for log reads backed by DuckDB-WASM. */
export class DuckDbLogQueryApi {
  constructor(private readonly conn: DuckDbLogQueryConnection) {}

  async executePlanned(sql: string): Promise<Record<string, unknown>[]> {
    const result = await this.conn.query(sql);
    return result.toArray();
  }

  async search(traceId: string | undefined, service: string | undefined, limit: number) {
    const conditions: string[] = [];
    if (traceId) conditions.push(`trace_id = '${escapeSqlString(traceId)}'`);
    if (service) conditions.push(`service_name = '${escapeSqlString(service)}'`);
    const where = conditions.length > 0 ? ` WHERE ${conditions.join(" AND ")}` : "";
    const result = await this.conn.query(
      `SELECT ${LOG_COLUMNS} FROM logs${where} ORDER BY timestamp_unix_nano DESC LIMIT ${Math.max(1, Math.floor(limit))}`
    );
    return result.toArray();
  }
}
