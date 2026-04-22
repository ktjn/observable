const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function tenantHeaders(): HeadersInit {
  return { "X-Tenant-ID": DEV_TENANT_ID };
}

export interface MetricSeries {
  tenant_id: string;
  metric_series_id: string;
  metric_name: string;
  description: string;
  unit: string;
  metric_type: string;
  is_monotonic?: boolean;
  aggregation_temporality?: string;
  attributes: Record<string, unknown>;
  resource_attributes: Record<string, unknown>;
  service_name: string;
  environment: string;
}

export interface MetricSeriesListResponse {
  series: MetricSeries[];
}

export async function listMetrics(params: {
  service?: string;
} = {}): Promise<MetricSeriesListResponse> {
  const url = new URL("/v1/metrics", window.location.origin);
  if (params.service) url.searchParams.set("service", params.service);

  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
