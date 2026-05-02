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

export interface MetricPoint {
  tenant_id: string;
  metric_series_id: string;
  metric_name: string;
  service_name: string;
  time_unix_nano: number | string;
  start_time_unix_nano?: number | string | null;
  value_double?: number | null;
  value_int?: number | null;
  histogram_count?: number | null;
  histogram_sum?: number | null;
  histogram_bucket_counts?: number[];
  histogram_explicit_bounds?: number[];
}

export interface MetricPointsResponse {
  points: MetricPoint[];
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

export async function getMetricPoints(seriesId: string): Promise<MetricPointsResponse> {
  const url = new URL(`/v1/metrics/${seriesId}`, window.location.origin);
  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
