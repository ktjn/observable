import type { Span, TraceHistogramResponse, TraceListResponse, TraceResponse } from "../api/traces";
import type { LogRecord, LogHistogramResponse } from "../api/logs";
import type { TenantListResponse, EnvironmentListResponse } from "../api/tenants";
import type { NlqRequest, NlqResponse, NlqIr, VisualizationFrame } from "../api/nlq";
import type { Dashboard } from "../api/dashboards";
import type { RuntimeApi, TraceHistogramParams, LogHistogramParams } from "./types";
// Dynamically imported (not a static import): engineClient statically
// references a Worker URL, which Vite's dep scanner eagerly resolves at
// import time. A static import here would drag that into every module that
// imports playgroundRuntime.ts, including production (non-playground)
// tests — see the identical issue with PlaygroundSpike in router.ts.

/**
 * In-memory stub for most operations; `nlq.execute` is wired to the real
 * playground engine (Rust-planned DuckDB-WASM query) for one shape — the
 * Traces page's page-load/filter-pill query (no free-text question,
 * signals=["traces"], operation="table"). Everything else still falls back
 * to STUB_NLQ_FRAME. See Phase 3 in
 * docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md.
 */

// Must match useTenantContext.tsx's DEFAULT_TENANT_ID / DEFAULT_TENANT_NAME and
// useAuth.ts's PLAYGROUND_USER tenant id — the seeded "observable" tenant.
const DEMO_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const DEMO_TENANT_NAME = "observable";

const STUB_SPAN: Span = {
  span_id: "span-1",
  trace_id: "playground-trace-1",
  tenant_id: DEMO_TENANT_ID,
  service_name: "checkout",
  service_namespace: "observable-playground",
  service_version: "0.0.0",
  operation_name: "POST /checkout",
  span_kind: "SERVER",
  start_time_unix_nano: 0,
  end_time_unix_nano: 12_000_000,
  duration_ns: 12_000_000,
  status_code: "OK",
  status_message: "",
  attributes: {},
  resource_attributes: {},
  environment: "production",
  host_id: "playground-host",
  workload: "checkout",
  deployment_id: "playground-deployment",
};

const STUB_TRACE: TraceResponse = {
  trace_id: STUB_SPAN.trace_id,
  spans: [STUB_SPAN],
  events: [],
};

interface NlqTraceRow {
  trace_id: string;
  root_service: string;
  root_operation: string;
  duration_ms: number;
  status_code: string;
  environment?: string;
  start_time_unix_nano: number | string;
}

const NOW_NS = () => BigInt(Date.now()) * 1_000_000n;

const STUB_NLQ_TRACE_ROWS: NlqTraceRow[] = [
  {
    trace_id: "playground-trace-1",
    root_service: "checkout",
    root_operation: "POST /checkout",
    duration_ms: 12,
    status_code: "OK",
    environment: "production",
    start_time_unix_nano: String(NOW_NS()),
  },
  {
    trace_id: "playground-trace-2",
    root_service: "payment",
    root_operation: "POST /charge",
    duration_ms: 340,
    status_code: "ERROR",
    environment: "production",
    start_time_unix_nano: String(NOW_NS() - 30_000_000_000n),
  },
];

const STUB_NLQ_IR: NlqIr = {
  operation: "table",
  signals: ["traces"],
  filters: [],
  group_by: [],
  time_range: { from: "now-1h", to: "now" },
  metric: null,
  window: null,
  resolution: null,
  visualization_hint: "table",
};

const STUB_NLQ_FRAME: VisualizationFrame = {
  frame_type: "table",
  x_field: null,
  y_field: null,
  series_field: null,
  unit: null,
  suggested_visualization: "table",
  field_roles: [],
  data: STUB_NLQ_TRACE_ROWS as unknown as Record<string, unknown>[],
  nlq_ir: STUB_NLQ_IR,
  source_sql: "-- playground fixture data, not executed",
  time_range: { from: "now-1h", to: "now" },
  signal_types: ["traces"],
  sample_rate: null,
  approximation_statement: "",
};

