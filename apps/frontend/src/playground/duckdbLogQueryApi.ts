export interface DuckDbLogQueryConnection {
  query: (sql: string) => Promise<{ toArray: () => Record<string, unknown>[] }>;
}

const LOG_COLUMNS =
  "log_id, timestamp_unix_nano, observed_timestamp_unix_nano, severity_number, severity_text, body, trace_id, span_id, service_name, environment, host_id";

/** Mirrors query-core's planner.rs::requested_log_facets allowed field set. */
const FACETABLE_FIELDS = new Set(["service_name", "severity_number", "environment", "host_id"]);

function escapeSqlString(value: string): string {
  return value.replace(/'/g, "''");
}

export interface LogSearchFilters {
  tenantId?: string;
  traceId?: string;
  spanId?: string;
  service?: string;
  severity?: number;
  from?: string;
  to?: string;
  limit: number;
  facets?: string[];
}

export interface LogFacetValue {
  value: string;
  count: number;
}

export interface LogSearchResult {
  rows: Record<string, unknown>[];
  total: number;
  facets: Record<string, LogFacetValue[]>;
}

/** Browser query-api adapter for log reads backed by DuckDB-WASM. */
export class DuckDbLogQueryApi {
  constructor(private readonly conn: DuckDbLogQueryConnection) {}

  async executePlanned(sql: string): Promise<Record<string, unknown>[]> {
    const result = await this.conn.query(sql);
    return result.toArray();
  }

  private buildWhere(filters: LogSearchFilters): string {
    const conditions: string[] = [];
    if (filters.tenantId) conditions.push(`tenant_id = '${escapeSqlString(filters.tenantId)}'`);
    if (filters.traceId) conditions.push(`trace_id = '${escapeSqlString(filters.traceId)}'`);
    if (filters.spanId) conditions.push(`span_id = '${escapeSqlString(filters.spanId)}'`);
    if (filters.service) conditions.push(`service_name = '${escapeSqlString(filters.service)}'`);
    if (filters.severity !== undefined) conditions.push(`severity_number >= ${Math.floor(filters.severity)}`);
    if (filters.from && /^\d+$/.test(filters.from)) conditions.push(`timestamp_unix_nano >= ${filters.from}`);
    if (filters.to && /^\d+$/.test(filters.to)) conditions.push(`timestamp_unix_nano <= ${filters.to}`);
    return conditions.length > 0 ? ` WHERE ${conditions.join(" AND ")}` : "";
  }

  /** `search_logs` semantics (facets, total count, tenant bind) over the local `logs` table. */
  async search(filters: LogSearchFilters): Promise<LogSearchResult> {
    const where = this.buildWhere(filters);
    const limit = Math.max(1, Math.floor(filters.limit));

    const rowsResult = await this.conn.query(
      `SELECT ${LOG_COLUMNS} FROM logs${where} ORDER BY timestamp_unix_nano DESC LIMIT ${limit}`
    );
    const countResult = await this.conn.query(`SELECT count(*) AS total FROM logs${where}`);
    const total = Number(countResult.toArray()[0]?.total ?? 0);

    const facets: Record<string, LogFacetValue[]> = {};
    for (const field of filters.facets ?? []) {
      if (!FACETABLE_FIELDS.has(field)) continue;
      const facetResult = await this.conn.query(
        `SELECT ${field} AS value, count(*) AS count FROM logs${where} GROUP BY ${field} ORDER BY count DESC LIMIT 10`
      );
      facets[field] = facetResult.toArray().map((row) => ({
        value: String(row.value),
        count: Number(row.count),
      }));
    }

    return { rows: rowsResult.toArray(), total, facets };
  }
}
