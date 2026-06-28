/**
 * @modelable domain: tracing
 * @modelable name: SpanEvent
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface TracingSpanEventV1 {
  tenant_id: string;
  trace_id: string;
  span_id: string;
  event_index: number;
  name: string;
  timestamp_unix_nano: number;
  attributes: Record<string, unknown>;
}
export type SpanEvent = TracingSpanEventV1;
