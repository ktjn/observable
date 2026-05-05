export interface Span {
  tenant_id: string;
  trace_id: string;
  span_id: string;
  parent_span_id?: string;
  service_name: string;
  service_namespace: string;
  service_version: string;
  operation_name: string;
  span_kind: string;
  start_time_unix_nano: number;
  end_time_unix_nano: number;
  duration_ns: number;
  status_code: string;
  status_message: string;
  attributes?: Record<string, unknown>;
  resource_attributes?: Record<string, unknown>;
  environment: string;
  host_id: string;
  workload: string;
  deployment_id: string;
}

export interface SpanEvent {
  span_id: string;
  event_index: number;
  name: string;
  timestamp_unix_nano: number;
  attributes?: Record<string, unknown>;
}

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

  const res = await fetch(url.toString(), { headers: tenantHeaders(tenantId) });
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

  const res = await fetch(url.toString(), { headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function getTrace(tenantId: string, traceId: string): Promise<TraceResponse> {
  const res = await fetch(`/v1/traces/${traceId}`, { headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Not found: ${res.status}`);
  return res.json();
}
