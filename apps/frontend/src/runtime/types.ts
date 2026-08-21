import type { TraceHistogramResponse, TraceListResponse } from "../api/traces";

export interface SearchTracesParams {
  service?: string;
  limit?: number;
  facets?: string[];
  from?: string;
  to?: string;
}

export interface TraceHistogramParams {
  service?: string;
  from?: string;
  to?: string;
  buckets?: number;
}

/**
 * Typed seam between UI components and either the production HTTP backend
 * (`httpRuntime`) or the browser-local playground engine (`playgroundRuntime`).
 * Components must depend only on this interface, never on the transport.
 */
export interface RuntimeApi {
  readonly mode: "http" | "playground";
  traces: {
    search(tenantId: string, params: SearchTracesParams): Promise<TraceListResponse>;
    histogram(tenantId: string, params: TraceHistogramParams): Promise<TraceHistogramResponse>;
  };
}
