/**
 * @modelable domain: tracing
 * @modelable name: SpanRow
 * @modelable owner: platform-team
 * @modelable kind: projection
 * @modelable version: 1
 * @modelable source: tracing.Span@1
 */
export interface TracingSpanRowV1 {
  tenantId: string;
  traceId: string;
  spanId: string;
  parentSpanId: string;
  serviceName: string;
  serviceNamespace: string;
  serviceVersion: string;
  operationName: string;
  spanKind: 'Internal' | 'Server' | 'Client' | 'Producer' | 'Consumer';
  startTimeUnixNano: number;
  endTimeUnixNano: number;
  durationNs: number;
  statusCode: 'Unset' | 'Ok' | 'Error';
  statusMessage: string;
  attributes: Record<string, unknown>;
  resourceAttributes: Record<string, unknown>;
  environment: string;
  hostId: string;
  workload: string;
  deploymentId: string;
}
export type SpanRow = TracingSpanRowV1;
