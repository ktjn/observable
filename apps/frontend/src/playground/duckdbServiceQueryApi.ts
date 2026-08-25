export interface DuckDbServiceQueryConnection {
  query: (sql: string) => Promise<{ toArray: () => Record<string, unknown>[] }>;
}

/** Browser query-api adapter for service summaries, topology, and names. */
export class DuckDbServiceQueryApi {
  constructor(private readonly conn: DuckDbServiceQueryConnection) {}

  async executePlanned(sql: string): Promise<Record<string, unknown>[]> {
    const result = await this.conn.query(sql);
    return result.toArray();
  }

  async listNames(): Promise<Record<string, unknown>[]> {
    const result = await this.conn.query(
      "SELECT DISTINCT service_name FROM spans WHERE service_name != '' ORDER BY service_name"
    );
    return result.toArray();
  }
}
