import { dispatch } from "../runtime/requestTransport";

function tenantHeaders(_tenantId: string): HeadersInit {
  return {};
}

function msToIso(ms?: number): string | undefined {
  return ms != null ? new Date(ms).toISOString() : undefined;
}

export interface ServiceSummary {
  service_name: string;
  request_rate: number;
  error_rate: number;
  p95_latency_ms: number;
  /** A legacy SLO burn-rate alert is firing; not a health assessment. */
  slo_breaching: boolean;
  active_alert_count: number;
  latest_deployment: string | null;
}

export interface ServiceSummaryResponse {
  items: ServiceSummary[];
}

export interface ServiceDetailResponse {
  service: ServiceSummary;
}

export async function listServiceSummaries(_tenantId: string, params: {
  environment?: string;
  from?: number;
  to?: number;
} = {}): Promise<ServiceSummaryResponse> {
  return dispatch<ServiceSummaryResponse>({
    method: "GET",
    path: "/v1/services/summary",
    query: { environment: params.environment, from: msToIso(params.from), to: msToIso(params.to) },
  });
}

export async function getServiceSummary(
  _tenantId: string,
  serviceName: string,
  params: {
    environment?: string;
    from?: number;
    to?: number;
  } = {},
): Promise<ServiceDetailResponse> {
  const encodedService = encodeURIComponent(serviceName);
  return dispatch<ServiceDetailResponse>({
    method: "GET",
    path: `/v1/services/${encodedService}/summary`,
    query: { environment: params.environment, from: msToIso(params.from), to: msToIso(params.to) },
  });
}

export interface DiscoveryResponse {
  items: string[];
}

export interface TopologyEdge {
  caller: string;
  callee: string;
  request_count: number;
  error_rate: number;
  p95_latency_ms: number;
}

export interface TopologyResponse {
  edges: TopologyEdge[];
}

export async function getTopology(_tenantId: string, params: {
  environment?: string;
  from?: number;
  to?: number;
  service?: string;
} = {}): Promise<TopologyResponse> {
  return dispatch<TopologyResponse>({
    method: "GET",
    path: "/v1/topology",
    query: {
      environment: params.environment,
      from: msToIso(params.from),
      to: msToIso(params.to),
      service: params.service,
    },
  });
}

export async function listEnvironments(tenantId: string): Promise<DiscoveryResponse> {
  const url = new URL("/v1/environments", window.location.origin);
  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function listServices(_tenantId: string): Promise<DiscoveryResponse> {
  return dispatch<DiscoveryResponse>({ method: "GET", path: "/v1/services" });
}

export interface ResponseTimeHistoryBucket {
  start_ms: number;
  end_ms: number;
  p50_ms: number;
  p95_ms: number;
  request_rate: number;
}

export interface ResponseTimeHistoryResponse {
  buckets: ResponseTimeHistoryBucket[];
}

export async function getServiceResponseTimeHistory(
  _tenantId: string,
  serviceName: string,
  params: {
    from?: number;
    to?: number;
    buckets?: number;
  } = {},
): Promise<ResponseTimeHistoryResponse> {
  const encodedService = encodeURIComponent(serviceName);
  return dispatch<ResponseTimeHistoryResponse>({
    method: "GET",
    path: `/v1/services/${encodedService}/response-time-history`,
    query: { from: msToIso(params.from), to: msToIso(params.to), buckets: params.buckets },
  });
}
