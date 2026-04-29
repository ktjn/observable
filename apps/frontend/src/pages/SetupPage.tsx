import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Panel } from "../components/ui/panel";
import {
  getConfig,
  getFirstSignalStatus,
  LOCAL_DEV_API_KEY,
  LOCAL_DEV_TENANT,
  LOCAL_DEV_TENANT_ID,
  OTLP_HTTP_TRACE_ENDPOINT,
  REDACTED_LOCAL_API_KEY,
  saveLlmConfig,
} from "../api/setup";

export default function SetupPage() {
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");
  const { data, isLoading, refetch } = useQuery({
    queryKey: ["setup", "first-signal"],
    queryFn: getFirstSignalStatus,
  });

  const statusText = isLoading
    ? "Checking telemetry"
    : data?.state === "detected"
      ? "First signal detected"
      : data?.state === "error"
        ? "First signal check failed"
        : "Waiting for first signal";

  async function copyApiKey() {
    try {
      await navigator.clipboard.writeText(LOCAL_DEV_API_KEY);
      setCopyState("copied");
    } catch {
      setCopyState("failed");
    }
  }

  return (
    <section className="page-stack" aria-labelledby="setup-heading">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Onboarding</div>
          <h1 id="setup-heading">Setup</h1>
        </div>
        <Button variant="secondary" onClick={() => void refetch()}>
          Recheck
        </Button>
      </div>

      <div className="detail-grid">
        <Panel eyebrow="Local ingest" title="Collector endpoint">
          <dl className="definition-grid">
            <div>
              <dt>Tenant</dt>
              <dd>{LOCAL_DEV_TENANT}</dd>
            </div>
            <div>
              <dt>Tenant ID</dt>
              <dd>{LOCAL_DEV_TENANT_ID}</dd>
            </div>
            <div>
              <dt>OTLP HTTP traces</dt>
              <dd>{OTLP_HTTP_TRACE_ENDPOINT}</dd>
            </div>
            <div>
              <dt>API key</dt>
              <dd>{REDACTED_LOCAL_API_KEY}</dd>
            </div>
          </dl>
          <div className="setup-actions">
            <Button variant="secondary" onClick={() => void copyApiKey()}>
              Copy API key
            </Button>
            <span className="text-xs font-bold uppercase text-[var(--muted)]" role="status">
              {copyState === "copied"
                ? "Copied"
                : copyState === "failed"
                  ? "Copy unavailable"
                : "Redacted in the UI"}
            </span>
          </div>
        </Panel>

        <Panel
          eyebrow="Validation"
          title="First signal"
          actions={
            <Badge tone={data?.state === "detected" ? "good" : "warn"}>
              {statusText}
            </Badge>
          }
        >
          <dl className="definition-grid">
            <div>
              <dt>Traces</dt>
              <dd>{data?.traces ?? 0}</dd>
            </div>
            <div>
              <dt>Logs</dt>
              <dd>{data?.logs ?? 0}</dd>
            </div>
            <div>
              <dt>Metrics</dt>
              <dd>{data?.metrics ?? 0}</dd>
            </div>
          </dl>
        </Panel>
      </div>

      <LlmConfigPanel />
    </section>
  );
}

// ── LLM config panel ───────────────────────────────────────────────────────────

/** Known model identifiers shown in the datalist suggestion list. */
const KNOWN_MODELS = [
  "gpt-4o-mini",
  "gpt-4o",
  "gpt-4.1",
  "microsoft/Phi-3-mini-4k-instruct",
  "meta-llama/Meta-Llama-3-8B-Instruct",
];

function LlmConfigPanel() {
  const { data: config, refetch: refetchConfig } = useQuery({
    queryKey: ["setup", "config"],
    queryFn: getConfig,
  });

  const [apiKey, setApiKey] = useState("");
  const [url, setUrl] = useState<string>("");
  const [model, setModel] = useState<string>("");
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle");

  // Pre-fill url and model from server config on first load.
  // (API key is never echoed back — only the placeholder reflects configured status.)
  const urlValue = url !== "" ? url : (config?.llm_url ?? "");
  const modelValue = model !== "" ? model : (config?.llm_model ?? "");

  const configured = config?.llm_key_configured ?? false;

  async function handleSave(e: React.FormEvent) {
    e.preventDefault();
    setSaveState("saving");
    try {
      await saveLlmConfig({
        ...(apiKey.trim() && { apiKey: apiKey.trim() }),
        url: urlValue.trim(),
        model: modelValue.trim(),
      });
      setApiKey("");
      setSaveState("saved");
      void refetchConfig();
    } catch {
      setSaveState("error");
    }
  }

  return (
    <Panel
      eyebrow="AI / NLQ"
      title="LLM configuration"
      actions={
        <Badge tone={configured ? "good" : "warn"}>
          {configured ? "Configured" : "Not configured"}
        </Badge>
      }
    >
      <p className="text-sm text-[var(--text-muted)] mb-3">
        Required for the Natural Language Query panel. Compatible with any OpenAI-format provider
        — OpenAI, Azure OpenAI, vLLM, Ollama, and others.
      </p>
      <form onSubmit={handleSave} className="flex flex-col gap-3">
        <label className="flex flex-col gap-1">
          <span className="text-xs font-semibold uppercase text-[var(--muted)]">API Key</span>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={configured ? "Enter new key to replace…" : "sk-… (leave blank for vLLM / no-auth endpoints)"}
            aria-label="LLM API key"
            className="rounded border border-[var(--border)] bg-[var(--bg-input)] px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-[var(--brand)]"
            data-testid="llm-key-input"
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs font-semibold uppercase text-[var(--muted)]">Endpoint URL</span>
          <input
            type="url"
            value={urlValue}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://api.openai.com/v1 (blank = OpenAI default)"
            aria-label="LLM endpoint URL"
            className="rounded border border-[var(--border)] bg-[var(--bg-input)] px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-[var(--brand)]"
            data-testid="llm-url-input"
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="text-xs font-semibold uppercase text-[var(--muted)]">Model</span>
          <input
            type="text"
            list="llm-model-suggestions"
            value={modelValue}
            onChange={(e) => setModel(e.target.value)}
            placeholder="gpt-4o-mini"
            aria-label="LLM model"
            className="rounded border border-[var(--border)] bg-[var(--bg-input)] px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-[var(--brand)]"
            data-testid="llm-model-input"
          />
          <datalist id="llm-model-suggestions">
            {KNOWN_MODELS.map((m) => (
              <option key={m} value={m} />
            ))}
          </datalist>
        </label>
        <div className="flex gap-2 items-center">
          <Button
            variant="secondary"
            type="submit"
            disabled={saveState === "saving"}
            data-testid="llm-config-save"
          >
            {saveState === "saving" ? "Saving…" : "Save"}
          </Button>
          {saveState === "saved" && (
            <span className="text-xs text-[var(--text-muted)]" role="status" data-testid="llm-config-saved">
              Saved. The NLQ panel will use it on the next request.
            </span>
          )}
          {saveState === "error" && (
            <span className="text-xs text-[var(--danger-text)]" role="alert" data-testid="llm-config-error">
              Failed to save. Check the console for details.
            </span>
          )}
        </div>
      </form>
      <p className="mt-3 text-xs text-[var(--text-muted)]">
        API key is obfuscated in PostgreSQL. Suitable for local development only.
      </p>
    </Panel>
  );
}
