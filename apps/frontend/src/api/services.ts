function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
}

function msToIso(ms?: number): string | undefined {
  return ms != null ? new Date(ms).toISOString() : undefined;
}

export interface ServiceSummary {
  service_name: string;
  request_rate: number;
  error_rate: number;
  p95_latency_ms: number;
  health_state: "healthy" | "watch" | "breach";
  active_alert_count: number;
  latest_deployment: string | null;
}

export interface ServiceSummaryResponse {
  items: ServiceSummary[];
}

export interface ServiceDetailResponse {
  service: ServiceSummary;
}

export async function listServiceSummaries(tenantId: string, params: {
  environment?: string;
  from?: number;
  to?: number;
} = {}): Promise<ServiceSummaryResponse> {
  const url = new URL("/v1/services/summary", window.location.origin);
  if (params.environment) url.searchParams.set("environment", params.environment);
  const fromIso = msToIso(params.from);
  const toIso = msToIso(params.to);
  if (fromIso) url.searchParams.set("from", fromIso);
  if (toIso) url.searchParams.set("to", toIso);
  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function getServiceSummary(
  tenantId: string,
  serviceName: string,
  params: {
    environment?: string;
    from?: number;
    to?: number;
  } = {},
): Promise<ServiceDetailResponse> {
  const encodedService = encodeURIComponent(serviceName);
  const url = new URL(`/v1/services/${encodedService}/summary`, window.location.origin);
  if (params.environment) url.searchParams.set("environment", params.environment);
  const fromIso = msToIso(params.from);
  const toIso = msToIso(params.to);
  if (fromIso) url.searchParams.set("from", fromIso);
  if (toIso) url.searchParams.set("to", toIso);
  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
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

export async function getTopology(tenantId: string, params: {
  environment?: string;
  from?: number;
  to?: number;
  service?: string;
} = {}): Promise<TopologyResponse> {
  const url = new URL("/v1/topology", window.location.origin);
  if (params.environment) url.searchParams.set("environment", params.environment);
  const fromIso = msToIso(params.from);
  const toIso = msToIso(params.to);
  if (fromIso) url.searchParams.set("from", fromIso);
  if (toIso) url.searchParams.set("to", toIso);
  if (params.service) url.searchParams.set("service", params.service);
  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function listEnvironments(tenantId: string): Promise<DiscoveryResponse> {
  const url = new URL("/v1/environments", window.location.origin);
  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function listServices(tenantId: string): Promise<DiscoveryResponse> {
  const url = new URL("/v1/services", window.location.origin);
  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
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
  tenantId: string,
  serviceName: string,
  params: {
    from?: number;
    to?: number;
    buckets?: number;
  } = {},
): Promise<ResponseTimeHistoryResponse> {
  const encodedService = encodeURIComponent(serviceName);
  const url = new URL(
    `/v1/services/${encodedService}/response-time-history`,
    window.location.origin,
  );
  const fromIso = msToIso(params.from);
  const toIso = msToIso(params.to);
  if (fromIso) url.searchParams.set("from", fromIso);
  if (toIso) url.searchParams.set("to", toIso);
  if (params.buckets) url.searchParams.set("buckets", String(params.buckets));
  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
