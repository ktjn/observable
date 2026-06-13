import type { MetricPoint } from "./generated/metrics/metrics.MetricPoint.v1";

export type { MetricPoint };

function tenantHeaders(tenantId: string): HeadersInit {
  return { "X-Tenant-ID": tenantId };
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

export interface MetricPointsResponse {
  points: MetricPoint[];
}

export async function listMetrics(tenantId: string, params: {
  service?: string;
} = {}): Promise<MetricCatalogResponse> {
  const url = new URL("/v1/metrics", window.location.origin);
  if (params.service) url.searchParams.set("service", params.service);

  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}

export async function getMetricGroupPoints(tenantId: string, metric: MetricCatalogEntry): Promise<MetricPointsResponse> {
  const url = new URL("/v1/metrics/points", window.location.origin);
  url.searchParams.set("metric_name", metric.metric_name);
  url.searchParams.set("service", metric.service_name);
  url.searchParams.set("environment", metric.environment || "default");
  url.searchParams.set("metric_type", metric.metric_type);
  url.searchParams.set("unit", metric.unit || "");

  const res = await fetch(url.toString(), { credentials: "include", headers: tenantHeaders(tenantId) });
  if (!res.ok) throw new Error(`Query failed: ${res.status}`);
  return res.json();
}
