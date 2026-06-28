/**
 * @modelable domain: metrics
 * @modelable name: MetricPoint
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface MetricsMetricPointV1 {
  tenant_id: string;
  metric_series_id: string;
  metric_name: string;
  service_name: string;
  time_unix_nano: number;
  start_time_unix_nano?: number;
  value_double?: number;
  value_int?: number;
  histogram_count?: number;
  histogram_sum?: number;
  histogram_bucket_counts?: number[];
  histogram_explicit_bounds?: number[];
}
export type MetricPoint = MetricsMetricPointV1;
