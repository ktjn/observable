/**
 * @modelable domain: tracing
 * @modelable name: SpanEventRow
 * @modelable owner: platform-team
 * @modelable kind: projection
 * @modelable version: 1
 * @modelable source: tracing.SpanEvent@1
 */
export interface TracingSpanEventRowV1 {
  tenant_id: string;
  trace_id: string;
  span_id: string;
  event_index: number;
  name: string;
  timestamp_unix_nano: number;
  attributes: Record<string, unknown>;
}
export type SpanEventRow = TracingSpanEventRowV1;
