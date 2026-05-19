import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Panel } from "../components/ui/panel";
import { Select, SelectOption } from "../components/ui/select";
import {
  fetchAvailableModels,
  getConfig,
  saveLlmConfig,
  type LlmModelsResult,
} from "../api/setup";
import { useTenantContext } from "../hooks/useTenantContext";

export default function SetupLlmPage() {
  const { tenantId } = useTenantContext();
  const { data: config, refetch: refetchConfig } = useQuery({
    queryKey: ["setup", "config", tenantId],
    queryFn: () => getConfig(tenantId),
  });

  const [apiKey, setApiKey] = useState("");
  // null = not yet initialised from config; "" = user explicitly cleared the field.
  const [url, setUrl] = useState<string | null>(null);
  const [model, setModel] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle");

  // Model discovery state
  const [remoteModels, setRemoteModels] = useState<string[]>([]);
  const [modelsStatus, setModelsStatus] = useState<"idle" | "loading" | "loaded" | "error">("idle");
  const [modelsError, setModelsError] = useState("");
  const [useCustomModel, setUseCustomModel] = useState(false);

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

  async function handleTestConnection() {
    setModelsStatus("loading");
    setModelsError("");
    const result: LlmModelsResult = await fetchAvailableModels(
      tenantId,
      urlValue.trim() || undefined,
      apiKey.trim() || undefined,
    );
    if (result.ok) {
      setRemoteModels(result.models);
      setModelsStatus("loaded");
      // If the currently selected model isn't in the fetched list, keep custom mode.
      if (result.models.length > 0 && modelValue && !result.models.includes(modelValue)) {
        setUseCustomModel(true);
      } else {
        setUseCustomModel(false);
      }
    } else {
      setModelsStatus("error");
      setModelsError(result.error ?? "Connection test failed");
    }
  }

  async function handleSave(e: React.FormEvent) {
    e.preventDefault();
    setSaveState("saving");
    try {
      await saveLlmConfig(tenantId, {
        ...(apiKey.trim() && { apiKey: apiKey.trim() }),
        // Only include url/model once the server config has been loaded to avoid
        // overwriting persisted values with empty strings on a save-before-load race.
        ...(url !== null && { url: urlValue.trim() }),
        ...(model !== null && { model: modelValue.trim() }),
      });
      setApiKey("");
      setSaveState("saved");
      void refetchConfig();
    } catch {
      setSaveState("error");
    }
  }

  const showModelSelect =
    modelsStatus === "loaded" && remoteModels.length > 0 && !useCustomModel;
  const modelLoading = modelsStatus === "loading";

  return (
    <section className="page-stack" aria-labelledby="setup-llm-heading">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Setup</div>
          <h1 id="setup-llm-heading">LLM configuration</h1>
        </div>
      </div>

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
          <div className="flex flex-col gap-1">
            <span className="text-xs font-semibold uppercase text-[var(--muted)]">
              {modelLoading
                ? "Model (loading…)"
                : modelsStatus === "loaded" && remoteModels.length > 0 && !useCustomModel
                  ? `Model (${remoteModels.length} available)`
                  : "Model"}
            </span>
            {showModelSelect ? (
              <Select
                value={modelValue}
                onChange={(e) => {
                  if (e.target.value === CUSTOM_MODEL_SENTINEL) {
                    setUseCustomModel(true);
                  } else {
                    setModel(e.target.value);
                  }
                }}
                aria-label="LLM model"
                data-testid="llm-model-select"
              >
                <SelectOption value="">Select a model…</SelectOption>
                {remoteModels.map((m) => (
                  <SelectOption key={m} value={m}>
                    {m}
                  </SelectOption>
                ))}
                <SelectOption value={CUSTOM_MODEL_SENTINEL}>— enter custom model ID —</SelectOption>
              </Select>
            ) : (
              <input
                type="text"
                list="llm-model-suggestions"
                value={modelValue}
                onChange={(e) => setModel(e.target.value)}
                placeholder="gpt-4o-mini"
                disabled={modelLoading}
                aria-label="LLM model"
                className="rounded border border-[var(--border)] bg-[var(--bg-input)] px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-[var(--brand)] disabled:opacity-50"
                data-testid="llm-model-input"
              />
            )}
            <datalist id="llm-model-suggestions">
              {KNOWN_MODELS.map((m) => (
                <option key={m} value={m} />
              ))}
            </datalist>
            {useCustomModel && remoteModels.length > 0 && (
              <button
                type="button"
                className="self-start text-xs text-[var(--brand)] hover:underline"
                onClick={() => setUseCustomModel(false)}
                data-testid="llm-model-pick-from-list"
              >
                ← pick from list
              </button>
            )}
          </div>
          <div className="flex flex-wrap gap-2 items-center">
            <Button
              variant="secondary"
              type="submit"
              disabled={saveState === "saving" || config === undefined}
              data-testid="llm-config-save"
            >
              {saveState === "saving" ? "Saving…" : "Save"}
            </Button>
            {urlValue.trim() && (
              <Button
                variant="secondary"
                type="button"
                disabled={modelsStatus === "loading"}
                onClick={() => void handleTestConnection()}
                data-testid="llm-config-test"
              >
                {modelsStatus === "loading" ? "Testing…" : "Test connection"}
              </Button>
            )}
            <ConnectivityBadge
              saveState={saveState}
              modelsStatus={modelsStatus}
              modelsError={modelsError}
              remoteModels={remoteModels}
            />
          </div>
        </form>
        <p className="mt-3 text-xs text-[var(--text-muted)]">
          API key is obfuscated in PostgreSQL. Suitable for local development only.
        </p>
      </Panel>
    </section>
  );
}

/** Known model identifiers shown in the datalist suggestion list (fallback when no remote models). */
const KNOWN_MODELS = [
  "gpt-4o-mini",
  "gpt-4o",
  "gpt-4.1",
  "microsoft/Phi-3-mini-4k-instruct",
  "meta-llama/Meta-Llama-3-8B-Instruct",
];

/** Sentinel value in the model <select> that switches the user to free-text entry. */
const CUSTOM_MODEL_SENTINEL = "__custom__";

interface ConnectivityBadgeProps {
  saveState: "idle" | "saving" | "saved" | "error";
  modelsStatus: "idle" | "loading" | "loaded" | "error";
  modelsError: string;
  remoteModels: string[];
}

function ConnectivityBadge({ saveState, modelsStatus, modelsError, remoteModels }: ConnectivityBadgeProps) {
  if (saveState === "error") {
    return (
      <span className="text-xs text-[var(--danger-text)]" role="alert" data-testid="llm-config-error">
        Failed to save. Check the console for details.
      </span>
    );
  }
  if (modelsStatus === "loaded") {
    const modelCount = remoteModels.length;
    return (
      <span className="text-xs text-[var(--good)]" role="status" data-testid="llm-config-test-ok">
        {saveState === "saved" ? "Saved. " : ""}✓ Connected
        {modelCount > 0 ? ` (${modelCount} model${modelCount === 1 ? "" : "s"})` : " (no models listed)"}
      </span>
    );
  }
  if (modelsStatus === "error") {
    return (
      <span className="text-xs text-[var(--danger-text)]" role="alert" data-testid="llm-config-test-failed">
        {saveState === "saved" ? "Saved. " : ""}⚠ Connection failed: {modelsError}
      </span>
    );
  }
  if (saveState === "saved") {
    return (
      <span className="text-xs text-[var(--text-muted)]" role="status" data-testid="llm-config-saved">
        Saved.
      </span>
    );
  }
  return null;
}
