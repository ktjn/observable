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
  try {
    const res = await fetch("/v1/setup/status", {
      credentials: "include",
      headers: {
        "x-api-key": LOCAL_DEV_API_KEY,
        "X-Tenant-ID": tenantId,
      },
    });
    if (!res.ok) {
      return { state: "error", traces: 0, logs: 0, metrics: 0 };
    }
    return (await res.json()) as FirstSignalStatus;
  } catch {
    return { state: "error", traces: 0, logs: 0, metrics: 0 };
  }
}

// ── LLM / AI config ───────────────────────────────────────────────────────────

export interface PlatformConfig {
  llm_key_configured: boolean;
  /** LLM endpoint URL stored in DB; null if not set. */
  llm_url: string | null;
  /** LLM model identifier stored in DB; null if not set. */
  llm_model: string | null;
  /** "remote" | "webllm" — always present, defaults to "remote". */
  llm_provider: "remote" | "webllm";
  /** WebLLM model identifier; null if not set. Separate from `llm_model`. */
  webllm_model: string | null;
}

export async function getConfig(tenantId: string): Promise<PlatformConfig> {
  const res = await fetch("/v1/config", {
    credentials: "include",
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
  provider?: "remote" | "webllm";
  webllmModel?: string;
}

/** PUT /v1/config/llm — upserts whichever of apiKey, url, model, provider, webllmModel are provided. */
export async function saveLlmConfig(tenantId: string, params: SaveLlmConfigParams): Promise<void> {
  const body: Record<string, string> = {};
  if (params.apiKey !== undefined) body.api_key = params.apiKey;
  if (params.url !== undefined) body.url = params.url;
  if (params.model !== undefined) body.model = params.model;
  if (params.provider !== undefined) body.provider = params.provider;
  if (params.webllmModel !== undefined) body.webllm_model = params.webllmModel;

  const res = await fetch("/v1/config/llm", {
    credentials: "include",
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
      credentials: "include",
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
