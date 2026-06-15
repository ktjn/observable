import type { NlqIr, NlqResponse, VisualizationFrame } from "../../api/nlq";
import { VisualizationPanel } from "../nlq/VisualizationPanel";
import type { WorkbenchQueryState } from "./workbenchRuntime";

interface Props {
  runtime: WorkbenchQueryState;
}

export function NotebookResults({ runtime }: Props) {
  if (runtime.status === "idle") {
    return (
      <div className="text-sm text-[var(--text-muted)]" data-testid="workbench-results-idle">
        Run the block to see a result.
      </div>
    );
  }

  if (runtime.status === "loading") {
    return (
      <div data-testid="workbench-results-loading">
        <div className="text-sm text-[var(--text-muted)]">Running query…</div>
      </div>
    );
  }

  if (runtime.status === "error") {
    return (
      <div
        className="rounded border border-[var(--danger-border)] bg-[var(--danger-bg)] px-3 py-2 text-sm text-[var(--danger-text)]"
        role="alert"
        data-testid="workbench-results-error"
      >
        {runtime.message}
      </div>
    );
  }

  const response = runtime.response;
  if (response.type === "decline") {
    return <DeclineMessage reason={response.reason} />;
  }
  if (response.type === "invalid_response") {
    return (
      <InvalidResponsePanel
        question={runtime.question}
        reason={response.reason}
        rawLlmResponse={response.raw_llm_response}
      />
    );
  }
  if (response.type === "capabilities") {
    return <CapabilitiesPanel hint={response.hint} />;
  }
  if (response.type === "ir") {
    return <InterpretedIrPanel ir={response.ir} />;
  }

  return <FrameResult response={response} question={runtime.question} />;
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

function DeclineMessage({ reason }: { reason: string }) {
  return (
    <div
      className="rounded border border-[var(--border)] bg-[var(--bg-subtle)] px-4 py-3 text-sm"
      data-testid="workbench-results-decline"
    >
      <p className="font-medium">This question is outside the NLQ scope</p>
      <p className="mt-1 text-[var(--text-muted)]">{reason}</p>
    </div>
  );
}

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
      data-testid="workbench-results-invalid-response"
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
          data-testid="workbench-results-raw-llm-response"
        >
          {rawLlmResponse}
        </pre>
      </details>
    </div>
  );
}

function CapabilitiesPanel({ hint }: { hint: string }) {
  return (
    <div
      className="rounded border border-[var(--border)] bg-[var(--bg-subtle)] px-4 py-3 text-sm"
      data-testid="workbench-results-capabilities"
    >
      <p className="font-medium mb-2">Observable NLQ Capabilities</p>
      <pre className="whitespace-pre-wrap text-[var(--text-muted)] text-xs font-mono leading-relaxed">
        {hint}
      </pre>
    </div>
  );
}

function FrameResult({
  response,
  question,
}: {
  response: Extract<NlqResponse, { type: "frame" }>;
  question: string;
}) {
  const frame = response.frame as VisualizationFrame;
  return (
    <div className="space-y-3" data-testid="workbench-results-frame">
      <VisualizationPanel frame={frame} />
      <p className="text-xs text-[var(--text-muted)] italic" data-testid="workbench-approximation">
        {frame.approximation_statement}
      </p>
      <details className="text-xs">
        <summary className="cursor-pointer select-none text-[var(--text-muted)] hover:text-[var(--text-strong)]">
          Show query details
        </summary>
        <div className="mt-2 space-y-2" data-testid="workbench-provenance">
          <div>
            <span className="font-medium">NLQ: </span>
            <span data-testid="workbench-question">{question}</span>
          </div>
          <div>
            <span className="font-medium">NLQ IR:</span>
            <pre className="mt-1 overflow-x-auto rounded bg-[var(--bg-code)] p-2 text-[0.7rem]">
              {JSON.stringify(frame.nlq_ir, null, 2)}
            </pre>
          </div>
          <div>
            <span className="font-medium">SQL:</span>
            <pre className="mt-1 overflow-x-auto rounded bg-[var(--bg-code)] p-2 text-[0.7rem]">
              {frame.source_sql}
            </pre>
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
    </div>
  );
}
