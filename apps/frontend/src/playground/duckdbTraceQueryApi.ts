export interface DuckDbQueryConnection {
  query: (sql: string) => Promise<{ toArray: () => Record<string, unknown>[] }>;
}

/** Browser query-api adapter for the trace tables owned by DuckDB-WASM. */
export class DuckDbTraceQueryApi {
  constructor(private readonly conn: DuckDbQueryConnection) {}

  async executePlanned(sql: string): Promise<Record<string, unknown>[]> {
    const result = await this.conn.query(sql);
    return result.toArray();
  }

  async findTrace(traceId: string): Promise<Record<string, unknown>[]> {
    const escapedTraceId = traceId.replace(/'/g, "''");
    const result = await this.conn.query(
      "SELECT trace_id, span_id, parent_span_id, service_name, operation_name, duration_ns, " +
        "status_code, environment, start_time_unix_nano FROM spans " +
        `WHERE trace_id = '${escapedTraceId}' ORDER BY start_time_unix_nano`
    );
    return result.toArray();
  }
}
