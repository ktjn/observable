// Persistent playground query engine worker. Unlike worker.ts (the Phase 0
// spike, which spawns a new worker per call and terminates it), this worker
// stays alive for the page's lifetime: DuckDB-WASM initialization is done
// once (memoized), then reused across queries and resets.
export {};

// Must match useTenantContext.tsx's DEFAULT_TENANT_ID / playgroundRuntime.ts's
// DEMO_TENANT_ID — the seeded "observable" tenant. Logs need a tenant_id to
// satisfy the frontend's LogRecord shape, but the local `logs` table itself
// is single-tenant, so this is attached at read time rather than stored.
const DEMO_TENANT_ID = "00000000-0000-0000-0000-000000000001";

interface NlqTraceRow {
  trace_id: string;
  root_service: string;
  root_operation: string;
  duration_ms: number;
  status_code: string;
  environment?: string;
  start_time_unix_nano: number | string;
}

interface NlqLogRow {
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
  attributes: Record<string, unknown>;
  resource_attributes: Record<string, unknown>;
}

interface TraceHistogramBucket {
  start_ms: number;
  end_ms: number;
  count: number;
}

interface LogHistogramBucket {
  start_ms: number;
  end_ms: number;
  counts: Record<number, number>;
}

interface ServiceSummary {
  service_name: string;
  request_rate: number;
  error_rate: number;
  p95_latency_ms: number;
  health_state: "healthy" | "watch" | "breach";
  active_alert_count: number;
  latest_deployment: string | null;
}

interface ResponseTimeHistoryBucket {
  start_ms: number;
  end_ms: number;
  p50_ms: number;
  p95_ms: number;
  request_rate: number;
}

interface TopologyEdge {
  caller: string;
  callee: string;
  request_count: number;
  error_rate: number;
  p95_latency_ms: number;
}

interface MetricCatalogEntry {
  tenant_id: string;
  metric_name: string;
  description: string;
  unit: string;
  metric_type: string;
  is_monotonic?: boolean;
  aggregation_temporality?: string;
  service_name: string;
  environment: string;
  series_count: number;
}

interface MetricPoint {
  tenant_id: string;
  metric_series_id: string;
  metric_name: string;
  service_name: string;
  time_unix_nano: number;
  start_time_unix_nano?: number;
  value_double?: number;
}

