export interface Span {
  tenant_id: string;
  trace_id: string;
  span_id: string;
  service_name: string;
  operation_name: string;
  start_time_unix_nano: string;
  end_time_unix_nano: string;
  duration_ns: number;
  status_code: string;
}

export interface TraceResponse {
  trace_id: string;
  spans: Span[];
}

export interface TraceListResponse {
  traces: TraceResponse[];
  total: number;
}

export async function searchTraces(params: {
  service?: string;
  limit?: number;
}): Promise<TraceListResponse> {
  const url = new URL("/v1/traces", window.location.origin);
  if (params.service) url.searchParams.set("service", params.service);
  if (params.limit) url.searchParams.set("limit", String(params.limit));
  const res = await fetch(url.toString());
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function getTrace(traceId: string): Promise<TraceResponse> {
  const res = await fetch(`/v1/traces/${traceId}`);
  if (!res.ok) throw new Error(`Not found: ${res.status}`);
  return res.json();
}
