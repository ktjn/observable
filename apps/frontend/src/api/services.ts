const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function tenantHeaders(): HeadersInit {
  return { "X-Tenant-ID": DEV_TENANT_ID };
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

export async function listServiceSummaries(params: {
  environment?: string;
  lookback_minutes?: number;
} = {}): Promise<ServiceSummaryResponse> {
  const url = new URL("/v1/services/summary", window.location.origin);
  if (params.environment) url.searchParams.set("environment", params.environment);
  if (params.lookback_minutes) {
    url.searchParams.set("lookback_minutes", String(params.lookback_minutes));
  }

  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function getServiceSummary(
  serviceName: string,
  params: {
    environment?: string;
    lookback_minutes?: number;
  } = {},
): Promise<ServiceDetailResponse> {
  const encodedService = encodeURIComponent(serviceName);
  const url = new URL(`/v1/services/${encodedService}/summary`, window.location.origin);
  if (params.environment) url.searchParams.set("environment", params.environment);
  if (params.lookback_minutes) {
    url.searchParams.set("lookback_minutes", String(params.lookback_minutes));
  }

  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export interface DiscoveryResponse {
  items: string[];
}

export async function listEnvironments(): Promise<DiscoveryResponse> {
  const url = new URL("/v1/environments", window.location.origin);
  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
