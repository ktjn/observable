// Persistent playground query engine worker. Unlike worker.ts (the Phase 0
// spike, which spawns a new worker per call and terminates it), this worker
// stays alive for the page's lifetime: DuckDB-WASM initialization and the
// seed dataset are done once (memoized), then reused across queries.
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

type EngineRequest = { type: "nlq-execute-trace-table"; requestId: string; ir: unknown };
type EngineResponse =
  | { type: "nlq-result"; requestId: string; rows: NlqTraceRow[]; sql: string }
  | { type: "nlq-error"; requestId: string; message: string };

interface WorkerSelf {
  onmessage: ((event: MessageEvent<EngineRequest>) => void) | null;
  postMessage(message: EngineResponse): void;
}
const workerSelf = self as unknown as WorkerSelf;

// Temporary fixed fixture standing in for Phase 4's deterministic telemetry
// generator (not built yet — see the plan doc's Phase 4). Two services,
// OK + ERROR statuses, one staging row, so filter-pill queries have
// something to narrow.
const SEED_SPANS = [
  {
    trace_id: "playground-trace-1",
    span_id: "span-1",
    parent_span_id: "",
    service_name: "checkout",
    operation_name: "POST /checkout",
    duration_ns: 12_000_000,
    status_code: "OK",
    environment: "production",
    // Deliberately not exactly Date.now(): the frontend snapshots its "now"
    // upper time bound before this worker (lazily initialized on first
    // query) computes its own Date.now(), so a row seeded at the exact
    // current instant can land a few ms *after* that bound and get silently
    // filtered out by start_time_unix_nano <= to_expr. Comfortable margin
    // avoids that race. Temporary fixed fixture — Phase 4 replaces this with
    // a real generator seeded relative to page-load time.
    start_time_unix_nano: Date.now() * 1_000_000 - 5_000_000_000,
  },
  {
    trace_id: "playground-trace-2",
    span_id: "span-2",
    parent_span_id: "",
    service_name: "payment",
    operation_name: "POST /charge",
    duration_ns: 340_000_000,
    status_code: "ERROR",
    environment: "production",
    start_time_unix_nano: Date.now() * 1_000_000 - 30_000_000_000,
  },
  {
    trace_id: "playground-trace-3",
    span_id: "span-3",
    parent_span_id: "",
    service_name: "checkout",
    operation_name: "GET /cart",
    duration_ns: 8_000_000,
    status_code: "OK",
    environment: "staging",
    start_time_unix_nano: Date.now() * 1_000_000 - 60_000_000_000,
  },
];

interface EngineState {
  renderTraceSearchSql: (irJson: string) => string;
  conn: { query: (sql: string) => Promise<{ toArray: () => Record<string, unknown>[] }> };
}

let statePromise: Promise<EngineState> | null = null;

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
  for (const span of SEED_SPANS) {
    await conn.query(
      `INSERT INTO spans VALUES ('${span.trace_id}', '${span.span_id}', '${span.parent_span_id}', ` +
        `'${span.service_name}', '${span.operation_name}', ${span.duration_ns}, ` +
        `'${span.status_code}', '${span.environment}', ${span.start_time_unix_nano})`
    );
  }

  return { renderTraceSearchSql: wasmModule.render_trace_search_sql, conn };
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

workerSelf.onmessage = async (event) => {
  if (event.data.type !== "nlq-execute-trace-table") return;
  const { requestId } = event.data;
  try {
    const { rows, sql } = await executeTraceTable(event.data.ir);
    workerSelf.postMessage({ type: "nlq-result", requestId, rows, sql });
  } catch (err) {
    workerSelf.postMessage({
      type: "nlq-error",
      requestId,
      message: err instanceof Error ? err.message : String(err),
    });
  }
};
