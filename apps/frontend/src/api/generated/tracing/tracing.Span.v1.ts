/**
 * @modelable domain: tracing
 * @modelable name: Span
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface TracingSpanV1 {
  span_id: string;
  trace_id: string;
  parent_span_id?: string;
  tenant_id: string;
  service_name: string;
  service_namespace: string;
  service_version: string;
  operation_name: string;
  span_kind: 'INTERNAL' | 'SERVER' | 'CLIENT' | 'PRODUCER' | 'CONSUMER';
  start_time_unix_nano: number;
  end_time_unix_nano: number;
  duration_ns: number;
  status_code: 'UNSET' | 'OK' | 'ERROR';
  status_message: string;
  attributes: Record<string, unknown>;
  resource_attributes: Record<string, unknown>;
  environment: string;
  host_id: string;
  workload: string;
  deployment_id: string;
}
export type Span = TracingSpanV1;
