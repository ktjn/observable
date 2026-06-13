import type { Span } from "./generated/tracing/tracing.Span.v1";
import type { SpanEvent } from "./generated/tracing/tracing.SpanEvent.v1";

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

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

export interface TraceHistogramBucket {
  start_ms: number;
  end_ms: number;
  count: number;
}

export interface TraceHistogramResponse {
  buckets: TraceHistogramBucket[];
}

export async function fetchTraceHistogram(tenantId: string, params: {
  service?: string;
  from?: string;
  to?: string;
  buckets?: number;
}): Promise<TraceHistogramResponse> {
  const url = new URL("/v1/traces/histogram", window.location.origin);
  if (params.service) url.searchParams.set("service", params.service);
  if (params.from) url.searchParams.set("from", params.from);
  if (params.to) url.searchParams.set("to", params.to);
  if (params.buckets) url.searchParams.set("buckets", String(params.buckets));

  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Histogram query failed: ${res.status}`);
  return res.json();
}

export async function searchTraces(tenantId: string, params: {
  service?: string;
  limit?: number;
  facets?: string[];
  from?: string;
  to?: string;
}): Promise<TraceListResponse> {
  const url = new URL("/v1/traces", window.location.origin);
  if (params.service) url.searchParams.set("service", params.service);
  if (params.limit) url.searchParams.set("limit", String(params.limit));
  if (params.facets) url.searchParams.set("facets", params.facets.join(","));
  if (params.from) url.searchParams.set("from", params.from);
  if (params.to) url.searchParams.set("to", params.to);

  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function getTrace(tenantId: string, traceId: string): Promise<TraceResponse> {
  const res = await fetch(`/v1/traces/${traceId}`, { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Not found: ${res.status}`);
  return res.json();
}