const STUB_LOG_RECORDS: LogRecord[] = [
  {
    tenant_id: DEMO_TENANT_ID,
    log_id: "playground-log-1",
    timestamp_unix_nano: Number(NOW_NS()),
    observed_timestamp_unix_nano: Number(NOW_NS()),
    severity_number: 9,
    severity_text: "INFO",
    body: "checkout request completed",
    attributes: {},
    resource_attributes: {},
    service_name: "checkout",
    environment: "production",
    host_id: "playground-host",
  },
  {
    tenant_id: DEMO_TENANT_ID,
    log_id: "playground-log-2",
    timestamp_unix_nano: Number(NOW_NS() - 30_000_000_000n),
    observed_timestamp_unix_nano: Number(NOW_NS() - 30_000_000_000n),
    severity_number: 17,
    severity_text: "ERROR",
    body: "payment charge failed: card declined",
    attributes: {},
    resource_attributes: {},
    service_name: "payment",
    environment: "production",
    host_id: "playground-host",
  },
];

const STUB_LOG_IR: NlqIr = {
  operation: "table",
  signals: ["logs"],
  filters: [],
  group_by: [],
  time_range: { from: "now-1h", to: "now" },
  metric: null,
  window: null,
  resolution: null,
  visualization_hint: "table",
};

const STUB_LOG_FRAME: VisualizationFrame = {
  frame_type: "table",
  x_field: null,
  y_field: null,
  series_field: null,
  unit: null,
  suggested_visualization: "table",
  field_roles: [],
  data: STUB_LOG_RECORDS as unknown as Record<string, unknown>[],
  nlq_ir: STUB_LOG_IR,
  source_sql: "-- playground fixture data, not executed",
  time_range: { from: "now-1h", to: "now" },
  signal_types: ["logs"],
  sample_rate: null,
  approximation_statement: "",
};

const STUB_DASHBOARD: Dashboard = {
  dashboard_id: "playground-dashboard-1",
  name: "playground dashboard",
  visibility: "private",
  panels: [],
  created_at: new Date(0).toISOString(),
};

export const playgroundRuntime: RuntimeApi = {
  mode: "playground",
  tenants: {
    async list(): Promise<TenantListResponse> {
      return { tenants: [{ id: DEMO_TENANT_ID, name: DEMO_TENANT_NAME }] };
    },
    async listEnvironments(): Promise<EnvironmentListResponse> {
      return { environments: [{ environment: "production" }, { environment: "staging" }] };
    },
  },
  traces: {
    async search(): Promise<TraceListResponse> {
      return {
        traces: [STUB_TRACE],
        total: 1,
        facets: { service_name: [{ value: "checkout", count: 1 }] },
      };
    },
    async histogram(_tenantId: string, params: TraceHistogramParams): Promise<TraceHistogramResponse> {
      if (params.from && params.to) {
        const { executeTraceHistogram } = await import("../playground/engineClient");
        const { buckets } = await executeTraceHistogram({
          fromNs: params.from,
          toNs: params.to,
          bucketCount: params.buckets ?? 30,
          service: params.service,
        });
        return { buckets };
      }
      return { buckets: [{ start_ms: 0, end_ms: 60_000, count: 1 }] };
    },
  },
  logs: {
    // Fixture only — not yet wired to the real DuckDB engine (traces got
    // that first; logs is a follow-up slice).
    async histogram(_tenantId: string, _params: LogHistogramParams): Promise<LogHistogramResponse> {
      return { buckets: [{ start_ms: 0, end_ms: 60_000, counts: { INFO: 1, ERROR: 1 } }] };
    },
  },
  nlq: {
    async execute(_tenantId: string, request: NlqRequest): Promise<NlqResponse> {
      const ir = request.base_ir;
      if (
        !request.question &&
        ir &&
        ir.operation === "table" &&
        ir.signals?.length === 1 &&
        ir.signals[0] === "traces"
      ) {
        const { executeTraceTable } = await import("../playground/engineClient");
        const { rows, sql } = await executeTraceTable(ir);
        return {
          type: "frame",
          frame: {
            ...STUB_NLQ_FRAME,
            data: rows as unknown as Record<string, unknown>[],
            nlq_ir: ir as NlqIr,
            source_sql: sql,
          },
        };
      }

      // Free-text NLQ questions and every other operation/signal still use
      // fixture data — shorthand/IR-merge logic isn't wired yet. Match the
      // fixture shape to the requested signal so at least the response
      // *shape* is correct even when the content is static.
      if (!request.question && ir?.signals?.length === 1 && ir.signals[0] === "logs") {
        return { type: "frame", frame: STUB_LOG_FRAME };
      }
      return { type: "frame", frame: STUB_NLQ_FRAME };
    },
  },
  dashboards: {
    async create(): Promise<Dashboard> {
      return STUB_DASHBOARD;
    },
  },
};
