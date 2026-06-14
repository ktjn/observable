import { useEffect, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "../../components/ui/button";
import { Panel } from "../../components/ui/panel";
import { Badge } from "../../components/ui/badge";
import { createToken } from "../../api/tokens";
import { getFirstSignalStatus } from "../../api/setup";
import { useTenantContext } from "../../hooks/useTenantContext";
import {
  type Language,
  type WizardStep,
  readOnboardingState,
  writeOnboardingState,
} from "./onboardingState";

// ── SDK install snippets ──────────────────────────────────────────────────────

export const LANGUAGE_LABELS: Record<Language, string> = {
  nodejs: "Node.js",
  python: "Python",
  java: "Java",
  go: "Go",
  ruby: "Ruby",
  dotnet: ".NET",
  other: "Other",
};

export function getInstallCommand(language: Language): string {
  switch (language) {
    case "nodejs":
      return "npm install @opentelemetry/sdk-node @opentelemetry/auto-instrumentations-node";
    case "python":
      return "pip install opentelemetry-distro opentelemetry-exporter-otlp\nopentelemetry-bootstrap -a install";
    case "java":
      return "curl -OL https://github.com/open-telemetry/opentelemetry-java-instrumentation/releases/latest/download/opentelemetry-javaagent.jar";
    case "go":
      return "go get go.opentelemetry.io/otel go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracegrpc";
    case "ruby":
      return "gem install opentelemetry-sdk opentelemetry-exporter-otlp opentelemetry-instrumentation-all";
    case "dotnet":
      return "dotnet add package OpenTelemetry.Exporter.OpenTelemetryProtocol\ndotnet add package OpenTelemetry.Extensions.Hosting";
    case "other":
      return "# Use any OTLP-compatible SDK — point it at the endpoint below.";
  }
}

export function getRunCommand(language: Language, endpoint: string, apiKey: string): string {
  const envBlock = `OTEL_EXPORTER_OTLP_ENDPOINT=${endpoint}\nOTEL_EXPORTER_OTLP_HEADERS="x-api-key=${apiKey}"`;
  switch (language) {
    case "nodejs":
      return `${envBlock} \\\nnode -r @opentelemetry/auto-instrumentations-node/register your-app.js`;
    case "python":
      return `${envBlock} \\\nopentelemetry-instrument python your_app.py`;
    case "java":
      return `${envBlock} \\\njava -javaagent:opentelemetry-javaagent.jar -jar your-app.jar`;
    case "go":
      return `# Set these environment variables before running your Go app:\n${envBlock}`;
    case "ruby":
      return `${envBlock} \\\nbundle exec opentelemetry-instrument ruby your_app.rb`;
    case "dotnet":
      return `# In appsettings.json:\n# "Otlp": { "Endpoint": "${endpoint}", "Headers": "x-api-key=${apiKey}" }`;
    case "other":
      return `# Configure your SDK with:\n# Endpoint: ${endpoint}\n# Header:   x-api-key: ${apiKey}`;
  }
}

// ── Step 1: Language picker ───────────────────────────────────────────────────

const LANGUAGES: Language[] = ["nodejs", "python", "java", "go", "ruby", "dotnet", "other"];

interface StepLanguageProps {
  onSelect: (lang: Language) => void;
}

function StepLanguage({ onSelect }: StepLanguageProps) {
  const [selected, setSelected] = useState<Language | null>(null);

  return (
    <div>
      <p className="mb-4 text-sm text-[var(--text-muted)]">
        Choose your service's language or framework so we can show you the right install command.
      </p>
      <div className="mb-6 flex flex-wrap gap-2">
        {LANGUAGES.map((lang) => (
          <button
            key={lang}
            type="button"
            aria-pressed={selected === lang}
            className={`rounded border px-3 py-1.5 text-sm transition-colors ${
              selected === lang
                ? "border-[var(--accent)] bg-[var(--accent)] text-white"
                : "border-[var(--border)] bg-[var(--surface)] hover:border-[var(--accent)]"
            }`}
            onClick={() => setSelected(lang)}
          >
            {LANGUAGE_LABELS[lang]}
          </button>
        ))}
      </div>
      <Button disabled={!selected} onClick={() => selected && onSelect(selected)}>
        Next →
      </Button>
    </div>
  );
}

// ── Step 2: API key generation ────────────────────────────────────────────────

const OTLP_HTTP_ENDPOINT = "http://localhost:4318";

interface StepApiKeyProps {
  language: Language;
  tenantId: string;
  onKeyReady: (tokenId: string, plaintext: string) => void;
  onNext: () => void;
}

function StepApiKey({ language, tenantId, onKeyReady, onNext }: StepApiKeyProps) {
  const qc = useQueryClient();
  const [envName, setEnvName] = useState("production");
  const [formError, setFormError] = useState<string | null>(null);
  const [plaintext, setPlaintext] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const createMutation = useMutation({
    mutationFn: () =>
      createToken(tenantId, { name: `onboarding-${language}`, environment: envName.trim() }),
    onSuccess: (res) => {
      void qc.invalidateQueries({ queryKey: ["tokens", tenantId] });
      setPlaintext(res.plaintext);
      onKeyReady(res.id, res.plaintext);
    },
    onError: () => setFormError("Failed to create API key. Please try again."),
  });

  async function copyText(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* ignore */
    }
  }

  const installCmd = getInstallCommand(language);
  const runCmd = plaintext
    ? getRunCommand(language, OTLP_HTTP_ENDPOINT, plaintext)
    : getRunCommand(language, OTLP_HTTP_ENDPOINT, "<your-api-key>");

  return (
    <div>
      <p className="mb-4 text-sm text-[var(--text-muted)]">
        Create an API key that binds your service to this tenant. The key will be shown once — copy it now.
      </p>

      {!plaintext ? (
        <div className="mb-4 flex flex-wrap items-end gap-2">
          <div className="flex flex-col gap-1">
            <label className="text-xs font-medium" htmlFor="onboarding-env">
              Environment
            </label>
            <input
              id="onboarding-env"
              className="select-input"
              value={envName}
              onChange={(e) => setEnvName(e.target.value)}
              placeholder="production"
            />
          </div>
          <Button
            onClick={() => {
              if (!envName.trim()) { setFormError("Environment is required."); return; }
              setFormError(null);
              createMutation.mutate();
            }}
            disabled={createMutation.isPending}
          >
            {createMutation.isPending ? "Creating…" : "Create API key"}
          </Button>
          {formError && <p className="w-full text-xs text-[var(--error)]">{formError}</p>}
        </div>
      ) : (
        <div className="mb-4 rounded border border-[var(--border)] bg-[var(--surface-raised)] p-3" role="alert">
          <p className="mb-1 text-xs font-semibold text-[var(--text-muted)]">
            Your API key — copy it now. It will not be shown again.
          </p>
          <div className="flex items-center gap-2">
            <code className="flex-1 break-all text-xs">{plaintext}</code>
            <Button variant="secondary" onClick={() => void copyText(plaintext)}>
              {copied ? "Copied!" : "Copy"}
            </Button>
          </div>
        </div>
      )}

      <div className="mb-4">
        <p className="mb-1 text-xs font-semibold text-[var(--text-muted)]">1. Install the SDK</p>
        <div className="relative rounded border border-[var(--border)] bg-[var(--surface-raised)] p-3">
          <pre className="overflow-x-auto text-xs">{installCmd}</pre>
          <button
            type="button"
            aria-label="Copy install command"
            className="absolute right-2 top-2 text-xs text-[var(--accent)] hover:underline"
            onClick={() => void copyText(installCmd)}
          >
            Copy
          </button>
        </div>
      </div>

      <div className="mb-6">
        <p className="mb-1 text-xs font-semibold text-[var(--text-muted)]">
          2. Configure and run your service
        </p>
        <div className="relative rounded border border-[var(--border)] bg-[var(--surface-raised)] p-3">
          <pre className="overflow-x-auto text-xs">{runCmd}</pre>
          <button
            type="button"
            aria-label="Copy run command"
            className="absolute right-2 top-2 text-xs text-[var(--accent)] hover:underline"
            onClick={() => void copyText(runCmd)}
          >
            Copy
          </button>
        </div>
      </div>

      <Button disabled={!plaintext} onClick={onNext}>
        My SDK is configured →
      </Button>
    </div>
  );
}

