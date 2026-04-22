export interface Span {
  tenant_id: string;
  trace_id: string;
  span_id: string;
  service_name: string;
  operation_name: string;
  start_time_unix_nano: number;
  end_time_unix_nano: number;
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

const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function tenantHeaders(): HeadersInit {
  return { "X-Tenant-ID": DEV_TENANT_ID };
}

export async function searchTraces(params: {
  service?: string;
  lookback_minutes?: number;
  limit?: number;
}): Promise<TraceListResponse> {
  const url = new URL("/v1/traces", window.location.origin);
  if (params.service) url.searchParams.set("service", params.service);
  if (params.lookback_minutes) {
    url.searchParams.set("lookback_minutes", String(params.lookback_minutes));
  }
  if (params.limit) url.searchParams.set("limit", String(params.limit));
  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function getTrace(traceId: string): Promise<TraceResponse> {
  const res = await fetch(`/v1/traces/${traceId}`, { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Not found: ${res.status}`);
  return res.json();
}
