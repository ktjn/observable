/**
 * NlqPanel — NLQ input bar + results panel (P8-S6 Step 7).
 *
 * Additive: sits alongside existing signal panels. Does not replace structured query UI.
 *
 * Advisory display contract (ADR-021):
 *   - approximation_statement always visible alongside results.
 *   - raw source_sql and full nlq_ir are behind a "Show details" disclosure.
 *   - Decline reasons are displayed directly (no error state, declines are expected control flow).
 *   - InvalidResponse (unparseable LLM output) shows reason + raw LLM text for debugging.
 */
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { NlqIr, NlqResponse, VisualizationFrame } from "../../api/nlq";
import { submitNlqQuery, prepareNlqQuery, completeNlqQuery } from "../../api/nlq";
import { getConfig } from "../../api/setup";
import { checkWebGpuSupport, getOrCreateEngine } from "../../lib/webllm/webllmEngine";
import { SignalQueryForm } from "../../components/shared/SignalQueryForm";
import { CopyButton } from "../../components/ui/copy-button";
import { VisualizationPanel } from "./VisualizationPanel";
import { useTenantContext } from "../../hooks/useTenantContext";

// Hard ceiling on repair round-trips. The backend enforces the real cap
// server-side (via the session token's repair_attempt counter) and always
// eventually returns "final", so this loop terminates on its own — this is
// only a defense-in-depth guard against a latent hang if that invariant is
// ever violated by a backend bug.
const MAX_REPAIR_ITERATIONS = 5;

const LOADING_PHASE_LABEL: Record<string, string> = {
  checking_gpu: "Checking GPU support…",
  preparing: "Preparing…",
  downloading_model: "Downloading model (first use can take a while)…",
  generating: "Thinking…",
};

interface Props {
  /** Optional service context. Passed to the backend to scope the NLQ query. */
  serviceName?: string;
  /** Placeholder text for the query input. */
  placeholder?: string;
  /** Called when an executed NLQ query returns a frame. */
  onFrameResult?: (frame: VisualizationFrame) => void;
  /** Hide inline frame rendering when another surface owns result placement. */
  suppressFrameResult?: boolean;
}

type LoadingPhase = "checking_gpu" | "preparing" | "downloading_model" | "generating";

type QueryState =
  | { status: "idle" }
  | { status: "loading"; phase?: LoadingPhase }
  | { status: "error"; message: string }
  | { status: "result"; response: NlqResponse; question: string };

