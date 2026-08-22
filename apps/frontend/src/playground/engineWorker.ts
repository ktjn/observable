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
  | { type: "reset"; requestId: string; seed: number };
type EngineResponse =
  | { type: "nlq-result"; requestId: string; rows: NlqTraceRow[]; sql: string }
  | { type: "histogram-result"; requestId: string; buckets: TraceHistogramBucket[] }
  | { type: "nlq-log-result"; requestId: string; rows: NlqLogRow[]; sql: string }
  | { type: "log-histogram-result"; requestId: string; buckets: LogHistogramBucket[] }
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

  const state: EngineState = {
    generateSpansJson: wasmModule.generate_spans_json,
    generateLogsJson: wasmModule.generate_logs_json,
    renderTraceSearchSql: wasmModule.render_trace_search_sql,
    renderTraceHistogramSql: wasmModule.render_trace_histogram_sql,
    renderLogSearchSql: wasmModule.render_log_search_sql,
    renderLogHistogramSql: wasmModule.render_log_histogram_sql,
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

async function resetPlayground(seed: number): Promise<void> {
  const state = await getEngine();
  currentSeed = seed;
  await state.conn.query("DELETE FROM spans");
  await state.conn.query("DELETE FROM logs");
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
