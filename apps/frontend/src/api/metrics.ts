const DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function tenantHeaders(): HeadersInit {
  return { "X-Tenant-ID": DEV_TENANT_ID };
}

export interface MetricCatalogEntry {
  tenant_id: string;
  metric_name: string;
  description: string;
  unit: string;
  metric_type: string;
  is_monotonic?: boolean;
  aggregation_temporality?: string;
  service_name: string;
  environment: string;
  series_count: number;
}

export interface MetricCatalogResponse {
  metrics: MetricCatalogEntry[];
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
} = {}): Promise<MetricCatalogResponse> {
  const url = new URL("/v1/metrics", window.location.origin);
  if (params.service) url.searchParams.set("service", params.service);

  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function getMetricGroupPoints(metric: MetricCatalogEntry): Promise<MetricPointsResponse> {
  const url = new URL("/v1/metrics/points", window.location.origin);
  url.searchParams.set("metric_name", metric.metric_name);
  url.searchParams.set("service", metric.service_name);
  url.searchParams.set("environment", metric.environment || "default");
  url.searchParams.set("metric_type", metric.metric_type);
  url.searchParams.set("unit", metric.unit || "");

  const res = await fetch(url.toString(), { headers: tenantHeaders() });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
