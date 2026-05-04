import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Panel } from "../components/ui/panel";
import { Select, SelectOption } from "../components/ui/select";
import {
  fetchAvailableModels,
  getConfig,
  getFirstSignalStatus,
  OTLP_GRPC_ENDPOINT,
  OTLP_HTTP_JSON_LOGS,
  OTLP_HTTP_JSON_METRICS,
  OTLP_HTTP_JSON_TRACES,
  saveLlmConfig,
  type LlmModelsResult,
} from "../api/setup";
import { createToken, deleteToken, listTokens, renewToken, restoreToken, revokeToken, type TokenRecord } from "../api/tokens";
import { TOKENS_QUERY_KEY } from "../hooks/useTokens";

export default function SetupPage() {
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
              <dt>OTLP gRPC Ingestion URL</dt>
              <dd><code className="text-xs">{OTLP_GRPC_ENDPOINT}</code></dd>
            </div>
            <div>
              <dt>OTLP HTTP/JSON Traces</dt>
              <dd><code className="text-xs">{OTLP_HTTP_JSON_TRACES}</code></dd>
            </div>
            <div>
              <dt>OTLP HTTP/JSON Metrics</dt>
              <dd><code className="text-xs">{OTLP_HTTP_JSON_METRICS}</code></dd>
            </div>
            <div>
              <dt>OTLP HTTP/JSON Logs</dt>
              <dd><code className="text-xs">{OTLP_HTTP_JSON_LOGS}</code></dd>
            </div>
          </dl>
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
      <IngressTokensPanel />
    </section>
  );
}

// ── LLM config panel ───────────────────────────────────────────────────────────

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
    } catch {
      setSaveState("error");
    }
  }

  const showModelSelect =
    modelsStatus === "loaded" && remoteModels.length > 0 && !useCustomModel;
  const modelLoading = modelsStatus === "loading";

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
  );
}

// ── ConnectivityBadge ─────────────────────────────────────────────────────────

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
      <span className="text-xs text-[var(--success-text,#22c55e)]" role="status" data-testid="llm-config-test-ok">
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

// ── IngressTokensPanel ────────────────────────────────────────────────────────

function IngressTokensPanel() {
  const qc = useQueryClient();
  const { data, isLoading } = useQuery({
    queryKey: TOKENS_QUERY_KEY,
    queryFn: listTokens,
  });

  const [showForm, setShowForm] = useState(false);
  const [name, setName] = useState("");
  const [environment, setEnvironment] = useState("");
  const [newPlaintext, setNewPlaintext] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);

  const knownEnvs = Array.from(
    new Set((data?.tokens ?? []).filter((t) => !t.revoked).map((t) => t.environment).filter(Boolean)),
  ).sort();

  const createMutation = useMutation({
    mutationFn: createToken,
    onSuccess: (res) => {
      void qc.invalidateQueries({ queryKey: TOKENS_QUERY_KEY });
      setNewPlaintext(res.plaintext);
      setShowForm(false);
      setName("");
      setEnvironment("");
      setFormError(null);
    },
    onError: () => setFormError("Failed to create token. Please try again."),
  });

  const revokeMutation = useMutation({
    mutationFn: revokeToken,
    onSuccess: () => void qc.invalidateQueries({ queryKey: TOKENS_QUERY_KEY }),
  });

  const renewMutation = useMutation({
    mutationFn: renewToken,
    onSuccess: (res) => {
      void qc.invalidateQueries({ queryKey: TOKENS_QUERY_KEY });
      setNewPlaintext(res.plaintext);
    },
  });

  const restoreMutation = useMutation({
    mutationFn: restoreToken,
    onSuccess: () => void qc.invalidateQueries({ queryKey: TOKENS_QUERY_KEY }),
  });

  const deleteMutation = useMutation({
    mutationFn: deleteToken,
    onSuccess: () => void qc.invalidateQueries({ queryKey: TOKENS_QUERY_KEY }),
  });

  function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim() || !environment.trim()) {
      setFormError("Name and environment are required.");
      return;
    }
    createMutation.mutate({ name: name.trim(), environment: environment.trim() });
  }

  function handleShowForm() {
    setShowForm(true);
    setFormError(null);
    setTimeout(() => nameRef.current?.focus(), 50);
  }

  async function copyPlaintext() {
    if (!newPlaintext) return;
    try {
      await navigator.clipboard.writeText(newPlaintext);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* ignore */
    }
  }

  return (
    <Panel eyebrow="Credentials" title="Ingestion tokens">
      <p className="mb-4 text-sm text-[var(--text-muted)]">
        Each token binds a client to a tenant and environment. The ingest gateway resolves the
        environment from the token — clients need no additional configuration.
      </p>

      {newPlaintext && (
        <div
          className="mb-4 rounded border border-[var(--border)] bg-[var(--surface-raised)] p-3"
          role="alert"
        >
          <p className="mb-1 text-xs font-semibold text-[var(--text-muted)]">
            Token value — copy it now. It will not be shown again.
          </p>
          <div className="flex items-center gap-2">
            <code className="flex-1 break-all text-xs">{newPlaintext}</code>
            <Button variant="secondary" onClick={() => void copyPlaintext()}>
              {copied ? "Copied!" : "Copy"}
            </Button>
            <Button variant="ghost" onClick={() => setNewPlaintext(null)}>
              Dismiss
            </Button>
          </div>
        </div>
      )}

      {showForm && (
        <form onSubmit={handleCreate} className="mb-4 flex flex-wrap items-end gap-2">
          <datalist id="env-list">
            {knownEnvs.map((e) => (
              <option key={e} value={e} />
            ))}
          </datalist>
          <div className="flex flex-col gap-1">
            <label className="text-xs font-medium" htmlFor="token-name">
              Name
            </label>
            <input
              id="token-name"
              ref={nameRef}
              className="select-input"
              placeholder="e.g. shop-api staging"
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoComplete="off"
            />
          </div>
          <div className="flex flex-col gap-1">
            <label className="text-xs font-medium" htmlFor="token-env">
              Environment
            </label>
            <input
              id="token-env"
              className="select-input"
              list="env-list"
              placeholder="e.g. production"
              value={environment}
              onChange={(e) => setEnvironment(e.target.value)}
              autoComplete="off"
            />
          </div>
          <Button type="submit" disabled={createMutation.isPending}>
            {createMutation.isPending ? "Creating…" : "Create token"}
          </Button>
          <Button
            type="button"
            variant="ghost"
            onClick={() => {
              setShowForm(false);
              setFormError(null);
            }}
          >
            Cancel
          </Button>
          {formError && <p className="w-full text-xs text-[var(--error)]">{formError}</p>}
        </form>
      )}

      {!showForm && (
        <Button variant="secondary" className="mb-4" onClick={handleShowForm}>
          + New token
        </Button>
      )}

      {isLoading ? (
        <p className="text-sm text-[var(--text-muted)]">Loading tokens…</p>
      ) : (
        <TokenTable
          tokens={data?.tokens ?? []}
          onRevoke={(id) => revokeMutation.mutate(id)}
          revoking={revokeMutation.isPending ? revokeMutation.variables : undefined}
          onRenew={(id) => renewMutation.mutate(id)}
          renewing={renewMutation.isPending ? renewMutation.variables : undefined}
          onRestore={(id) => restoreMutation.mutate(id)}
          restoring={restoreMutation.isPending ? restoreMutation.variables : undefined}
          onDelete={(id) => {
            if (confirm("Permanently delete this token? This cannot be undone.")) {
              deleteMutation.mutate(id);
            }
          }}
          deleting={deleteMutation.isPending ? deleteMutation.variables : undefined}
        />
      )}
    </Panel>
  );
}