export function NlqPanel({
  serviceName,
  placeholder,
  onFrameResult,
  suppressFrameResult = false,
}: Props) {
  const [question, setQuestion] = useState("");
  const [state, setState] = useState<QueryState>({ status: "idle" });
  const { tenantId } = useTenantContext();
  const { data: config } = useQuery({
    queryKey: ["setup", "config", tenantId],
    queryFn: () => getConfig(tenantId),
  });
  const provider = config?.llm_provider ?? "remote";

  function handleReset() {
    setQuestion("");
    setState({ status: "idle" });
  }

  function handleFinalResponse(response: NlqResponse, q: string) {
    if (response.type === "frame") {
      onFrameResult?.(response.frame);
    }
    setState({ status: "result", response, question: q });
  }

  async function handleWebLlmSubmit(q: string) {
    setState({ status: "loading", phase: "checking_gpu" });
    const gpuSupport = await checkWebGpuSupport();
    if (!gpuSupport.supported) {
      setState({
        status: "error",
        message: `WebLLM is configured but this browser doesn't support it: ${gpuSupport.reason}`,
      });
      return;
    }

    setState({ status: "loading", phase: "preparing" });
    const prepared = await prepareNlqQuery(tenantId, { question: q, service_name: serviceName });
    if (prepared.type === "final") {
      handleFinalResponse(prepared.response, q);
      return;
    }

    const model = config?.webllm_model;
    if (!model) {
      setState({
        status: "error",
        message: "No WebLLM model configured. Set one on the Setup page.",
      });
      return;
    }

    setState({ status: "loading", phase: "downloading_model" });
    const engine = await getOrCreateEngine(model, () => {
      // Text-only loading label is sufficient for this task; progress detail
      // (byte counts etc.) is intentionally not surfaced.
    });

    setState({ status: "loading", phase: "generating" });
    let raw = await engine.complete(prepared.system_prompt, prepared.question);
    let result = await completeNlqQuery(tenantId, prepared.session_token, raw);

    let iterations = 0;
    while (result.type === "needs_repair") {
      iterations += 1;
      if (iterations > MAX_REPAIR_ITERATIONS) {
        setState({
          status: "error",
          message: "NLQ repair loop exceeded the maximum number of attempts",
        });
        return;
      }
      setState({ status: "loading", phase: "generating" });
      raw = await engine.complete(prepared.system_prompt, result.repair_prompt);
      result = await completeNlqQuery(tenantId, prepared.session_token, raw);
    }

    handleFinalResponse(result.response, q);
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const q = question.trim();
    if (!q) return;
    try {
      if (provider === "webllm") {
        await handleWebLlmSubmit(q);
      } else {
        setState({ status: "loading" });
        const response = await submitNlqQuery(tenantId, { question: q, service_name: serviceName });
        if (response.type === "frame") {
          onFrameResult?.(response.frame);
        }
        setState({ status: "result", response, question: q });
      }
    } catch (err) {
      setState({
        status: "error",
        message: err instanceof Error ? err.message : "Query failed",
      });
    }
  }

  return (
    <section className="nlq-panel" aria-label="Natural language query">
      {/* Input bar */}
      <SignalQueryForm
        value={question}
        onChange={setQuestion}
        onSubmit={handleSubmit}
        isLoading={state.status === "loading"}
        inputLabel="Natural language query"
        formLabel="Natural language query form"
        placeholder={
          placeholder ??
          "Ask a question about your metrics\u2026 e.g. \u201cp99 latency last hour\u201d"
        }
        idleLabel="Ask"
        loadingLabel={
          state.status === "loading" && state.phase
            ? LOADING_PHASE_LABEL[state.phase]
            : "Querying…"
        }
        inputTestId="nlq-input"
        submitTestId="nlq-submit"
        onReset={handleReset}
        resetTestId="nlq-reset"
      />

      {/* Results */}
      {state.status === "error" && (
        <div
          className="mt-3 rounded border border-[var(--danger-border)] bg-[var(--danger-bg)] px-3 py-2 text-sm text-[var(--danger-text)]"
          role="alert"
          data-testid="nlq-error"
        >
          {state.message}
        </div>
      )}

      {state.status === "result" && (
        <div className="mt-4 space-y-3" data-testid="nlq-result">
          {state.response.type === "decline" ? (
            <DeclineMessage reason={state.response.reason} />
          ) : state.response.type === "invalid_response" ? (
            <InvalidResponsePanel
              question={state.question}
              reason={state.response.reason}
              rawLlmResponse={state.response.raw_llm_response}
            />
          ) : state.response.type === "capabilities" ? (
            <CapabilitiesPanel hint={state.response.hint} />
          ) : state.response.type === "ir" ? (
            <InterpretedIrPanel ir={state.response.ir} />
          ) : suppressFrameResult ? (
            <p className="text-sm text-[var(--muted)]">
              Query result is shown in the matching signal tab below.
            </p>
          ) : (
            <FrameResult response={state.response} question={state.question} />
          )}
        </div>
      )}
    </section>
  );
}

function InterpretedIrPanel({ ir }: { ir: NlqIr }) {
  return (
    <details className="text-xs" open>
      <summary className="cursor-pointer select-none text-[var(--text-muted)]">
        Interpreted IR
      </summary>
      <pre className="mt-1 overflow-x-auto rounded bg-[var(--bg-code)] p-2 text-[0.7rem]">
        {JSON.stringify(ir, null, 2)}
      </pre>
    </details>
  );
}

// ── Decline ───────────────────────────────────────────────────────────────────

function DeclineMessage({ reason }: { reason: string }) {
  return (
    <div
      className="rounded border border-[var(--border)] bg-[var(--bg-subtle)] px-4 py-3 text-sm"
      data-testid="nlq-decline"
    >
      <p className="font-medium">This question is outside the NLQ scope</p>
      <p className="mt-1 text-[var(--text-muted)]">{reason}</p>
    </div>
  );
}

// ── Invalid LLM response ──────────────────────────────────────────────────────

