import type { LogRecord } from "./generated/logs/logs.LogRecord.v1";

export type { LogRecord };

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
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

export async function searchLogs(tenantId: string, params: {
  service?: string;
  trace_id?: string;
  span_id?: string;
  from?: string;
  to?: string;
  limit?: number;
  facets?: string[];
}): Promise<LogListResponse> {
  const url = new URL("/v1/logs", window.location.origin);
  if (params.service) url.searchParams.set("service", params.service);
  if (params.trace_id) url.searchParams.set("trace_id", params.trace_id);
  if (params.span_id) url.searchParams.set("span_id", params.span_id);
  if (params.from) url.searchParams.set("from", params.from);
  if (params.to) url.searchParams.set("to", params.to);
  if (params.limit) url.searchParams.set("limit", String(params.limit));
  if (params.facets) url.searchParams.set("facets", params.facets.join(","));

  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export interface LogHistogramBucket {
  start_ms: number;
  end_ms: number;
  counts: Record<string, number>;
}

export interface LogHistogramResponse {
  buckets: LogHistogramBucket[];
}

export async function fetchLogHistogram(tenantId: string, params: {
  service?: string;
  from: string;
  to: string;
  buckets?: number;
}): Promise<LogHistogramResponse> {
  const url = new URL("/v1/logs/histogram", window.location.origin);
  if (params.service) url.searchParams.set("service", params.service);
  url.searchParams.set("from", params.from);
  url.searchParams.set("to", params.to);
  if (params.buckets) url.searchParams.set("buckets", String(params.buckets));

  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Histogram query failed: ${res.status}`);
  return res.json();
}

export async function tailLogs(tenantId: string, params: {
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

  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Live tail failed: ${res.status}`);
  return res.json();
}

export async function getLogContext(
  tenantId: string,
  logId: string,
  params: { window?: number } = {}
): Promise<LogListResponse> {
  const url = new URL(`/v1/logs/${logId}/context`, window.location.origin);
  if (params.window) url.searchParams.set("window", String(params.window));

  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
