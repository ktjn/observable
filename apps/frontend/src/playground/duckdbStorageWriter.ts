export interface DuckDbConnection {
  query: (sql: string) => Promise<unknown>;
}

export interface ProcessedSpan {
  tenant_id: string;
  trace_id: string;
  span_id: string;
  parent_span_id: string | null;
  service_name: string;
  operation_name: string;
  duration_ns: string;
  status_code: string;
  environment: string;
  start_time_unix_nano: string;
}

export interface ProcessedLog {
  tenant_id: string;
  log_id: string;
  timestamp_unix_nano: string;
  observed_timestamp_unix_nano: string;
  severity_number: number;
  severity_text: string;
  body: string;
  trace_id: string;
  span_id: string;
  service_name: string;
  environment: string;
  host_id: string;
}

export interface ProcessedMetricSeries {
  tenant_id: string;
  metric_series_id: string;
  metric_name: string;
  description: string;
  unit: string;
  metric_type: string;
  is_monotonic: boolean;
  aggregation_temporality: string;
  service_name: string;
  environment: string;
}

export interface ProcessedMetricPoint {
  tenant_id: string;
  metric_series_id: string;
  time_unix_nano: string;
  start_time_unix_nano: string;
  value_double: number;
}

export interface StorageTelemetryBatch {
  spans: ProcessedSpan[];
  logs: ProcessedLog[];
  series: ProcessedMetricSeries[];
  points: ProcessedMetricPoint[];
}

function escapeSqlString(value: string): string {
  return value.replace(/'/g, "''");
}

function insertSpansSql(spans: ProcessedSpan[]): string {
  const values = spans
    .map(
      (s) =>
        `('${escapeSqlString(s.tenant_id)}', '${escapeSqlString(s.trace_id)}', '${escapeSqlString(s.span_id)}', ` +
        `${s.parent_span_id === null ? "NULL" : `'${escapeSqlString(s.parent_span_id)}'`}, '${escapeSqlString(s.service_name)}', ` +
        `'${escapeSqlString(s.operation_name)}', ${s.duration_ns}, ` +
        `'${escapeSqlString(s.status_code)}', '${escapeSqlString(s.environment)}', ${s.start_time_unix_nano})`
    )
    .join(", ");
  return `INSERT INTO spans VALUES ${values}`;
}

function insertLogsSql(logs: ProcessedLog[]): string {
  const values = logs
    .map(
      (l) =>
        `('${escapeSqlString(l.tenant_id)}', '${escapeSqlString(l.log_id)}', ${l.timestamp_unix_nano}, ${l.observed_timestamp_unix_nano}, ` +
        `${l.severity_number}, '${escapeSqlString(l.severity_text)}', '${escapeSqlString(l.body)}', ` +
        `'${escapeSqlString(l.trace_id)}', '${escapeSqlString(l.span_id)}', '${escapeSqlString(l.service_name)}', ` +
        `'${escapeSqlString(l.environment)}', '${escapeSqlString(l.host_id)}')`
    )
    .join(", ");
  return `INSERT INTO logs VALUES ${values}`;
}

function insertMetricSeriesSql(series: ProcessedMetricSeries[]): string {
  const values = series
    .map(
      (s) =>
        `('${escapeSqlString(s.tenant_id)}', '${escapeSqlString(s.metric_series_id)}', '${escapeSqlString(s.metric_name)}', ` +
        `'${escapeSqlString(s.description)}', '${escapeSqlString(s.unit)}', '${escapeSqlString(s.metric_type)}', ` +
        `${s.is_monotonic}, '${escapeSqlString(s.aggregation_temporality)}', ` +
        `'${escapeSqlString(s.service_name)}', '${escapeSqlString(s.environment)}')`
    )
    .join(", ");
  return `INSERT INTO metric_series VALUES ${values}`;
}

function insertMetricPointsSql(points: ProcessedMetricPoint[]): string {
  const values = points
    .map(
      (p) =>
        `('${escapeSqlString(p.tenant_id)}', '${escapeSqlString(p.metric_series_id)}', ${p.time_unix_nano}, ${p.start_time_unix_nano}, ${p.value_double})`
    )
    .join(", ");
  return `INSERT INTO metric_points VALUES ${values}`;
}

/** Browser adapter for the storage-writer boundary backed by DuckDB-WASM. */
export class DuckDbStorageWriter {
  constructor(private readonly conn: DuckDbConnection) {}

  async write(batch: StorageTelemetryBatch): Promise<void> {
    const writes: Promise<unknown>[] = [];
    if (batch.spans.length > 0) writes.push(this.conn.query(insertSpansSql(batch.spans)));
    if (batch.logs.length > 0) writes.push(this.conn.query(insertLogsSql(batch.logs)));
    if (batch.series.length > 0) writes.push(this.conn.query(insertMetricSeriesSql(batch.series)));
    if (batch.points.length > 0) writes.push(this.conn.query(insertMetricPointsSql(batch.points)));
    await Promise.all(writes);
  }
}