// ── Step 3: Waiting for first signal ─────────────────────────────────────────

interface StepWaitingProps {
  tenantId: string;
  plaintext?: string | null;
  onDetected: (counts: { traces: number; logs: number; metrics: number }) => void;
}

function StepWaiting({ tenantId, plaintext, onDetected }: StepWaitingProps) {
  const { data } = useQuery({
    queryKey: ["onboarding-signal", tenantId],
    queryFn: () => getFirstSignalStatus(tenantId),
    refetchInterval: 3000,
  });

  useEffect(() => {
    if (data?.state === "detected") {
      onDetected({ traces: data.traces, logs: data.logs, metrics: data.metrics });
    }
  }, [data, onDetected]);

  return (
    <div className="flex flex-col items-center gap-4 py-8 text-center">
      {plaintext && (
        <div className="w-full rounded border border-[var(--border)] bg-[var(--surface-raised)] p-3 text-left" role="alert">
          <p className="mb-1 text-xs font-semibold text-[var(--text-muted)]">
            Your API key — copy it now. It will not be shown again.
          </p>
          <code className="block break-all text-xs">{plaintext}</code>
        </div>
      )}
      <div
        className="h-8 w-8 animate-pulse rounded-full bg-[var(--accent)]"
        aria-label="Waiting for signal"
        role="status"
      />
      <p className="text-sm text-[var(--text-muted)]">
        Waiting for your first telemetry signal…
      </p>
      <p className="text-xs text-[var(--text-muted)]">
        Start your instrumented service and send a request. We'll detect it automatically.
      </p>
    </div>
  );
}

