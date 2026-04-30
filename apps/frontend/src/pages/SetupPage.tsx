import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
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
  testLlmConfig,
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
  // null = not yet initialised from config; "" = user explicitly cleared the field.
  const [url, setUrl] = useState<string | null>(null);
  const [model, setModel] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [testState, setTestState] = useState<"idle" | "testing" | "ok" | "failed">("idle");
  const [testError, setTestError] = useState("");
  const [testModel, setTestModel] = useState("");

  // Pre-fill url and model from server config on first load.
  // API key is never echoed back — only the placeholder reflects configured status.
  useEffect(() => {
    if (config) {
      if (url === null) setUrl(config.llm_url ?? "");
      if (model === null) setModel(config.llm_model ?? "");
    }
  }, [config, url, model]);

  const configured = config?.llm_key_configured ?? false;
  const urlValue = url ?? "";
  const modelValue = model ?? "";

  async function runTest() {
    setTestState("testing");
    try {
      const result = await testLlmConfig();
      if (result.ok) {
        setTestState("ok");
        setTestModel(result.model);
      } else {
        setTestState("failed");
        setTestError(result.error ?? "Connection test failed");
        setTestModel(result.model);
      }
    } catch (err) {
      setTestState("failed");
      setTestError(err instanceof Error ? err.message : "Connection test failed");
      setTestModel("");
    }
  }

  async function handleSave(e: React.FormEvent) {
    e.preventDefault();
    setSaveState("saving");
    setTestState("idle");
    try {
      await saveLlmConfig({
        ...(apiKey.trim() && { apiKey: apiKey.trim() }),
        // Only include url/model once the server config has been loaded to avoid
        // overwriting persisted values with empty strings on a save-before-load race.
        ...(url !== null && { url: urlValue.trim() }),
        ...(model !== null && { model: modelValue.trim() }),
      });
      setApiKey("");
      setSaveState("saved");
      void refetchConfig();
      // Auto-test connectivity immediately after saving.
      await runTest();
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
        <div className="flex flex-wrap gap-2 items-center">
          <Button
            variant="secondary"
            type="submit"
            disabled={saveState === "saving" || testState === "testing" || config === undefined}
            data-testid="llm-config-save"
          >
            {saveState === "saving"
              ? "Saving…"
              : testState === "testing"
                ? "Testing…"
                : "Save"}
          </Button>
          {configured && saveState === "idle" && testState === "idle" && (
            <Button
              variant="secondary"
              type="button"
              onClick={() => void runTest()}
              data-testid="llm-config-test"
            >
              Test connection
            </Button>
          )}
          <ConnectivityBadge
            saveState={saveState}
            testState={testState}
            testError={testError}
            testModel={testModel}
          />
        </div>
      </form>
      <p className="mt-3 text-xs text-[var(--text-muted)]">
        API key is obfuscated in PostgreSQL. Suitable for local development only.
      </p>
    </Panel>
  );
}

// ── ConnectivityBadge ─────────────────────────────────────────────────────────

interface ConnectivityBadgeProps {
  saveState: "idle" | "saving" | "saved" | "error";
  testState: "idle" | "testing" | "ok" | "failed";
  testError: string;
  testModel: string;
}

function ConnectivityBadge({ saveState, testState, testError, testModel }: ConnectivityBadgeProps) {
  if (saveState === "error") {
    return (
      <span className="text-xs text-[var(--danger-text)]" role="alert" data-testid="llm-config-error">
        Failed to save. Check the console for details.
      </span>
    );
  }
  if (testState === "ok") {
    return (
      <span className="text-xs text-[var(--success-text,#22c55e)]" role="status" data-testid="llm-config-test-ok">
        {saveState === "saved" ? "Saved. " : ""}✓ Connected
        {testModel ? ` (${testModel})` : ""}
      </span>
    );
  }
  if (testState === "failed") {
    return (
      <span className="text-xs text-[var(--danger-text)]" role="alert" data-testid="llm-config-test-failed">
        {saveState === "saved" ? "Saved. " : ""}⚠ Connection failed: {testError}
      </span>
    );
  }
  if (saveState === "saved" && testState === "idle") {
    return (
      <span className="text-xs text-[var(--text-muted)]" role="status" data-testid="llm-config-saved">
        Saved.
      </span>
    );
  }
  return null;
}
