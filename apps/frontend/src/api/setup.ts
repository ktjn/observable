import { searchLogs } from "./logs";
import { listMetrics } from "./metrics";
import { searchTraces } from "./traces";

export const LOCAL_DEV_TENANT = "local-dev";
export const LOCAL_DEV_TENANT_ID = "00000000-0000-0000-0000-000000000001";
export const OTLP_HTTP_TRACE_ENDPOINT = "http://localhost:4318/v1/traces";
export const LOCAL_DEV_API_KEY = "dev-api-key-0000";
export const REDACTED_LOCAL_API_KEY = "dev-api-key-...-0000";

export interface FirstSignalStatus {
  state: "detected" | "waiting" | "error";
  traces: number;
  logs: number;
  metrics: number;
}

export async function getFirstSignalStatus(): Promise<FirstSignalStatus> {
  const [traces, logs, metrics] = await Promise.allSettled([
    searchTraces({ lookback_minutes: 60, limit: 1 }),
    searchLogs({ lookback_minutes: 60, limit: 1 }),
    listMetrics(),
  ]);

  if (traces.status === "rejected" || logs.status === "rejected" || metrics.status === "rejected") {
    return { state: "error", traces: 0, logs: 0, metrics: 0 };
  }

  const traceCount = traces.value.total ?? traces.value.traces.length;
  const logCount = logs.value.total ?? logs.value.logs.length;
  const metricCount = metrics.value.series.length;
  const hasSignal = traceCount + logCount + metricCount > 0;

  return {
    state: hasSignal ? "detected" : "waiting",
    traces: traceCount,
    logs: logCount,
    metrics: metricCount,
  };
}
