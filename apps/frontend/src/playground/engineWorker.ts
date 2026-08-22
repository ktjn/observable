// Persistent playground query engine worker. Unlike worker.ts (the Phase 0
// spike, which spawns a new worker per call and terminates it), this worker
// stays alive for the page's lifetime: DuckDB-WASM initialization is done
// once (memoized), then reused across queries and resets.
export {};

interface NlqTraceRow {
  trace_id: string;
  root_service: string;
  root_operation: string;
  duration_ms: number;
  status_code: string;
  environment?: string;
  start_time_unix_nano: number | string;
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

type EngineRequest =
  | { type: "nlq-execute-trace-table"; requestId: string; ir: unknown }
  | { type: "reset"; requestId: string; seed: number };
type EngineResponse =
  | { type: "nlq-result"; requestId: string; rows: NlqTraceRow[]; sql: string }
  | { type: "reset-done"; requestId: string }
  | { type: "nlq-error"; requestId: string; message: string };

interface WorkerSelf {
  onmessage: ((event: MessageEvent<EngineRequest>) => void) | null;
  postMessage(message: EngineResponse): void;
}
const workerSelf = self as unknown as WorkerSelf;

interface EngineState {
  generateSpansJson: (seed: number, nowUnixNano: string) => string;
  renderTraceSearchSql: (irJson: string) => string;
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

async function seedSpans(state: EngineState, seed: number): Promise<void> {
  const nowUnixNano = String(BigInt(Date.now()) * 1_000_000n);
  const spansJson = state.generateSpansJson(seed, nowUnixNano);
  const spans: GeneratedSpan[] = JSON.parse(spansJson);
  if (spans.length > 0) {
    await state.conn.query(insertSpansSql(spans));
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

  const state: EngineState = {
    generateSpansJson: wasmModule.generate_spans_json,
    renderTraceSearchSql: wasmModule.render_trace_search_sql,
    conn,
  };
  await seedSpans(state, currentSeed);
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

async function resetPlayground(seed: number): Promise<void> {
  const state = await getEngine();
  currentSeed = seed;
  await state.conn.query("DELETE FROM spans");
  await seedSpans(state, seed);
}

workerSelf.onmessage = async (event) => {
  const { requestId } = event.data;
  try {
    if (event.data.type === "nlq-execute-trace-table") {
      const { rows, sql } = await executeTraceTable(event.data.ir);
      workerSelf.postMessage({ type: "nlq-result", requestId, rows, sql });
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
