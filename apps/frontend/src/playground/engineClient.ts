export interface NlqTraceRow {
  trace_id: string;
  root_service: string;
  root_operation: string;
  duration_ms: number;
  status_code: string;
  environment?: string;
  start_time_unix_nano: number | string;
}

type EngineResponse =
  | { type: "nlq-result"; requestId: string; rows: NlqTraceRow[]; sql: string }
  | { type: "reset-done"; requestId: string }
  | { type: "nlq-error"; requestId: string; message: string };

interface PendingRequest {
  resolve: (result: { rows: NlqTraceRow[]; sql: string } | undefined) => void;
  reject: (error: Error) => void;
}

let worker: Worker | null = null;
let nextRequestId = 0;
const pending = new Map<string, PendingRequest>();

function getWorker(): Worker {
  if (worker) return worker;
  worker = new Worker(new URL("./engineWorker.ts", import.meta.url), { type: "module" });
  worker.onmessage = (event: MessageEvent<EngineResponse>) => {
    const request = pending.get(event.data.requestId);
    if (!request) return;
    pending.delete(event.data.requestId);
    if (event.data.type === "nlq-result") {
      request.resolve({ rows: event.data.rows, sql: event.data.sql });
    } else if (event.data.type === "reset-done") {
      request.resolve(undefined);
    } else {
      request.reject(new Error(event.data.message));
    }
  };
  return worker;
}

/**
 * Runs a Traces table query (page-load / filter-pill shape) through the
 * persistent playground engine worker: Rust-planned DuckDB SQL executed
 * against the browser-local `spans` table. `ir` is a plain-object NlqIr —
 * typed loosely here to avoid a runtime<->worker type-import cycle.
 */
export function executeTraceTable(ir: unknown): Promise<{ rows: NlqTraceRow[]; sql: string }> {
  const requestId = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    pending.set(requestId, {
      resolve: resolve as (r: { rows: NlqTraceRow[]; sql: string } | undefined) => void,
      reject,
    });
    getWorker().postMessage({ type: "nlq-execute-trace-table", requestId, ir });
  });
}

/** Regenerates the playground's demo dataset with a fresh seed. */
export function resetPlayground(): Promise<void> {
  const requestId = String(nextRequestId++);
  const seed = Date.now() % 0xffffffff;
  return new Promise((resolve, reject) => {
    pending.set(requestId, { resolve: () => resolve(), reject });
    getWorker().postMessage({ type: "reset", requestId, seed });
  });
}
