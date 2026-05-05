import { searchLogs } from "./logs";
import { listMetrics } from "./metrics";
import { searchTraces } from "./traces";

export const OTLP_GRPC_ENDPOINT = "http://localhost:4317";
export const OTLP_HTTP_JSON_TRACES = "http://localhost:4318/v1/traces";
export const OTLP_HTTP_JSON_METRICS = "http://localhost:4318/v1/metrics";
export const OTLP_HTTP_JSON_LOGS = "http://localhost:4318/v1/logs";
export const LOCAL_DEV_API_KEY = "dev-api-key-0000";
export const REDACTED_LOCAL_API_KEY = "dev-api-key-...-0000";

export interface FirstSignalStatus {
  state: "detected" | "waiting" | "error";
  traces: number;
  logs: number;
  metrics: number;
}

export async function getFirstSignalStatus(tenantId: string): Promise<FirstSignalStatus> {
  const [traces, logs, metrics] = await Promise.allSettled([
    searchTraces(tenantId, {
      from: new Date(Date.now() - 60 * 60 * 1000).toISOString(),
      to: new Date().toISOString(),
      limit: 1,
    }),
    searchLogs(tenantId, { from: new Date(Date.now() - 60 * 60 * 1000).toISOString(), limit: 1 }),
    listMetrics(tenantId),
  ]);

  if (traces.status === "rejected" || logs.status === "rejected" || metrics.status === "rejected") {
    return { state: "error", traces: 0, logs: 0, metrics: 0 };
  }

  const traceCount = traces.value.total ?? traces.value.traces.length;
  const logCount = logs.value.total ?? logs.value.logs.length;
  const metricCount = metrics.value.metrics.length;
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

export async function getConfig(tenantId: string): Promise<PlatformConfig> {
  const res = await fetch("/v1/config", {
    headers: {
      "x-api-key": LOCAL_DEV_API_KEY,
      "X-Tenant-ID": tenantId,
    },
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
export async function saveLlmConfig(tenantId: string, params: SaveLlmConfigParams): Promise<void> {
  const body: Record<string, string> = {};
  if (params.apiKey !== undefined) body.api_key = params.apiKey;
  if (params.url !== undefined) body.url = params.url;
  if (params.model !== undefined) body.model = params.model;

  const res = await fetch("/v1/config/llm", {
    method: "PUT",
    headers: {
      "Content-Type": "application/json",
      "x-api-key": LOCAL_DEV_API_KEY,
      "X-Tenant-ID": tenantId,
    },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`saveLlmConfig failed: ${res.status}`);
}

/** Legacy alias — kept for backwards compatibility. */
export async function saveLlmKey(tenantId: string, key: string): Promise<void> {
  return saveLlmConfig(tenantId, { apiKey: key });
}

// ── LLM model listing / connectivity probe ────────────────────────────────────

export interface LlmModelsResult {
  ok: boolean;
  models: string[];
  error?: string;
}

/**
 * POST /v1/config/llm/models — verifies LLM connectivity and returns the list
 * of available model IDs. Accepts optional url and apiKey to test pre-save
 * credentials; falls back to stored DB/env config when omitted.
 *
 * Always resolves (never rejects) — callers inspect `ok`.
 */
export async function fetchAvailableModels(
  tenantId: string,
  url?: string,
  apiKey?: string,
): Promise<LlmModelsResult> {
  try {
    const body: Record<string, string> = {};
    if (url) body.url = url;
    if (apiKey) body.api_key = apiKey;

    const res = await fetch("/v1/config/llm/models", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "x-api-key": LOCAL_DEV_API_KEY,
        "X-Tenant-ID": tenantId,
      },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      return { ok: false, models: [], error: `Server error: ${res.status}` };
    }
    return res.json() as Promise<LlmModelsResult>;
  } catch (err) {
    return {
      ok: false,
      models: [],
      error: err instanceof Error ? err.message : "Network error",
    };
  }
}
