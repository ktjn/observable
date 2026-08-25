// Persistent playground query engine worker. Unlike worker.ts (the Phase 0
// spike, which spawns a new worker per call and terminates it), this worker
// stays alive for the page's lifetime: DuckDB-WASM initialization is done
// once (memoized), then reused across queries and resets.
import {
  DuckDbStorageWriter,
  type ProcessedLog,
  type ProcessedMetricPoint,
  type ProcessedMetricSeries,
  type ProcessedSpan,
} from "./duckdbStorageWriter";
import { DuckDbTraceQueryApi } from "./duckdbTraceQueryApi";
import { DuckDbLogQueryApi } from "./duckdbLogQueryApi";
import { DuckDbMetricsQueryApi } from "./duckdbMetricsQueryApi";
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

/**
 * Full frontend Span shape assembled at read time from the playground's
 * narrow local `spans` table (which only persists the columns any wired
 * query actually plans against) plus fixed demo defaults.
 */
interface TraceDetailSpan {
  span_id: string;
  trace_id: string;
  parent_span_id?: string;
  tenant_id: string;
  service_name: string;
  service_namespace: string;
  service_version: string;
  operation_name: string;
  span_kind: "INTERNAL" | "SERVER" | "CLIENT" | "PRODUCER" | "CONSUMER";
  start_time_unix_nano: number;
  end_time_unix_nano: number;
  duration_ns: number;
  status_code: "UNSET" | "OK" | "ERROR";
  status_message: string;
  attributes: Record<string, unknown>;
  resource_attributes: Record<string, unknown>;
  environment: string;
  host_id: string;
  workload: string;
  deployment_id: string;
}

type EngineRequest =
  | { type: "nlq-execute-trace-table"; requestId: string; ir: unknown }
  | { type: "trace-detail"; requestId: string; traceId: string }
  | {
      type: "logs-search";
      requestId: string;
      traceId?: string;
      service?: string;
      limit: number;
    }
  | { type: "logs-context"; requestId: string; logId: string; window: number }
  | {
      type: "logs-tail";
      requestId: string;
      service?: string;
      severity?: number;
      sinceUnixNano?: string;
      limit: number;
    }
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
  | { type: "trace-detail-result"; requestId: string; spans: TraceDetailSpan[] }
  | { type: "logs-result"; requestId: string; logs: NlqLogRow[] }
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
  processGeneratedSpansJson: (json: string, tenantId: string) => string;
  generateLogsJson: (seed: number, nowUnixNano: string) => string;
  processGeneratedLogsJson: (json: string, tenantId: string) => string;
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
  processGeneratedMetricsJson: (json: string, tenantId: string) => string;
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

