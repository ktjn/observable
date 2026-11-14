import { useState } from "react";
import { Badge } from "./badge";
import { Button } from "./button";

/**
 * Canonical action and status terminology shared across monitoring and investigation views (OBS-24-07).
 */
export const SHARED_TERMINOLOGY = {
  acknowledge: "Acknowledge",
  escalate: "Escalate",
  resolve: "Resolve",
  undoFilter: "Undo last refinement",
  clearFilters: "Clear filters",
  inspectEvidence: "Inspect evidence",
} as const;

export type DraftSaveOutcome = "idle" | "pending_reauth" | "saved" | "failed";

/**
 * Preserves unsent user input (e.g. incident note) across session expiry or connectivity loss
 * and reports the actual save outcome once reconnected (OBS-24-12).
 */
export function usePreservedDraft(storageKey: string, initialValue = "") {
  const [value, setValueState] = useState<string>(() => {
    try {
      return sessionStorage.getItem(storageKey) ?? initialValue;
    } catch {
      return initialValue;
    }
  });
  const [outcome, setOutcome] = useState<DraftSaveOutcome>("idle");

  function setValue(next: string) {
    setValueState(next);
    try {
      if (next) {
        sessionStorage.setItem(storageKey, next);
      } else {
        sessionStorage.removeItem(storageKey);
      }
    } catch {
      // Ignore storage errors in restricted environments.
    }
  }

  function markSaved() {
    setOutcome("saved");
    try {
      sessionStorage.removeItem(storageKey);
    } catch {
      // Ignore storage errors.
    }
  }

  function markPendingReauth() {
    setOutcome("pending_reauth");
  }

  function markFailed() {
    setOutcome("failed");
  }

  return { value, setValue, outcome, markSaved, markPendingReauth, markFailed };
}

/**
 * Plain-language contextual explanation of SLO attainment for non-specialist users (OBS-25-05).
 */
export function SloPlainExplanation({
  attainmentPercent,
  targetPercent,
}: {
  attainmentPercent: number;
  targetPercent: number;
}) {
  const meeting = attainmentPercent >= targetPercent;
  return (
    <div
      data-testid="slo-plain-explanation"
      className="rounded border border-[var(--border)] bg-[var(--surface-1)] p-2.5 text-xs"
    >
      <div className="flex items-center gap-2 font-semibold text-[var(--text)]">
        <Badge tone={meeting ? "good" : "bad"}>
          {meeting ? "Meeting customer commitment" : "Commitment at risk"}
        </Badge>
        <span>
          {attainmentPercent.toFixed(2)}% achieved vs {targetPercent.toFixed(2)}% target
        </span>
      </div>
      <p className="mt-1 text-[var(--muted)]">
        {meeting
          ? "Customers are experiencing reliable service within the agreed reliability goal."
          : "More customer requests failed or slowed down than the agreed reliability goal allows."}
      </p>
    </div>
  );
}

export interface FeedbackSubmission {
  route: string;
  service?: string;
  environment?: string;
  periodLabel: string;
  description: string;
}

/**
 * Contextual feedback control for reporting misleading displays or missing context (OBS-25-06).
 */
export function FeedbackControl({
  route,
  service,
  environment,
  periodLabel,
  onSubmit,
}: {
  route: string;
  service?: string;
  environment?: string;
  periodLabel: string;
  onSubmit: (payload: FeedbackSubmission) => void;
}) {
  const [open, setOpen] = useState(false);
  const [description, setDescription] = useState("");
  const [submitted, setSubmitted] = useState(false);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!description.trim()) return;
    onSubmit({ route, service, environment, periodLabel, description: description.trim() });
    setSubmitted(true);
    setDescription("");
    setOpen(false);
  }

  return (
    <div data-testid="feedback-control" className="inline-flex items-center gap-2 text-xs">
      <Button variant="ghost" onClick={() => setOpen((v) => !v)}>
        Report display issue
      </Button>
      {submitted && <Badge tone="good">Feedback submitted for {route}</Badge>}
      {open && (
        <form onSubmit={handleSubmit} className="flex items-center gap-1.5">
          <input
            type="text"
            aria-label="Issue description"
            placeholder="Describe misleading display or missing context…"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            className="rounded border border-[var(--border)] bg-[var(--surface-1)] px-2 py-1 text-xs"
          />
          <Button type="submit" variant="primary">
            Submit feedback
          </Button>
        </form>
      )}
    </div>
  );
}

export interface HandoverReportInput {
  title: string;
  service?: string;
  environment: string;
  periodLabel: string;
  filters: string[];
  metrics: Array<{ label: string; value: string; unit: string }>;
}

/**
 * Generates an accessible plain-text / Markdown operational handover report
 * with explicit units, filters, and time context (OBS-25-07).
 */
export function buildAccessibleHandoverReport(input: HandoverReportInput): string {
  const lines = [
    `# ${input.title}`,
    `- Service: ${input.service ?? "All services"}`,
    `- Environment: ${input.environment}`,
    `- Time context: ${input.periodLabel}`,
    `- Active filters: ${input.filters.length > 0 ? input.filters.join(", ") : "None"}`,
    "",
    "## Measures",
    ...input.metrics.map((m) => `- ${m.label}: ${m.value} (${m.unit})`),
  ];
  return lines.join("\n");
}

export function ExportHandoverButton({
  reportInput,
  onExport,
}: {
  reportInput: HandoverReportInput;
  onExport?: (markdown: string) => void;
}) {
  const [exportedText, setExportedText] = useState<string | null>(null);

  function handleExport() {
    const md = buildAccessibleHandoverReport(reportInput);
    setExportedText(md);
    onExport?.(md);
  }

  return (
    <div className="inline-flex flex-col gap-1">
      <Button variant="secondary" data-testid="export-handover-button" onClick={handleExport}>
        Export handover report
      </Button>
      {exportedText && (
        <pre
          data-testid="handover-report-preview"
          className="rounded border border-[var(--border)] bg-[var(--surface-1)] p-2 text-xs whitespace-pre-wrap"
        >
          {exportedText}
        </pre>
      )}
    </div>
  );
}
