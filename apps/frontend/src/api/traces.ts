import type { Span } from "./generated/tracing/tracing.Span.v1";
import type { SpanEvent } from "./generated/tracing/tracing.SpanEvent.v1";
import { dispatch } from "../runtime/requestTransport";

export type { Span, SpanEvent };

export interface TraceResponse {
  trace_id: string;
  spans: Span[];
  events: SpanEvent[];
}

export interface FacetValue {
  value: string;
  count: number;
}

export interface Facets {
  [field: string]: FacetValue[];
}

export interface TraceListResponse {
  traces: TraceResponse[];
  total: number;
  facets: Facets;
}

export interface TraceHistogramBucket {
  start_ms: number;
  end_ms: number;
  count: number;
}

export interface TraceHistogramResponse {
  buckets: TraceHistogramBucket[];
}

export async function fetchTraceHistogram(_tenantId: string, params: {
  service?: string;
  from?: string;
  to?: string;
  buckets?: number;
}): Promise<TraceHistogramResponse> {
  return dispatch<TraceHistogramResponse>({
    method: "GET",
    path: "/v1/traces/histogram",
    query: { service: params.service, from: params.from, to: params.to, buckets: params.buckets },
  });
}

export async function searchTraces(_tenantId: string, params: {
  service?: string;
  limit?: number;
  facets?: string[];
  from?: string;
  to?: string;
}): Promise<TraceListResponse> {
  return dispatch<TraceListResponse>({
    method: "GET",
    path: "/v1/traces",
    query: {
      service: params.service,
      limit: params.limit,
      facets: params.facets?.join(","),
      from: params.from,
      to: params.to,
    },
  });
}

export async function getTrace(_tenantId: string, traceId: string): Promise<TraceResponse> {
  return dispatch<TraceResponse>({ method: "GET", path: `/v1/traces/${traceId}` });
}