async function seedData(state: EngineState, seed: number): Promise<void> {
  // Same nowUnixNano for both calls: generate_logs derives its records from
  // generate_spans internally (see generator.rs), so spans and their
  // correlated logs must share one "now" reference to stay start-time
  // consistent.
  const nowUnixNano = String(BigInt(Date.now()) * 1_000_000n);
  const spansJson = state.generateSpansJson(seed, nowUnixNano);
  const processedSpansJson = state.processGeneratedSpansJson(spansJson, DEMO_TENANT_ID);
  const spans: ProcessedSpan[] = JSON.parse(processedSpansJson);
  const logsJson = state.generateLogsJson(seed, nowUnixNano);
  const processedLogsJson = state.processGeneratedLogsJson(logsJson, DEMO_TENANT_ID);
  const logs: ProcessedLog[] = JSON.parse(processedLogsJson);

  const metricsJson = state.generateMetricsJson(seed, nowUnixNano);
  const processedMetricsJson = state.processGeneratedMetricsJson(metricsJson, DEMO_TENANT_ID);
  const [series, points]: [ProcessedMetricSeries[], ProcessedMetricPoint[]] =
    JSON.parse(processedMetricsJson);
  const storageWriter = new DuckDbStorageWriter(state.conn);
  await storageWriter.write({ spans, logs, series, points });

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
      "tenant_id VARCHAR, trace_id VARCHAR, span_id VARCHAR, parent_span_id VARCHAR, " +
      "service_name VARCHAR, operation_name VARCHAR, duration_ns BIGINT, " +
      "status_code VARCHAR, environment VARCHAR, start_time_unix_nano BIGINT)"
  );
  await conn.query(
    "CREATE TABLE logs (" +
      "tenant_id VARCHAR, log_id VARCHAR, timestamp_unix_nano BIGINT, observed_timestamp_unix_nano BIGINT, " +
      "severity_number INTEGER, severity_text VARCHAR, body VARCHAR, " +
      "trace_id VARCHAR, span_id VARCHAR, service_name VARCHAR, " +
      "environment VARCHAR, host_id VARCHAR)"
  );
  await conn.query(
    "CREATE TABLE metric_series (" +
      "tenant_id VARCHAR, metric_series_id VARCHAR, metric_name VARCHAR, description VARCHAR, unit VARCHAR, " +
      "metric_type VARCHAR, is_monotonic BOOLEAN, aggregation_temporality VARCHAR, " +
      "service_name VARCHAR, environment VARCHAR)"
  );
  await conn.query(
    "CREATE TABLE metric_points (" +
      "tenant_id VARCHAR, metric_series_id VARCHAR, time_unix_nano BIGINT, start_time_unix_nano BIGINT, value_double DOUBLE)"
  );
  await conn.query(
    "CREATE TABLE change_events (" +
      "change_event_id VARCHAR, event_type VARCHAR, service_name VARCHAR, environment VARCHAR, " +
      "title VARCHAR, description VARCHAR, occurred_at_unix_nano BIGINT, source VARCHAR)"
  );

  const state: EngineState = {
    generateSpansJson: wasmModule.generate_spans_json,
    processGeneratedSpansJson: wasmModule.process_generated_spans_json,
    generateLogsJson: wasmModule.generate_logs_json,
    processGeneratedLogsJson: wasmModule.process_generated_logs_json,
    renderTraceSearchSql: wasmModule.render_trace_search_sql,
    renderTraceHistogramSql: wasmModule.render_trace_histogram_sql,
    renderLogSearchSql: wasmModule.render_log_search_sql,
    renderLogHistogramSql: wasmModule.render_log_histogram_sql,
    renderServiceSummarySql: wasmModule.render_service_summary_sql,
    computeServiceSummaries: wasmModule.compute_service_summaries,
    renderTopologySql: wasmModule.render_topology_sql,
    computeTopologyEdges: wasmModule.compute_topology_edges,
    generateMetricsJson: wasmModule.generate_metrics_json,
    processGeneratedMetricsJson: wasmModule.process_generated_metrics_json,
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

/**
 * Fetches every span of one trace (trace-detail page's waterfall shape)
 * from the playground's local `spans` table and assembles full frontend
 * Span objects at read time. Root spans (empty parent_span_id) are
 * reported as SERVER kind; end times are derived from start + duration.
 */
async function executeTraceDetail(traceId: string): Promise<TraceDetailSpan[]> {
  const { conn } = await getEngine();
  const queryApi = new DuckDbTraceQueryApi(conn);
  const rows = await queryApi.findTrace(traceId);
  return rows.map((row) => {
    const startNs = Number(row.start_time_unix_nano);
    const durationNs = Number(row.duration_ns);
    const parentSpanId = String(row.parent_span_id);
    return {
      span_id: String(row.span_id),
      trace_id: String(row.trace_id),
      parent_span_id: parentSpanId === "" ? undefined : parentSpanId,
      tenant_id: DEMO_TENANT_ID,
      service_name: String(row.service_name),
      service_namespace: "observable-playground",
      service_version: "",
      operation_name: String(row.operation_name),
      span_kind: (parentSpanId === "" ? "SERVER" : "INTERNAL") as TraceDetailSpan["span_kind"],
      start_time_unix_nano: startNs,
      end_time_unix_nano: startNs + durationNs,
      duration_ns: durationNs,
      status_code: String(row.status_code) as TraceDetailSpan["status_code"],
      status_message: "",
      attributes: {},
      resource_attributes: {},
      environment: String(row.environment),
      host_id: "",
      workload: "",
      deployment_id: "",
    };
  });
}

async function executeTraceTable(ir: unknown): Promise<{ rows: NlqTraceRow[]; sql: string }> {
  const { renderTraceSearchSql, conn } = await getEngine();
  const sql = renderTraceSearchSql(JSON.stringify(ir));
  const queryApi = new DuckDbTraceQueryApi(conn);
  const rawRows = await queryApi.executePlanned(sql);
  const rows: NlqTraceRow[] = rawRows.map((row) => ({
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

function mapLogRows(raw: Record<string, unknown>[]): NlqLogRow[] {
  return raw.map((row) => ({
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
}

const LOG_COLUMNS =
  "log_id, timestamp_unix_nano, observed_timestamp_unix_nano, severity_number, severity_text, body, trace_id, span_id, service_name, environment, host_id";

async function executeLogTable(ir: unknown): Promise<{ rows: NlqLogRow[]; sql: string }> {
  const { renderLogSearchSql, conn } = await getEngine();
  const sql = renderLogSearchSql(JSON.stringify(ir));
  const queryApi = new DuckDbLogQueryApi(conn);
  return { rows: mapLogRows(await queryApi.executePlanned(sql)), sql };
}

/**
 * Log search (correlated-logs panel / log list shapes) over the playground's
 * local `logs` table. Filters are optional and ANDed; results are newest-first.
 */
async function executeLogsSearch(
  traceId: string | undefined,
  service: string | undefined,
  limit: number
): Promise<NlqLogRow[]> {
  const { conn } = await getEngine();
  const queryApi = new DuckDbLogQueryApi(conn);
  return mapLogRows(await queryApi.search(traceId, service, limit));
}

/**
 * Surrounding-log context for one log record: the pivot record plus the
 * `window` records immediately before and after it in timestamp order.
 */
async function executeLogsContext(logId: string, window: number): Promise<NlqLogRow[]> {
  const { conn } = await getEngine();
  const pivotResult = await conn.query(
    `SELECT timestamp_unix_nano FROM logs WHERE log_id = '${escapeSqlString(logId)}'`
  );
  const pivot = pivotResult.toArray();
  if (pivot.length === 0) return [];
  const pivotTs = BigInt(String(pivot[0].timestamp_unix_nano));
  const half = Math.max(1, Math.floor(window));
  // Nearest records around the pivot by timestamp ordering (the pivot's
  // own timestamp is inclusive on the "before" side).
  const before = await conn.query(
    `SELECT ${LOG_COLUMNS} FROM logs WHERE timestamp_unix_nano <= ${pivotTs} ORDER BY timestamp_unix_nano DESC LIMIT ${half}`
  );
  const after = await conn.query(
    `SELECT ${LOG_COLUMNS} FROM logs WHERE timestamp_unix_nano > ${pivotTs} ORDER BY timestamp_unix_nano ASC LIMIT ${half}`
  );
  return mapLogRows([...before.toArray(), ...after.toArray()]);
}

/**
 * Live-tail poll: records newer than the caller's cursor (`sinceUnixNano`),
 * oldest-first so callers can append in order.
 */
async function executeLogsTail(
  service: string | undefined,
  severity: number | undefined,
  sinceUnixNano: string | undefined,
  limit: number
): Promise<NlqLogRow[]> {
  const { conn } = await getEngine();
  const conditions: string[] = [];
  if (sinceUnixNano && /^\d+$/.test(sinceUnixNano.trim())) {
    conditions.push(`timestamp_unix_nano > ${BigInt(sinceUnixNano.trim())}`);
  }
  if (service) conditions.push(`service_name = '${escapeSqlString(service)}'`);
  if (severity !== undefined) conditions.push(`severity_number >= ${Math.floor(severity)}`);
  const where = conditions.length > 0 ? ` WHERE ${conditions.join(" AND ")}` : "";
  const result = await conn.query(
    `SELECT ${LOG_COLUMNS} FROM logs${where} ORDER BY timestamp_unix_nano ASC LIMIT ${Math.max(1, Math.floor(limit))}`
  );
  return mapLogRows(result.toArray());
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
  const queryApi = new DuckDbMetricsQueryApi(conn);
  const rows = await queryApi.executePlanned(sql);
  return rows.map((row) => ({
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
  const queryApi = new DuckDbMetricsQueryApi(conn);
  const rows = await queryApi.executePlanned(sql);
  return rows.map((row) => ({
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
    } else if (event.data.type === "trace-detail") {
      const spans = await executeTraceDetail(event.data.traceId);
      workerSelf.postMessage({ type: "trace-detail-result", requestId, spans });
    } else if (event.data.type === "logs-search") {
      const { traceId, service, limit } = event.data;
      const logs = await executeLogsSearch(traceId, service, limit);
      workerSelf.postMessage({ type: "logs-result", requestId, logs });
    } else if (event.data.type === "logs-context") {
      const { logId, window: windowParam } = event.data;
      const logs = await executeLogsContext(logId, windowParam);
      workerSelf.postMessage({ type: "logs-result", requestId, logs });
    } else if (event.data.type === "logs-tail") {
      const { service, severity, sinceUnixNano, limit } = event.data;
      const logs = await executeLogsTail(service, severity, sinceUnixNano, limit);
      workerSelf.postMessage({ type: "logs-result", requestId, logs });
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
