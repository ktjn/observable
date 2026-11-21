import type { MetricPoint } from "./generated/metrics/metrics.MetricPoint.v1";
import { dispatch } from "../runtime/requestTransport";

export type { MetricPoint };

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

export async function listMetrics(_tenantId: string, params: {
  service?: string;
} = {}): Promise<MetricCatalogResponse> {
  return dispatch<MetricCatalogResponse>({
    method: "GET",
    path: "/v1/metrics",
    query: { service: params.service },
  });
}

export async function getMetricGroupPoints(
  _tenantId: string,
  metric: MetricCatalogEntry,
): Promise<MetricPointsResponse> {
  return dispatch<MetricPointsResponse>({
    method: "GET",
    path: "/v1/metrics/points",
    query: {
      metric_name: metric.metric_name,
      service: metric.service_name,
      environment: metric.environment || "default",
      metric_type: metric.metric_type,
      unit: metric.unit || "",
    },
  });
}
