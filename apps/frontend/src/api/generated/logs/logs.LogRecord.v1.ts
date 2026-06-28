/**
 * @modelable domain: logs
 * @modelable name: LogRecord
 * @modelable owner: platform-team
 * @modelable kind: entity
 * @modelable version: 1
 * @modelable changeKind: additive
 */
export interface LogsLogRecordV1 {
  tenant_id: string;
  log_id: string;
  timestamp_unix_nano: number;
  observed_timestamp_unix_nano: number;
  severity_number: number;
  severity_text: string;
  body: unknown;
  trace_id?: string;
  span_id?: string;
  attributes: Record<string, unknown>;
  resource_attributes: Record<string, unknown>;
  service_name: string;
  environment: string;
  host_id: string;
  fingerprint?: number;
}
export type LogRecord = LogsLogRecordV1;
