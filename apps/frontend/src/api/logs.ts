const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function tenantHeaders(): HeadersInit {
  return { "X-Tenant-ID": DEV_TENANT_ID };
}

export interface LogRecord {
  tenant_id: string;
  log_id: string;
  timestamp_unix_nano: string;
  observed_timestamp_unix_nano?: string;
  severity_number: number;
  severity_text: string;
  body: unknown;
  trace_id?: string;
  span_id?: string;
  service_name: string;
  environment?: string;
  host_id?: string;
  fingerprint?: number | string | null;
  attributes?: Record<string, unknown>;
  resource_attributes?: Record<string, unknown>;
}

export interface FacetValue {
  value: string;
  count: number;
}

export interface Facets {
  [field: string]: FacetValue[];
}

export interface LogListResponse {
  logs: LogRecord[];
  total: number;
  facets: Facets;
}

export async function searchLogs(params: {
  service?: string;
  trace_id?: string;
  span_id?: string;
  lookback_minutes?: number;
  limit?: number;
  facets?: string[];
}): Promise<LogListResponse> {
  const url = new URL("/v1/logs", window.location.origin);
  if (params.service) url.searchParams.set("service", params.service);
  if (params.trace_id) url.searchParams.set("trace_id", params.trace_id);
  if (params.span_id) url.searchParams.set("span_id", params.span_id);
  if (params.lookback_minutes) {
    url.searchParams.set("lookback_minutes", String(params.lookback_minutes));
  }
  if (params.limit) url.searchParams.set("limit", String(params.limit));
  if (params.facets) url.searchParams.set("facets", params.facets.join(","));

  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function tailLogs(params: {
  service?: string;
  severity?: number;
  since_unix_nano?: string;
  limit?: number;
}): Promise<LogListResponse> {
  const url = new URL("/v1/logs/tail", window.location.origin);
  if (params.service) url.searchParams.set("service", params.service);
  if (params.severity !== undefined) {
    url.searchParams.set("severity", String(params.severity));
  }
  if (params.since_unix_nano) {
    url.searchParams.set("since_unix_nano", params.since_unix_nano);
  }
  if (params.limit) url.searchParams.set("limit", String(params.limit));

  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Live tail failed: ${res.status}`);
  return res.json();
}

export async function getLogContext(
  logId: string,
  params: { window?: number } = {}
): Promise<LogListResponse> {
  const url = new URL(`/v1/logs/${logId}/context`, window.location.origin);
  if (params.window) url.searchParams.set("window", String(params.window));

  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