interface GeneratedMetricSeries {
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

interface GeneratedMetricPoint {
  metric_series_id: string;
  time_unix_nano: string;
  start_time_unix_nano: string;
  value_double: number;
}

interface ChangeEvent {
  change_event_id: string;
  tenant_id: string;
  project_id: string | null;
  event_type: string;
  service_name: string | null;
  environment: string;
  title: string;
  description: string | null;
  occurred_at: string;
  source: string | null;
  created_by: string | null;
  metadata: Record<string, unknown> | null;
}

interface GeneratedChangeEvent {
  change_event_id: string;
  event_type: string;
  service_name: string;
  environment: string;
  title: string;
  description: string;
  occurred_at_unix_nano: string;
  source: string;
}

interface GeneratedSpan {
  trace_id: string;
  span_id: string;
  parent_span_id: string;
  service_name: string;
  operation_name: string;
  duration_ns: string;
  status_code: string;
  environment: string;
  start_time_unix_nano: string;
}

interface GeneratedLog {
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

type EngineRequest =
  | { type: "nlq-execute-trace-table"; requestId: string; ir: unknown }
  | {
      type: "trace-histogram";
      requestId: string;
      fromNs: string;
      toNs: string;
      bucketCount: number;
      service?: string;
    }
  | { type: "nlq-execute-log-table"; requestId: string; ir: unknown }
  | {
      type: "log-histogram";
      requestId: string;
      fromNs: string;
      toNs: string;
      bucketCount: number;
      service?: string;
    }
  | {
      type: "service-summaries";
      requestId: string;
      fromNs: string;
      toNs: string;
      environment?: string;
    }
  | {
      type: "topology";
      requestId: string;
      fromNs: string;
      toNs: string;
      environment?: string;
      service?: string;
    }
  | { type: "service-names"; requestId: string }
  | { type: "metric-catalog"; requestId: string; service?: string }
  | { type: "metric-group-points"; requestId: string; metric: MetricCatalogEntry }
  | {
      type: "change-events";
      requestId: string;
      fromNs: string;
      toNs: string;
      service?: string;
      environment?: string;
      eventType?: string;
      limit: number;
    }
  | {
      type: "response-time-histogram";
      requestId: string;
      fromNs: string;
      toNs: string;
      bucketCount: number;
      serviceName: string;
    }
  | { type: "reset"; requestId: string; seed: number };
type EngineResponse =
  | { type: "nlq-result"; requestId: string; rows: NlqTraceRow[]; sql: string }
  | { type: "histogram-result"; requestId: string; buckets: TraceHistogramBucket[] }
  | { type: "nlq-log-result"; requestId: string; rows: NlqLogRow[]; sql: string }
  | { type: "log-histogram-result"; requestId: string; buckets: LogHistogramBucket[] }
  | { type: "service-summaries-result"; requestId: string; items: ServiceSummary[] }
  | { type: "topology-result"; requestId: string; edges: TopologyEdge[] }
  | { type: "service-names-result"; requestId: string; names: string[] }
  | { type: "metric-catalog-result"; requestId: string; metrics: MetricCatalogEntry[] }
  | { type: "metric-group-points-result"; requestId: string; points: MetricPoint[] }
  | { type: "change-events-result"; requestId: string; items: ChangeEvent[] }
  | { type: "response-time-histogram-result"; requestId: string; buckets: ResponseTimeHistoryBucket[] }
  | { type: "reset-done"; requestId: string }
  | { type: "nlq-error"; requestId: string; message: string };

interface WorkerSelf {
  onmessage: ((event: MessageEvent<EngineRequest>) => void) | null;
  postMessage(message: EngineResponse): void;
}
const workerSelf = self as unknown as WorkerSelf;

interface EngineState {
  generateSpansJson: (seed: number, nowUnixNano: string) => string;
  generateLogsJson: (seed: number, nowUnixNano: string) => string;
  renderTraceSearchSql: (irJson: string) => string;
  renderTraceHistogramSql: (
    fromNs: string,
    toNs: string,
    bucketCount: number,
    service: string | undefined
  ) => string;
  renderLogSearchSql: (irJson: string) => string;
  renderLogHistogramSql: (
    fromNs: string,
    toNs: string,
    bucketCount: number,
    service: string | undefined
  ) => string;
  renderServiceSummarySql: (fromNs: string, toNs: string, environment: string | undefined) => string;
  computeServiceSummaries: (rowsJson: string, durationSecs: number) => string;
  renderTopologySql: (
    fromNs: string,
    toNs: string,
    environment: string | undefined,
    service: string | undefined
  ) => string;
  computeTopologyEdges: (rowsJson: string) => string;
  generateMetricsJson: (seed: number, nowUnixNano: string) => string;
  renderMetricCatalogSql: (service: string | undefined) => string;
  renderMetricGroupPointsSql: (
    metricName: string,
    service: string,
    environment: string,
    metricType: string,
    unit: string
  ) => string;
  generateChangeEventsJson: (seed: number, nowUnixNano: string) => string;
  renderChangeEventsSql: (
    serviceName: string | undefined,
    environment: string | undefined,
    eventType: string | undefined,
    fromNs: string,
    toNs: string,
    limit: number
  ) => string;
  renderResponseTimeHistogramSql: (
    fromNs: string,
    toNs: string,
    bucketCount: number,
    serviceName: string
  ) => string;
  conn: { query: (sql: string) => Promise<{ toArray: () => Record<string, unknown>[] }> };
}

let statePromise: Promise<EngineState> | null = null;
let currentSeed = 1;

function escapeSqlString(value: string): string {
  return value.replace(/'/g, "''");
}

function insertSpansSql(spans: GeneratedSpan[]): string {
  const values = spans
    .map(
      (s) =>
        `('${escapeSqlString(s.trace_id)}', '${escapeSqlString(s.span_id)}', ` +
        `'${escapeSqlString(s.parent_span_id)}', '${escapeSqlString(s.service_name)}', ` +
        `'${escapeSqlString(s.operation_name)}', ${s.duration_ns}, ` +
        `'${escapeSqlString(s.status_code)}', '${escapeSqlString(s.environment)}', ${s.start_time_unix_nano})`
    )
    .join(", ");
  return `INSERT INTO spans VALUES ${values}`;
}

function insertMetricSeriesSql(series: GeneratedMetricSeries[]): string {
  const values = series
    .map(
      (s) =>
        `('${escapeSqlString(s.metric_series_id)}', '${escapeSqlString(s.metric_name)}', ` +
        `'${escapeSqlString(s.description)}', '${escapeSqlString(s.unit)}', '${escapeSqlString(s.metric_type)}', ` +
        `${s.is_monotonic}, '${escapeSqlString(s.aggregation_temporality)}', ` +
        `'${escapeSqlString(s.service_name)}', '${escapeSqlString(s.environment)}')`
    )
    .join(", ");
  return `INSERT INTO metric_series VALUES ${values}`;
}

function insertMetricPointsSql(points: GeneratedMetricPoint[]): string {
  const values = points
    .map(
      (p) =>
        `('${escapeSqlString(p.metric_series_id)}', ${p.time_unix_nano}, ${p.start_time_unix_nano}, ${p.value_double})`
    )
    .join(", ");
  return `INSERT INTO metric_points VALUES ${values}`;
}

function insertChangeEventsSql(events: GeneratedChangeEvent[]): string {
  const values = events
    .map(
      (e) =>
        `('${escapeSqlString(e.change_event_id)}', '${escapeSqlString(e.event_type)}', ` +
        `'${escapeSqlString(e.service_name)}', '${escapeSqlString(e.environment)}', ` +
        `'${escapeSqlString(e.title)}', '${escapeSqlString(e.description)}', ` +
        `${e.occurred_at_unix_nano}, '${escapeSqlString(e.source)}')`
    )
    .join(", ");
  return `INSERT INTO change_events VALUES ${values}`;
}

function insertLogsSql(logs: GeneratedLog[]): string {
  const values = logs
    .map(
      (l) =>
        `('${escapeSqlString(l.log_id)}', ${l.timestamp_unix_nano}, ${l.observed_timestamp_unix_nano}, ` +
        `${l.severity_number}, '${escapeSqlString(l.severity_text)}', '${escapeSqlString(l.body)}', ` +
        `'${escapeSqlString(l.trace_id)}', '${escapeSqlString(l.span_id)}', '${escapeSqlString(l.service_name)}', ` +
        `'${escapeSqlString(l.environment)}', '${escapeSqlString(l.host_id)}')`
    )
    .join(", ");
  return `INSERT INTO logs VALUES ${values}`;
}

async function seedData(state: EngineState, seed: number): Promise<void> {
  // Same nowUnixNano for both calls: generate_logs derives its records from
  // generate_spans internally (see generator.rs), so spans and their
  // correlated logs must share one "now" reference to stay start-time
  // consistent.
  const nowUnixNano = String(BigInt(Date.now()) * 1_000_000n);
  const spansJson = state.generateSpansJson(seed, nowUnixNano);
  const spans: GeneratedSpan[] = JSON.parse(spansJson);
  if (spans.length > 0) {
    await state.conn.query(insertSpansSql(spans));
  }

  const logsJson = state.generateLogsJson(seed, nowUnixNano);
  const logs: GeneratedLog[] = JSON.parse(logsJson);
  if (logs.length > 0) {
    await state.conn.query(insertLogsSql(logs));
  }

  const metricsJson = state.generateMetricsJson(seed, nowUnixNano);
  const { series, points }: { series: GeneratedMetricSeries[]; points: GeneratedMetricPoint[] } =
    JSON.parse(metricsJson);
  if (series.length > 0) {
    await state.conn.query(insertMetricSeriesSql(series));
  }
  if (points.length > 0) {
    await state.conn.query(insertMetricPointsSql(points));
  }

  const changeEventsJson = state.generateChangeEventsJson(seed, nowUnixNano);
  const changeEvents: GeneratedChangeEvent[] = JSON.parse(changeEventsJson);
  if (changeEvents.length > 0) {
    await state.conn.query(insertChangeEventsSql(changeEvents));
  }
}

async function initEngine(): Promise<EngineState> {
  const wasmGlueUrl = `${import.meta.env.BASE_URL}playground/wasm/playground_wasm.js`;
  const wasmModule = await import(/* @vite-ignore */ wasmGlueUrl);
  await wasmModule.default();

  const duckdb = await import("@duckdb/duckdb-wasm");
  const bundle = await duckdb.selectBundle(duckdb.getJsDelivrBundles());
  const dbWorkerUrl = URL.createObjectURL(
    new Blob([`importScripts("${bundle.mainWorker}");`], { type: "text/javascript" })
  );
  const dbWorker = new Worker(dbWorkerUrl);
  const logger = new duckdb.ConsoleLogger(duckdb.LogLevel.WARNING);
  const db = new duckdb.AsyncDuckDB(logger, dbWorker);
  await db.instantiate(bundle.mainModule, bundle.pthreadWorker);
  URL.revokeObjectURL(dbWorkerUrl);

  const conn = await db.connect();
  await conn.query(
    "CREATE TABLE spans (" +
      "trace_id VARCHAR, span_id VARCHAR, parent_span_id VARCHAR, " +
      "service_name VARCHAR, operation_name VARCHAR, duration_ns BIGINT, " +
      "status_code VARCHAR, environment VARCHAR, start_time_unix_nano BIGINT)"
  );
  await conn.query(
    "CREATE TABLE logs (" +
      "log_id VARCHAR, timestamp_unix_nano BIGINT, observed_timestamp_unix_nano BIGINT, " +
      "severity_number INTEGER, severity_text VARCHAR, body VARCHAR, " +
      "trace_id VARCHAR, span_id VARCHAR, service_name VARCHAR, " +
      "environment VARCHAR, host_id VARCHAR)"
  );
  await conn.query(
    "CREATE TABLE metric_series (" +
      "metric_series_id VARCHAR, metric_name VARCHAR, description VARCHAR, unit VARCHAR, " +
      "metric_type VARCHAR, is_monotonic BOOLEAN, aggregation_temporality VARCHAR, " +
      "service_name VARCHAR, environment VARCHAR)"
  );
  await conn.query(
    "CREATE TABLE metric_points (" +
      "metric_series_id VARCHAR, time_unix_nano BIGINT, start_time_unix_nano BIGINT, value_double DOUBLE)"
  );
  await conn.query(
    "CREATE TABLE change_events (" +
      "change_event_id VARCHAR, event_type VARCHAR, service_name VARCHAR, environment VARCHAR, " +
      "title VARCHAR, description VARCHAR, occurred_at_unix_nano BIGINT, source VARCHAR)"
  );

  const state: EngineState = {
    generateSpansJson: wasmModule.generate_spans_json,
    generateLogsJson: wasmModule.generate_logs_json,
    renderTraceSearchSql: wasmModule.render_trace_search_sql,
    renderTraceHistogramSql: wasmModule.render_trace_histogram_sql,
    renderLogSearchSql: wasmModule.render_log_search_sql,
    renderLogHistogramSql: wasmModule.render_log_histogram_sql,
    renderServiceSummarySql: wasmModule.render_service_summary_sql,
    computeServiceSummaries: wasmModule.compute_service_summaries,
    renderTopologySql: wasmModule.render_topology_sql,
    computeTopologyEdges: wasmModule.compute_topology_edges,
    generateMetricsJson: wasmModule.generate_metrics_json,
    renderMetricCatalogSql: wasmModule.render_metric_catalog_sql,
    renderMetricGroupPointsSql: wasmModule.render_metric_group_points_sql,
    generateChangeEventsJson: wasmModule.generate_change_events_json,
    renderChangeEventsSql: wasmModule.render_change_events_sql,
    renderResponseTimeHistogramSql: wasmModule.render_response_time_histogram_sql,
    conn,
  };
  await seedData(state, currentSeed);
  return state;
}

function getEngine(): Promise<EngineState> {
  if (!statePromise) statePromise = initEngine();
  return statePromise;
}

async function executeTraceTable(ir: unknown): Promise<{ rows: NlqTraceRow[]; sql: string }> {
  const { renderTraceSearchSql, conn } = await getEngine();
  const sql = renderTraceSearchSql(JSON.stringify(ir));
  const result = await conn.query(sql);
  const rows: NlqTraceRow[] = result.toArray().map((row) => ({
    trace_id: String(row.trace_id),
    root_service: String(row.root_service),
    root_operation: String(row.root_operation),
    duration_ms: Number(row.duration_ms),
    status_code: String(row.status_code),
    environment: row.environment != null ? String(row.environment) : undefined,
    start_time_unix_nano: String(row.start_time_unix_nano),
  }));
  return { rows, sql };
}

async function executeTraceHistogram(
  fromNs: string,
  toNs: string,
  bucketCount: number,
  service: string | undefined
): Promise<TraceHistogramBucket[]> {
  const { renderTraceHistogramSql, conn } = await getEngine();
  const planJson = renderTraceHistogramSql(fromNs, toNs, bucketCount, service);
  const plan: { sql: string; from_ns: string; interval_ns: string } = JSON.parse(planJson);
  const result = await conn.query(plan.sql);

  const counts = new Map<number, number>();
  for (const row of result.toArray()) {
    counts.set(Number(row.bucket_idx), Number(row.cnt));
  }

  const fromNsBig = BigInt(plan.from_ns);
  const intervalNsBig = BigInt(plan.interval_ns);
  const buckets: TraceHistogramBucket[] = [];
  for (let i = 0; i < bucketCount; i++) {
    const startNs = fromNsBig + BigInt(i) * intervalNsBig;
    const endNs = startNs + intervalNsBig;
    buckets.push({
      start_ms: Number(startNs / 1_000_000n),
      end_ms: Number(endNs / 1_000_000n),
      count: counts.get(i) ?? 0,
    });
  }
  return buckets;
}

async function executeLogTable(ir: unknown): Promise<{ rows: NlqLogRow[]; sql: string }> {
  const { renderLogSearchSql, conn } = await getEngine();
  const sql = renderLogSearchSql(JSON.stringify(ir));
  const result = await conn.query(sql);
  const rows: NlqLogRow[] = result.toArray().map((row) => ({
    tenant_id: DEMO_TENANT_ID,
    log_id: String(row.log_id),
    timestamp_unix_nano: String(row.timestamp_unix_nano),
    observed_timestamp_unix_nano: String(row.observed_timestamp_unix_nano),
    severity_number: Number(row.severity_number),
    severity_text: String(row.severity_text),
    body: String(row.body),
    trace_id: String(row.trace_id),
    span_id: String(row.span_id),
    service_name: String(row.service_name),
    environment: String(row.environment),
    host_id: String(row.host_id),
    attributes: {},
    resource_attributes: {},
  }));
  return { rows, sql };
}

async function executeLogHistogram(
  fromNs: string,
  toNs: string,
  bucketCount: number,
  service: string | undefined
): Promise<LogHistogramBucket[]> {
  const { renderLogHistogramSql, conn } = await getEngine();
  const planJson = renderLogHistogramSql(fromNs, toNs, bucketCount, service);
  const plan: { sql: string; from_ns: string; interval_ns: string } = JSON.parse(planJson);
  const result = await conn.query(plan.sql);

  const counts = new Map<number, Map<number, number>>();
  for (const row of result.toArray()) {
    const idx = Number(row.bucket_idx);
    const severity = Number(row.severity_number);
    const cnt = Number(row.cnt);
    if (!counts.has(idx)) counts.set(idx, new Map());
    counts.get(idx)!.set(severity, cnt);
  }

  const fromNsBig = BigInt(plan.from_ns);
  const intervalNsBig = BigInt(plan.interval_ns);
  const buckets: LogHistogramBucket[] = [];
  for (let i = 0; i < bucketCount; i++) {
    const startNs = fromNsBig + BigInt(i) * intervalNsBig;
    const endNs = startNs + intervalNsBig;
    const bucketCounts: Record<number, number> = {};
    for (const [severity, cnt] of counts.get(i) ?? []) {
      bucketCounts[severity] = cnt;
    }
    buckets.push({
      start_ms: Number(startNs / 1_000_000n),
      end_ms: Number(endNs / 1_000_000n),
      counts: bucketCounts,
    });
  }
  return buckets;
}

async function executeServiceSummaries(
  fromNs: string,
  toNs: string,
  environment: string | undefined
): Promise<ServiceSummary[]> {
  const { renderServiceSummarySql, computeServiceSummaries, conn } = await getEngine();
  const sql = renderServiceSummarySql(fromNs, toNs, environment);
  const result = await conn.query(sql);
  const rows = result.toArray().map((row) => ({
    service_name: String(row.service_name),
    request_count: Number(row.request_count),
    error_count: Number(row.error_count),
    p95_latency_ns: Number(row.p95_latency_ns) || 0,
  }));

  const durationSecs = Number(BigInt(toNs) - BigInt(fromNs)) / 1_000_000_000;
  const summariesJson = computeServiceSummaries(JSON.stringify(rows), durationSecs);
  return JSON.parse(summariesJson);
}

async function executeTopology(
  fromNs: string,
  toNs: string,
  environment: string | undefined,
  service: string | undefined
): Promise<TopologyEdge[]> {
  const { renderTopologySql, computeTopologyEdges, conn } = await getEngine();
  const sql = renderTopologySql(fromNs, toNs, environment, service);
  const result = await conn.query(sql);
  const rows = result.toArray().map((row) => ({
    caller: String(row.caller),
    callee: String(row.callee),
    request_count: Number(row.request_count),
    error_count: Number(row.error_count),
    p95_latency_ns: Number(row.p95_latency_ns) || 0,
  }));

  const edgesJson = computeTopologyEdges(JSON.stringify(rows));
  return JSON.parse(edgesJson);
}

async function executeServiceNames(): Promise<string[]> {
  const { conn } = await getEngine();
  const result = await conn.query(
    "SELECT DISTINCT service_name FROM spans WHERE service_name != '' ORDER BY service_name"
  );
  return result.toArray().map((row) => String(row.service_name));
}

async function executeMetricCatalog(service: string | undefined): Promise<MetricCatalogEntry[]> {
  const { renderMetricCatalogSql, conn } = await getEngine();
  const sql = renderMetricCatalogSql(service);
  const result = await conn.query(sql);
  return result.toArray().map((row) => ({
    tenant_id: DEMO_TENANT_ID,
    metric_name: String(row.metric_name),
    description: String(row.description),
    unit: String(row.unit),
    metric_type: String(row.metric_type),
    is_monotonic: Boolean(row.is_monotonic),
    aggregation_temporality: String(row.aggregation_temporality),
    service_name: String(row.service_name),
    environment: String(row.environment),
    series_count: Number(row.series_count),
  }));
}

async function executeMetricGroupPoints(metric: MetricCatalogEntry): Promise<MetricPoint[]> {
  const { renderMetricGroupPointsSql, conn } = await getEngine();
  const sql = renderMetricGroupPointsSql(
    metric.metric_name,
    metric.service_name,
    metric.environment || "default",
    metric.metric_type,
    metric.unit || ""
  );
  const result = await conn.query(sql);
  return result.toArray().map((row) => ({
    tenant_id: DEMO_TENANT_ID,
    metric_series_id: String(row.metric_series_id),
    metric_name: String(row.metric_name),
    service_name: String(row.service_name),
    time_unix_nano: Number(row.time_unix_nano),
    start_time_unix_nano: row.start_time_unix_nano != null ? Number(row.start_time_unix_nano) : undefined,
    value_double: Number(row.value_double),
  }));
}

async function executeChangeEvents(
  fromNs: string,
  toNs: string,
  service: string | undefined,
  environment: string | undefined,
  eventType: string | undefined,
  limit: number
): Promise<ChangeEvent[]> {
  const { renderChangeEventsSql, conn } = await getEngine();
  const sql = renderChangeEventsSql(service, environment, eventType, fromNs, toNs, limit);
  const result = await conn.query(sql);
  return result.toArray().map((row) => {
    const occurredAtMs = Number(BigInt(String(row.occurred_at_unix_nano)) / 1_000_000n);
    const serviceName = String(row.service_name);
    const description = String(row.description);
    const source = String(row.source);
    return {
      change_event_id: String(row.change_event_id),
      tenant_id: DEMO_TENANT_ID,
      project_id: null,
      event_type: String(row.event_type),
      service_name: serviceName === "" ? null : serviceName,
      environment: String(row.environment),
      title: String(row.title),
      description: description === "" ? null : description,
      occurred_at: new Date(occurredAtMs).toISOString(),
      source: source === "" ? null : source,
      created_by: null,
      metadata: null,
    };
  });
}

async function executeResponseTimeHistogram(
  fromNs: string,
  toNs: string,
  bucketCount: number,
  serviceName: string
): Promise<ResponseTimeHistoryBucket[]> {
  const { renderResponseTimeHistogramSql, conn } = await getEngine();
  const planJson = renderResponseTimeHistogramSql(fromNs, toNs, bucketCount, serviceName);
  const plan: { sql: string; from_ns: string; interval_ns: string } = JSON.parse(planJson);
  const result = await conn.query(plan.sql);

  const counts = new Map<number, { p50Ns: number; p95Ns: number; spanCount: number }>();
  for (const row of result.toArray()) {
    counts.set(Number(row.bucket_idx), {
      p50Ns: Number(row.p50_latency_ns) || 0,
      p95Ns: Number(row.p95_latency_ns) || 0,
      spanCount: Number(row.span_count),
    });
  }

  const fromNsBig = BigInt(plan.from_ns);
  const intervalNsBig = BigInt(plan.interval_ns);
  const intervalSecs = Number(intervalNsBig) / 1_000_000_000;
  const buckets: ResponseTimeHistoryBucket[] = [];
  for (let i = 0; i < bucketCount; i++) {
    const startNs = fromNsBig + BigInt(i) * intervalNsBig;
    const endNs = startNs + intervalNsBig;
    const bucket = counts.get(i);
    buckets.push({
      start_ms: Number(startNs / 1_000_000n),
      end_ms: Number(endNs / 1_000_000n),
      p50_ms: (bucket?.p50Ns ?? 0) / 1_000_000,
      p95_ms: (bucket?.p95Ns ?? 0) / 1_000_000,
      request_rate: (bucket?.spanCount ?? 0) / intervalSecs,
    });
  }
  return buckets;
}

async function resetPlayground(seed: number): Promise<void> {
  const state = await getEngine();
  currentSeed = seed;
  await state.conn.query("DELETE FROM spans");
  await state.conn.query("DELETE FROM logs");
  await state.conn.query("DELETE FROM metric_series");
  await state.conn.query("DELETE FROM metric_points");
  await state.conn.query("DELETE FROM change_events");
  await seedData(state, seed);
}

workerSelf.onmessage = async (event) => {
  const { requestId } = event.data;
  try {
    if (event.data.type === "nlq-execute-trace-table") {
      const { rows, sql } = await executeTraceTable(event.data.ir);
      workerSelf.postMessage({ type: "nlq-result", requestId, rows, sql });
    } else if (event.data.type === "trace-histogram") {
      const { fromNs, toNs, bucketCount, service } = event.data;
      const buckets = await executeTraceHistogram(fromNs, toNs, bucketCount, service);
      workerSelf.postMessage({ type: "histogram-result", requestId, buckets });
    } else if (event.data.type === "nlq-execute-log-table") {
      const { rows, sql } = await executeLogTable(event.data.ir);
      workerSelf.postMessage({ type: "nlq-log-result", requestId, rows, sql });
    } else if (event.data.type === "log-histogram") {
      const { fromNs, toNs, bucketCount, service } = event.data;
      const buckets = await executeLogHistogram(fromNs, toNs, bucketCount, service);
      workerSelf.postMessage({ type: "log-histogram-result", requestId, buckets });
    } else if (event.data.type === "service-summaries") {
      const { fromNs, toNs, environment } = event.data;
      const items = await executeServiceSummaries(fromNs, toNs, environment);
      workerSelf.postMessage({ type: "service-summaries-result", requestId, items });
    } else if (event.data.type === "topology") {
      const { fromNs, toNs, environment, service } = event.data;
      const edges = await executeTopology(fromNs, toNs, environment, service);
      workerSelf.postMessage({ type: "topology-result", requestId, edges });
    } else if (event.data.type === "service-names") {
      const names = await executeServiceNames();
      workerSelf.postMessage({ type: "service-names-result", requestId, names });
    } else if (event.data.type === "metric-catalog") {
      const metrics = await executeMetricCatalog(event.data.service);
      workerSelf.postMessage({ type: "metric-catalog-result", requestId, metrics });
    } else if (event.data.type === "metric-group-points") {
      const points = await executeMetricGroupPoints(event.data.metric);
      workerSelf.postMessage({ type: "metric-group-points-result", requestId, points });
    } else if (event.data.type === "change-events") {
      const { fromNs, toNs, service, environment, eventType, limit } = event.data;
      const items = await executeChangeEvents(fromNs, toNs, service, environment, eventType, limit);
      workerSelf.postMessage({ type: "change-events-result", requestId, items });
    } else if (event.data.type === "response-time-histogram") {
      const { fromNs, toNs, bucketCount, serviceName } = event.data;
      const buckets = await executeResponseTimeHistogram(fromNs, toNs, bucketCount, serviceName);
      workerSelf.postMessage({ type: "response-time-histogram-result", requestId, buckets });
    } else if (event.data.type === "reset") {
      await resetPlayground(event.data.seed);
      workerSelf.postMessage({ type: "reset-done", requestId });
    }
  } catch (err) {
    workerSelf.postMessage({
      type: "nlq-error",
      requestId,
      message: err instanceof Error ? err.message : String(err),
    });
  }
};
