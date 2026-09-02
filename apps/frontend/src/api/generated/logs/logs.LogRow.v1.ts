/**
 * @modelable domain: logs
 * @modelable name: LogRow
 * @modelable owner: platform-team
 * @modelable kind: projection
 * @modelable version: 1
 * @modelable source: logs.LogRecord@1
 */
export interface LogsLogRowV1 {
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
export type LogRow = LogsLogRowV1;
