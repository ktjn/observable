import type { Span, TraceHistogramResponse, TraceListResponse, TraceResponse } from "../api/traces";
import type { RuntimeApi } from "./types";

/**
 * Phase 1 (start) in-memory stub. Proves the runtime seam end-to-end with fixed
 * fixture data; not yet wired to the playground worker/WASM/DuckDB engine
 * (that wiring is Phase 3).
 */

const STUB_SPAN: Span = {
  span_id: "span-1",
  trace_id: "playground-trace-1",
  tenant_id: "demo",
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

export const playgroundRuntime: RuntimeApi = {
  mode: "playground",
  traces: {
    async search(): Promise<TraceListResponse> {
      return {
        traces: [STUB_TRACE],
        total: 1,
        facets: { service_name: [{ value: "checkout", count: 1 }] },
      };
    },
    async histogram(): Promise<TraceHistogramResponse> {
      return {
        buckets: [{ start_ms: 0, end_ms: 60_000, count: 1 }],
      };
    },
  },
};
