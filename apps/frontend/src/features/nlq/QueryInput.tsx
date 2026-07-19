import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { NlqIr } from "../../api/nlq";
import { getConfig } from "../../api/setup";
import { SignalQueryForm } from "../../components/shared/SignalQueryForm";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
import { useTenantContext } from "../../hooks/useTenantContext";
import { submitNlqWithProvider } from "./submitNlqWithProvider";
import { detectQueryMode, toShorthandQuery, type QueryMode } from "./detectQueryMode";
import type { NlqIrLike } from "./queryFilters";

interface QueryInputProps {
  /**
   * The page base IR. Sent as `base_ir` in interpret requests so the LLM
   * receives correct page context. Also forwarded on `onSubmit` for execute calls.
   */
  baseIr: NlqIrLike;
  serviceName?: string;
  placeholder?: string;
  /**
   * Called with the raw text (as typed, or NLQ text) after the user submits.
   * The page uses this text in its own execute request, merged with `baseIr` server-side.
   */
  onSubmit?: (rawText: string) => void;
  /** Called with the interpreted IR, for debug purposes. */
  onIr?: (ir: NlqIrLike | Record<string, unknown>) => void;
}

const MODE_LABEL: Record<QueryMode, string> = {
  filter: "Filter",
  search: "Search",
  ai: "AI",
};

const MODE_CLASS: Record<QueryMode, string> = {
  filter: "text-[var(--brand)]",
  search: "text-[var(--good)]",
  ai: "text-[var(--muted)]",
};

export function QueryInput({
  baseIr,
  serviceName,
  placeholder,
  onSubmit,
  onIr,
}: QueryInputProps) {
  const { fromMs, toMs } = useGlobalDateRange();
  const { tenantId } = useTenantContext();
  const { data: config } = useQuery({
    queryKey: ["setup", "config", tenantId],
    queryFn: () => getConfig(tenantId),
  });
  const provider = config?.llm_provider ?? "remote";
  const effectiveBaseIr = useMemo<NlqIrLike>(
    () => ({
      ...baseIr,
      time_range: {
        from: String(BigInt(Math.floor(fromMs)) * 1_000_000n),
        to: String(BigInt(Math.floor(toMs)) * 1_000_000n),
      },
    }),
    [baseIr, fromMs, toMs],
  );
  const [query, setQuery] = useState("");
  const [state, setState] = useState<
    | { status: "idle" }
    | { status: "loading" }
    | { status: "error"; message: string }
    | { status: "interpreted"; ir: NlqIr }
  >({ status: "idle" });

  const mode = query.trim() ? detectQueryMode(query) : null;

  function handleReset() {
    setQuery("");
    setState({ status: "idle" });
    onSubmit?.("");
  }

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const rawText = query.trim();
    if (!rawText) return;
    const detectedMode = detectQueryMode(rawText);
    const question = toShorthandQuery(rawText, detectedMode);

    setState({ status: "loading" });
    try {
      const response = await submitNlqWithProvider(
        tenantId,
        { provider, webllmModel: config?.webllm_model },
        {
          question,
          mode: "interpret",
          service_name: serviceName,
          base_ir: effectiveBaseIr,
        },
      );
      if (response.type !== "ir") {
        const message =
          response.type === "decline"
            ? response.reason
            : response.type === "capabilities"
              ? response.hint
              : response.type === "invalid_response"
                ? response.reason
                : "Query returned data instead of filter instructions";
        setState({ status: "error", message });
        return;
      }
      setState({ status: "interpreted", ir: response.ir });
      // Notify parent with the raw (non-shorthand) text so it can drive its own execute call.
      onSubmit?.(rawText);
      onIr?.(response.ir);
    } catch (error) {
      setState({
        status: "error",
        message: error instanceof Error ? error.message : "Query failed",
      });
    }
  }

  return (
    <section className="grid gap-2" aria-label="query filter">
      <SignalQueryForm
        value={query}
        onChange={setQuery}
        onSubmit={handleSubmit}
        isLoading={state.status === "loading"}
        inputLabel="Query current view input"
        formLabel="Query current view"
        placeholder={placeholder ?? "Filter this view — a word, field:value, or a question"}
        idleLabel="Apply query"
        loadingLabel="Interpreting..."
        onReset={handleReset}
        badge={
          mode && (
            <span
              data-testid="query-mode-badge"
              className={`text-[9px] font-bold uppercase tracking-wide ${MODE_CLASS[mode]}`}
            >
              {MODE_LABEL[mode]}
            </span>
          )
        }
      />

      {state.status === "error" && (
        <p className="m-0 text-sm text-[var(--bad)]" role="alert">
          {state.message}
        </p>
      )}

      {state.status === "interpreted" && (
        <details className="text-xs text-[var(--muted)]">
          <summary className="cursor-pointer select-none">Show interpreted IR</summary>
          <pre
            data-testid="query-filter-ir"
            className="mt-1 max-h-48 overflow-auto border border-[var(--border)] bg-[var(--surface)] p-2 text-[0.7rem]"
          >
            {JSON.stringify(state.ir, null, 2)}
          </pre>
        </details>
      )}
    </section>
  );
}