function InvalidResponsePanel({
  question,
  reason,
  rawLlmResponse,
}: {
  question: string;
  reason: string;
  rawLlmResponse: string;
}) {
  return (
    <div
      className="rounded border border-[var(--warn-border,var(--border))] bg-[var(--warn-bg,var(--bg-subtle))] px-4 py-3 text-sm"
      data-testid="nlq-invalid-response"
    >
      <p className="font-medium">Could not interpret the LLM response</p>
      <p className="mt-1 text-[var(--text-muted)]">
        Query: <span className="italic">{question}</span>
      </p>
      <p className="mt-1 text-[var(--text-muted)]">{reason}</p>
      <details className="mt-2">
        <summary className="cursor-pointer select-none text-[var(--text-muted)] hover:text-[var(--text-strong)]">
          Show raw LLM response
        </summary>
        <pre
          className="mt-1 overflow-x-auto rounded bg-[var(--bg-code)] p-2 text-[0.7rem]"
          data-testid="nlq-raw-llm-response"
        >
          {rawLlmResponse}
        </pre>
      </details>
    </div>
  );
}

// ── Capabilities ──────────────────────────────────────────────────────────────

function CapabilitiesPanel({ hint }: { hint: string }) {
  return (
    <div
      className="rounded border border-[var(--border)] bg-[var(--bg-subtle)] px-4 py-3 text-sm"
      data-testid="nlq-capabilities"
    >
      <p className="font-medium mb-2">Observable NLQ Capabilities</p>
      <pre className="whitespace-pre-wrap text-[var(--text-muted)] text-xs font-mono leading-relaxed">
        {hint}
      </pre>
    </div>
  );
}

// ── Frame result ──────────────────────────────────────────────────────────────

function FrameResult({
  response,
  question,
}: {
  response: Extract<NlqResponse, { type: "frame" }>;
  question: string;
}) {
  const { frame } = response;
  const [showDetails, setShowDetails] = useState(false);

  return (
    <>
      {/* Visualization auto-graphing (Step 8) */}
      <VisualizationPanel frame={frame} />

      {/* Approximation statement — always visible (ADR-021 advisory requirement) */}
      <p
        className="text-xs text-[var(--text-muted)] italic"
        data-testid="nlq-approximation"
      >
        {frame.approximation_statement}
      </p>

      {/* Provenance disclosure — raw SQL + full IR hidden by default */}
      <details
        open={showDetails}
        onToggle={(e) => setShowDetails((e.target as HTMLDetailsElement).open)}
        className="text-xs"
      >
        <summary
          className="cursor-pointer select-none text-[var(--text-muted)] hover:text-[var(--text-strong)]"
          data-testid="nlq-show-details"
        >
          {showDetails ? "Hide details" : "Show query details"}
        </summary>
        <div className="mt-2 space-y-2" data-testid="nlq-provenance">
          <div>
            <span className="font-medium">NLQ: </span>
            <span data-testid="nlq-question">{question}</span>
          </div>
          <div>
            <span className="font-medium">NLQ IR:</span>
            <pre className="mt-1 overflow-x-auto rounded bg-[var(--bg-code)] p-2 text-[0.7rem]">
              {JSON.stringify(frame.nlq_ir, null, 2)}
            </pre>
          </div>
          <div>
            <span className="font-medium">SQL:</span>
            <div className="relative mt-1">
              <pre className="overflow-x-auto rounded bg-[var(--bg-code)] p-2 pr-6 text-[0.7rem]">
                {frame.source_sql}
              </pre>
              <CopyButton
                value={frame.source_sql}
                label="Copy SQL"
                visibility="always"
                className="absolute right-1 top-1"
              />
            </div>
          </div>
          <div>
            <span className="font-medium">Time range: </span>
            {frame.time_range.from} → {frame.time_range.to}
          </div>
          <div>
            <span className="font-medium">Signals: </span>
            {frame.signal_types.join(", ")}
          </div>
          {frame.sample_rate !== null && frame.sample_rate !== undefined && (
            <div>
              <span className="font-medium">Sample rate: </span>
              {(frame.sample_rate * 100).toFixed(0)}%
            </div>
          )}
        </div>
      </details>
    </>
  );
}
