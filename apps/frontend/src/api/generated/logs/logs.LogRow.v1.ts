/**
 * @modelable domain: logs
 * @modelable name: LogRow
 * @modelable owner: platform-team
 * @modelable kind: projection
 * @modelable version: 1
 * @modelable source: logs.LogRecord@1
 */
export interface LogsLogRowV1 {
  tenantId: string;
  logId: string;
  timestampUnixNano: number;
  observedTimestampUnixNano: number;
  severityNumber: number;
  severityText: string;
  body: unknown;
  traceId: string;
  spanId: string;
  attributes: Record<string, unknown>;
  resourceAttributes: Record<string, unknown>;
  serviceName: string;
  environment: string;
  hostId: string;
  fingerprint: number;
}
export type LogRow = LogsLogRowV1;