interface TokenTableProps {
  tokens: TokenRecord[];
  onRevoke: (id: string) => void;
  revoking?: string;
  onRenew: (id: string) => void;
  renewing?: string;
  onRestore: (id: string) => void;
  restoring?: string;
  onDelete: (id: string) => void;
  deleting?: string;
}

function TokenTable({ tokens, onRevoke, revoking, onRenew, renewing, onRestore, restoring, onDelete, deleting }: TokenTableProps) {
  if (tokens.length === 0) {
    return <p className="text-sm text-[var(--text-muted)]">No tokens registered.</p>;
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b border-[var(--border)] text-left text-xs font-medium text-[var(--text-muted)]">
            <th className="py-2 pr-4">Name</th>
            <th className="py-2 pr-4">Tenant</th>
            <th className="py-2 pr-4">Environment</th>
            <th className="py-2 pr-4">Created</th>
            <th className="py-2 pr-4">Status</th>
            <th className="py-2" />
          </tr>
        </thead>
        <tbody>
          {tokens.map((t) => (
            <tr key={t.id} className="border-b border-[var(--border-subtle)]">
              <td className="py-2 pr-4 font-medium">{t.name}</td>
              <td className="py-2 pr-4 text-[var(--text-muted)]">{t.tenant_name}</td>
              <td className="py-2 pr-4">
                <code className="text-xs">{t.environment || <em className="text-[var(--text-muted)]">—</em>}</code>
              </td>
              <td className="py-2 pr-4 text-xs text-[var(--text-muted)]">
                {new Date(t.created_at).toLocaleDateString()}
              </td>
              <td className="py-2 pr-4">
                <Badge tone={t.revoked ? "neutral" : "good"}>
                  {t.revoked ? "Revoked" : "Active"}
                </Badge>
              </td>
              <td className="py-2">
                {!t.revoked ? (
                  <div className="flex gap-1">
                    <Button
                      variant="ghost"
                      disabled={renewing === t.id}
                      onClick={() => onRenew(t.id)}
                    >
                      {renewing === t.id ? "Renewing…" : "Renew"}
                    </Button>
                    <Button
                      variant="ghost"
                      disabled={revoking === t.id}
                      onClick={() => {
                        if (confirm(`Revoke token "${t.name}"? This cannot be undone.`)) {
                          onRevoke(t.id);
                        }
                      }}
                    >
                      {revoking === t.id ? "Revoking…" : "Revoke"}
                    </Button>
                  </div>
                ) : (
                  <div className="flex gap-1">
                    <Button
                      variant="ghost"
                      disabled={restoring === t.id}
                      onClick={() => onRestore(t.id)}
                    >
                      {restoring === t.id ? "Restoring…" : "Restore"}
                    </Button>
                    <Button
                      variant="ghost"
                      disabled={deleting === t.id}
                      onClick={() => onDelete(t.id)}
                    >
                      {deleting === t.id ? "Deleting…" : "Delete"}
                    </Button>
                  </div>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
