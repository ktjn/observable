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
  saveLlmKey,
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

      <LlmKeyPanel />
    </section>
  );
}

// ── LLM key panel ─────────────────────────────────────────────────────────────

function LlmKeyPanel() {
  const { data: config, refetch: refetchConfig } = useQuery({
    queryKey: ["setup", "config"],
    queryFn: getConfig,
  });

  const [keyInput, setKeyInput] = useState("");
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle");

  async function handleSave(e: React.FormEvent) {
    e.preventDefault();
    if (!keyInput.trim()) return;
    setSaveState("saving");
    try {
      await saveLlmKey(keyInput.trim());
      setKeyInput("");
      setSaveState("saved");
      void refetchConfig();
    } catch {
      setSaveState("error");
    }
  }

  const configured = config?.llm_key_configured ?? false;

  return (
    <Panel
      eyebrow="AI / NLQ"
      title="LLM API key"
      actions={
        <Badge tone={configured ? "good" : "warn"}>
          {configured ? "Configured" : "Not configured"}
        </Badge>
      }
    >
      <p className="text-sm text-[var(--text-muted)] mb-3">
        Required for the Natural Language Query panel on service pages.
        Compatible with any OpenAI-format provider (OpenAI, Azure OpenAI, Ollama, etc.).
        Use <code className="font-mono">OPENAI_BASE_URL</code> and{" "}
        <code className="font-mono">OPENAI_MODEL</code> env vars to customise the endpoint and model.
      </p>
      <form onSubmit={handleSave} className="flex gap-2 items-center">
        <input
          type="password"
          value={keyInput}
          onChange={(e) => setKeyInput(e.target.value)}
          placeholder={configured ? "Enter new key to replace…" : "sk-…"}
          aria-label="LLM API key"
          className="flex-1 rounded border border-[var(--border)] bg-[var(--bg-input)] px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-[var(--brand-primary)]"
          data-testid="llm-key-input"
        />
        <Button
          variant="secondary"
          type="submit"
          disabled={saveState === "saving" || !keyInput.trim()}
          data-testid="llm-key-save"
        >
          {saveState === "saving" ? "Saving…" : "Save"}
        </Button>
      </form>
      {saveState === "saved" && (
        <p className="mt-2 text-xs text-[var(--text-muted)]" role="status" data-testid="llm-key-saved">
          Key saved. The NLQ panel will use it on the next request.
        </p>
      )}
      {saveState === "error" && (
        <p className="mt-2 text-xs text-[var(--danger-text)]" role="alert" data-testid="llm-key-error">
          Failed to save key. Check the console for details.
        </p>
      )}
      <p className="mt-3 text-xs text-[var(--text-muted)]">
        ⚠ Stored in plaintext in PostgreSQL. Suitable for local development only.
      </p>
    </Panel>
  );
}
