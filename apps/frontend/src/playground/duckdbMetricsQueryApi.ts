export interface DuckDbMetricsQueryConnection {
  query: (sql: string) => Promise<{ toArray: () => Record<string, unknown>[] }>;
}

/** Browser query-api adapter for metric catalog and point reads. */
export class DuckDbMetricsQueryApi {
  constructor(private readonly conn: DuckDbMetricsQueryConnection) {}

  async executePlanned(sql: string): Promise<Record<string, unknown>[]> {
    const result = await this.conn.query(sql);
    return result.toArray();
  }
}
