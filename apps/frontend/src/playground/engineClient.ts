export interface NlqTraceRow {
  trace_id: string;
  root_service: string;
  root_operation: string;
  duration_ms: number;
  status_code: string;
  environment?: string;
  start_time_unix_nano: number | string;
}

export interface TraceDetailSpan {
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

export interface MetricCatalogEntry {
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

export interface MetricPoint {
  tenant_id: string;
  metric_series_id: string;
  metric_name: string;
  service_name: string;
  time_unix_nano: number;
  start_time_unix_nano?: number;
  value_double?: number;
}

export interface ChangeEvent {
  change_event_id: string;
  tenant_id: string;
  project_id: string | null;
  event_type: "config_change" | "feature_flag" | "migration" | "incident" | "other";
  service_name: string | null;
  environment: string;
  title: string;
  description: string | null;
  occurred_at: string;
  source: string | null;
  created_by: string | null;
  metadata: Record<string, unknown> | null;
}

export interface ResponseTimeHistoryBucket {
  start_ms: number;
  end_ms: number;
  p50_ms: number;
  p95_ms: number;
  request_rate: number;
}

type EngineResult =
  | { rows: NlqTraceRow[]; sql: string }
  | { spans: TraceDetailSpan[] }
  | { buckets: TraceHistogramBucket[] }
  | { rows: NlqLogRow[]; sql: string }
  | { buckets: LogHistogramBucket[] }
  | { items: ServiceSummary[] }
  | { edges: TopologyEdge[] }
  | { names: string[] }
  | { metrics: MetricCatalogEntry[] }
  | { points: MetricPoint[] }
  | { changeEvents: ChangeEvent[] }
  | { buckets: ResponseTimeHistoryBucket[] }
  | undefined;

type EngineResponse =
  | { type: "nlq-result"; requestId: string; rows: NlqTraceRow[]; sql: string }
  | { type: "trace-detail-result"; requestId: string; spans: TraceDetailSpan[] }
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
    } else if (event.data.type === "trace-detail-result") {
      request.resolve({ spans: event.data.spans });
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
    } else if (event.data.type === "metric-catalog-result") {
      request.resolve({ metrics: event.data.metrics });
    } else if (event.data.type === "metric-group-points-result") {
      request.resolve({ points: event.data.points });
    } else if (event.data.type === "change-events-result") {
      request.resolve({ changeEvents: event.data.items });
    } else if (event.data.type === "response-time-histogram-result") {
      request.resolve({ buckets: event.data.buckets });
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

/**
 * Fetches every span of one trace (the trace-detail waterfall) from the
 * playground's local `spans` table through the persistent engine worker.
 * Resolves with an empty array when the trace id is unknown.
 */
export function executeTraceDetail(traceId: string): Promise<{ spans: TraceDetailSpan[] }> {
  const requestId = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    pending.set(requestId, {
      resolve: resolve as (r: EngineResult) => void,
      reject,
    });
    getWorker().postMessage({ type: "trace-detail", requestId, traceId });
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

/**
 * Lists the Metrics page's catalog (Rust-planned DuckDB aggregation over
 * the browser-local `metric_series` table) through the persistent
 * playground engine worker.
 */
export function executeMetricCatalog(service: string | undefined): Promise<{ metrics: MetricCatalogEntry[] }> {
  const requestId = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    pending.set(requestId, {
      resolve: resolve as (r: EngineResult) => void,
      reject,
    });
    getWorker().postMessage({ type: "metric-catalog", requestId, service });
  });
}

/**
 * Runs the Metrics page's group-points query (Rust-planned DuckDB join
 * over the browser-local `metric_points`/`metric_series` tables) through
 * the persistent playground engine worker.
 */
export function executeMetricGroupPoints(metric: MetricCatalogEntry): Promise<{ points: MetricPoint[] }> {
  const requestId = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    pending.set(requestId, {
      resolve: resolve as (r: EngineResult) => void,
      reject,
    });
    getWorker().postMessage({ type: "metric-group-points", requestId, metric });
  });
}

/**
 * Runs the Change Events page's filtered/limited listing (Rust-planned
 * DuckDB query over the browser-local `change_events` table) through the
 * persistent playground engine worker.
 */
export function executeChangeEvents(params: {
  fromNs: string;
  toNs: string;
  service?: string;
  environment?: string;
  eventType?: string;
  limit: number;
}): Promise<{ changeEvents: ChangeEvent[] }> {
  const requestId = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    pending.set(requestId, {
      resolve: resolve as (r: EngineResult) => void,
      reject,
    });
    getWorker().postMessage({ type: "change-events", requestId, ...params });
  });
}

/**
 * Runs the Service Detail page's response-time histogram (P50/P95/
 * throughput graph) through the persistent playground engine worker.
 */
export function executeResponseTimeHistogram(params: {
  fromNs: string;
  toNs: string;
  bucketCount: number;
  serviceName: string;
}): Promise<{ buckets: ResponseTimeHistoryBucket[] }> {
  const requestId = String(nextRequestId++);
  return new Promise((resolve, reject) => {
    pending.set(requestId, {
      resolve: resolve as (r: EngineResult) => void,
      reject,
    });
    getWorker().postMessage({ type: "response-time-histogram", requestId, ...params });
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