// ── Step 4: Success ───────────────────────────────────────────────────────────

interface StepDoneProps {
  counts: { traces: number; logs: number; metrics: number };
  onFinish: () => void;
}

function StepDone({ counts, onFinish }: StepDoneProps) {
  return (
    <div className="flex flex-col items-center gap-4 py-8 text-center">
      <div className="text-4xl" aria-hidden="true">🎉</div>
      <h2 className="text-lg font-semibold">Your first signal arrived!</h2>
      <div className="flex gap-4">
        {counts.traces > 0 && (
          <Badge tone="good">{counts.traces} trace{counts.traces !== 1 ? "s" : ""}</Badge>
        )}
        {counts.logs > 0 && (
          <Badge tone="good">{counts.logs} log{counts.logs !== 1 ? "s" : ""}</Badge>
        )}
        {counts.metrics > 0 && (
          <Badge tone="good">{counts.metrics} metric{counts.metrics !== 1 ? "s" : ""}</Badge>
        )}
      </div>
      <Button onClick={onFinish}>Go to Services →</Button>
    </div>
  );
}

// ── Main wizard ───────────────────────────────────────────────────────────────

const STEP_LABELS: Record<WizardStep, string> = {
  language: "Choose language",
  apikey: "Get API key",
  waiting: "Send data",
  done: "Done",
};

const STEP_ORDER: WizardStep[] = ["language", "apikey", "waiting", "done"];

export function OnboardingWizard() {
  const { tenantId } = useTenantContext();
  const navigate = useNavigate();

  const [step, setStep] = useState<WizardStep>(() => readOnboardingState().step);
  const [language, setLanguage] = useState<Language | null>(() => readOnboardingState().language);
  const [tokenPlaintext, setTokenPlaintext] = useState<string | null>(null);
  const [signalCounts, setSignalCounts] = useState<{ traces: number; logs: number; metrics: number } | null>(null);

  function goTo(s: WizardStep) {
    setStep(s);
    writeOnboardingState({ step: s });
  }

  function handleLanguageSelected(lang: Language) {
    setLanguage(lang);
    writeOnboardingState({ language: lang });
    goTo("apikey");
  }

  function handleKeyReady(tokenId: string, plaintext: string) {
    writeOnboardingState({ tokenId });
    setTokenPlaintext(plaintext);
    goTo("waiting");
  }

  function handleDetected(counts: { traces: number; logs: number; metrics: number }) {
    setSignalCounts(counts);
    goTo("done");
  }

  function handleFinish() {
    writeOnboardingState({ complete: true });
    void navigate({ to: "/services" });
  }

  function handleDismiss() {
    writeOnboardingState({ complete: true });
    void navigate({ to: "/" });
  }

  const currentIndex = STEP_ORDER.indexOf(step);

  return (
    <section className="page-stack" aria-labelledby="onboarding-heading">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Onboarding</div>
          <h1 id="onboarding-heading">Getting Started</h1>
        </div>
        <Button variant="ghost" onClick={handleDismiss}>
          Skip wizard
        </Button>
      </div>

      {/* Progress bar */}
      <div className="mb-6 flex gap-2" aria-label="Wizard progress">
        {STEP_ORDER.map((s, i) => (
          <div key={s} className="flex items-center gap-2">
            <div
              className={`flex h-6 w-6 items-center justify-center rounded-full text-xs font-semibold ${
                i < currentIndex
                  ? "bg-[var(--accent)] text-white"
                  : i === currentIndex
                    ? "border-2 border-[var(--accent)] text-[var(--accent)]"
                    : "border border-[var(--border)] text-[var(--text-muted)]"
              }`}
              aria-current={i === currentIndex ? "step" : undefined}
            >
              {i < currentIndex ? "✓" : i + 1}
            </div>
            <span
              className={`text-xs ${
                i === currentIndex ? "font-semibold" : "text-[var(--text-muted)]"
              }`}
            >
              {STEP_LABELS[s]}
            </span>
            {i < STEP_ORDER.length - 1 && (
              <div className="mx-1 h-px w-6 bg-[var(--border)]" aria-hidden="true" />
            )}
          </div>
        ))}
      </div>

      <Panel eyebrow={`Step ${currentIndex + 1} of ${STEP_ORDER.length}`} title={STEP_LABELS[step]}>
        {step === "language" && (
          <StepLanguage onSelect={handleLanguageSelected} />
        )}
        {step === "apikey" && language && (
          <StepApiKey
            language={language}
            tenantId={tenantId}
            onKeyReady={handleKeyReady}
            onNext={() => goTo("waiting")}
          />
        )}
        {step === "waiting" && (
          <StepWaiting tenantId={tenantId} plaintext={tokenPlaintext} onDetected={handleDetected} />
        )}
        {step === "done" && signalCounts && (
          <StepDone counts={signalCounts} onFinish={handleFinish} />
        )}
      </Panel>
    </section>
  );
}
