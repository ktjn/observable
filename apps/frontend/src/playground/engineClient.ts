export interface NlqTraceRow {
  trace_id: string;
  root_service: string;
  root_operation: string;
  duration_ms: number;
  status_code: string;
  environment?: string;
  start_time_unix_nano: number | string;
}

export interface TraceHistogramBucket {
  start_ms: number;
  end_ms: number;
  count: number;
}

export interface NlqLogRow {
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

export interface LogHistogramBucket {
  start_ms: number;
  end_ms: number;
  counts: Record<string, number>;
}

export interface ServiceSummary {
  service_name: string;
  request_rate: number;
  error_rate: number;
  p95_latency_ms: number;
  health_state: "healthy" | "watch" | "breach";
  active_alert_count: number;
  latest_deployment: string | null;
}

export interface TopologyEdge {
  caller: string;
  callee: string;
  request_count: number;
  error_rate: number;
  p95_latency_ms: number;
}

type EngineResult =
  | { rows: NlqTraceRow[]; sql: string }
  | { buckets: TraceHistogramBucket[] }
  | { rows: NlqLogRow[]; sql: string }
  | { buckets: LogHistogramBucket[] }
  | { items: ServiceSummary[] }
  | { edges: TopologyEdge[] }
  | { names: string[] }
  | undefined;

type EngineResponse =
  | { type: "nlq-result"; requestId: string; rows: NlqTraceRow[]; sql: string }
  | { type: "histogram-result"; requestId: string; buckets: TraceHistogramBucket[] }
  | { type: "nlq-log-result"; requestId: string; rows: NlqLogRow[]; sql: string }
  | { type: "log-histogram-result"; requestId: string; buckets: LogHistogramBucket[] }
  | { type: "service-summaries-result"; requestId: string; items: ServiceSummary[] }
  | { type: "topology-result"; requestId: string; edges: TopologyEdge[] }
  | { type: "service-names-result"; requestId: string; names: string[] }
  | { type: "reset-done"; requestId: string }
  | { type: "nlq-error"; requestId: string; message: string };

interface PendingRequest {
  resolve: (result: EngineResult) => void;
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
    } else if (event.data.type === "nlq-log-result") {
      request.resolve({ rows: event.data.rows, sql: event.data.sql });
    } else if (event.data.type === "histogram-result") {
      request.resolve({ buckets: event.data.buckets });
    } else if (event.data.type === "log-histogram-result") {
      request.resolve({ buckets: event.data.buckets });
    } else if (event.data.type === "service-summaries-result") {
      request.resolve({ items: event.data.items });
    } else if (event.data.type === "topology-result") {
      request.resolve({ edges: event.data.edges });
    } else if (event.data.type === "service-names-result") {
      request.resolve({ names: event.data.names });
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
      resolve: resolve as (r: EngineResult) => void,
      reject,
    });
    getWorker().postMessage({ type: "nlq-execute-trace-table", requestId, ir });
  });
}

/** Runs a Traces histogram query through the persistent playground engine worker. */
export function executeTraceHistogram(params: {
  fromNs: string;
  toNs: string;
  bucketCount: number;
  service?: string;
}): Promise<{ buckets: TraceHistogramBucket[] }> {
  const requestId = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    pending.set(requestId, {
      resolve: resolve as (r: EngineResult) => void,
      reject,
    });
    getWorker().postMessage({ type: "trace-histogram", requestId, ...params });
  });
}

/**
 * Runs a Logs table query (page-load / filter-pill shape) through the
 * persistent playground engine worker: Rust-planned DuckDB SQL executed
 * against the browser-local `logs` table.
 */
export function executeLogTable(ir: unknown): Promise<{ rows: NlqLogRow[]; sql: string }> {
  const requestId = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    pending.set(requestId, {
      resolve: resolve as (r: EngineResult) => void,
      reject,
    });
    getWorker().postMessage({ type: "nlq-execute-log-table", requestId, ir });
  });
}

/** Runs a Logs histogram query through the persistent playground engine worker. */
export function executeLogHistogram(params: {
  fromNs: string;
  toNs: string;
  bucketCount: number;
  service?: string;
}): Promise<{ buckets: LogHistogramBucket[] }> {
  const requestId = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    pending.set(requestId, {
      resolve: resolve as (r: EngineResult) => void,
      reject,
    });
    getWorker().postMessage({ type: "log-histogram", requestId, ...params });
  });
}

/**
 * Runs the Services list page's per-service summary aggregation through the
 * persistent playground engine worker: Rust-planned DuckDB SQL executed
 * against the browser-local `spans` table, then shaped (rates, health
 * state) by the same pure Rust logic production's discovery.rs uses.
 */
export function executeServiceSummaries(params: {
  fromNs: string;
  toNs: string;
  environment?: string;
}): Promise<{ items: ServiceSummary[] }> {
  const requestId = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    pending.set(requestId, {
      resolve: resolve as (r: EngineResult) => void,
      reject,
    });
    getWorker().postMessage({ type: "service-summaries", requestId, ...params });
  });
}

/**
 * Runs the Services topology view's parent/child join aggregation through
 * the persistent playground engine worker: Rust-planned DuckDB SQL executed
 * against the browser-local `spans` table.
 */
export function executeTopology(params: {
  fromNs: string;
  toNs: string;
  environment?: string;
  service?: string;
}): Promise<{ edges: TopologyEdge[] }> {
  const requestId = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    pending.set(requestId, {
      resolve: resolve as (r: EngineResult) => void,
      reject,
    });
    getWorker().postMessage({ type: "topology", requestId, ...params });
  });
}

/** Lists distinct service names observed in the playground's local `spans` table. */
export function executeServiceNames(): Promise<{ names: string[] }> {
  const requestId = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    pending.set(requestId, {
      resolve: resolve as (r: EngineResult) => void,
      reject,
    });
    getWorker().postMessage({ type: "service-names", requestId });
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
