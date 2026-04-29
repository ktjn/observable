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
    searchLogs({ from: new Date(Date.now() - 60 * 60 * 1000).toISOString(), limit: 1 }),
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

// ── LLM / AI config ───────────────────────────────────────────────────────────

export interface PlatformConfig {
  llm_key_configured: boolean;
  /** LLM endpoint URL stored in DB; null if not set. */
  llm_url: string | null;
  /** LLM model identifier stored in DB; null if not set. */
  llm_model: string | null;
}

export async function getConfig(): Promise<PlatformConfig> {
  const res = await fetch("/v1/config", {
    headers: { "x-api-key": LOCAL_DEV_API_KEY },
  });
  if (!res.ok) throw new Error(`getConfig failed: ${res.status}`);
  return res.json() as Promise<PlatformConfig>;
}

export interface SaveLlmConfigParams {
  apiKey?: string;
  url?: string;
  model?: string;
}

/** PUT /v1/config/llm — upserts whichever of apiKey, url, model are provided. */
export async function saveLlmConfig(params: SaveLlmConfigParams): Promise<void> {
  const body: Record<string, string> = {};
  if (params.apiKey !== undefined) body.api_key = params.apiKey;
  if (params.url !== undefined) body.url = params.url;
  if (params.model !== undefined) body.model = params.model;

  const res = await fetch("/v1/config/llm", {
    method: "PUT",
    headers: {
      "Content-Type": "application/json",
      "x-api-key": LOCAL_DEV_API_KEY,
    },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`saveLlmConfig failed: ${res.status}`);
}

/** Legacy alias — kept for backwards compatibility. */
export async function saveLlmKey(key: string): Promise<void> {
  return saveLlmConfig({ apiKey: key });
}
