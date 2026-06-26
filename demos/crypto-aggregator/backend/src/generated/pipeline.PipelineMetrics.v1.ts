/**
 * @modelable domain: pipeline
 * @modelable name: PipelineMetrics
 * @modelable owner: crypto-demo-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface PipelinePipelineMetricsV1 {
  snapshot_id: string;
  ingest_rate: number;
  correlation_lag_ms: number;
  buffer_fill_ratio: number;
  exporter_latency_ms: number;
  error_count: number;
  ts_unix_ms: number;
}
export type PipelineMetrics = PipelinePipelineMetricsV1;
