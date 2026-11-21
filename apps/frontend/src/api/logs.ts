import type { LogRecord } from "./generated/logs/logs.LogRecord.v1";
import { dispatch } from "../runtime/requestTransport";

export type { LogRecord };

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

export async function searchLogs(_tenantId: string, params: {
  service?: string;
  trace_id?: string;
  span_id?: string;
  from?: string;
  to?: string;
  limit?: number;
  facets?: string[];
}): Promise<LogListResponse> {
  return dispatch<LogListResponse>({
    method: "GET",
    path: "/v1/logs",
    query: {
      service: params.service,
      trace_id: params.trace_id,
      span_id: params.span_id,
      from: params.from,
      to: params.to,
      limit: params.limit,
      facets: params.facets?.join(","),
    },
  });
}

export interface LogHistogramBucket {
  start_ms: number;
  end_ms: number;
  counts: Record<string, number>;
}

export interface LogHistogramResponse {
  buckets: LogHistogramBucket[];
}

export async function fetchLogHistogram(_tenantId: string, params: {
  service?: string;
  from: string;
  to: string;
  buckets?: number;
}): Promise<LogHistogramResponse> {
  return dispatch<LogHistogramResponse>({
    method: "GET",
    path: "/v1/logs/histogram",
    query: { service: params.service, from: params.from, to: params.to, buckets: params.buckets },
  });
}

export async function tailLogs(_tenantId: string, params: {
  service?: string;
  severity?: number;
  since_unix_nano?: string;
  limit?: number;
}): Promise<LogListResponse> {
  return dispatch<LogListResponse>({
    method: "GET",
    path: "/v1/logs/tail",
    query: {
      service: params.service,
      severity: params.severity,
      since_unix_nano: params.since_unix_nano,
      limit: params.limit,
    },
  });
}

export async function getLogContext(
  _tenantId: string,
  logId: string,
  params: { window?: number } = {}
): Promise<LogListResponse> {
  return dispatch<LogListResponse>({
    method: "GET",
    path: `/v1/logs/${logId}/context`,
    query: { window: params.window },
  });
}
