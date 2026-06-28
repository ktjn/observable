/**
 * @modelable domain: tracing
 * @modelable name: SpanEventRow
 * @modelable owner: platform-team
 * @modelable kind: projection
 * @modelable version: 1
 * @modelable source: tracing.SpanEvent@1
 */
export interface TracingSpanEventRowV1 {
  tenantId: string;
  traceId: string;
  spanId: string;
  eventIndex: number;
  name: string;
  timestampUnixNano: number;
  attributes: Record<string, unknown>;
}
export type SpanEventRow = TracingSpanEventRowV1;
