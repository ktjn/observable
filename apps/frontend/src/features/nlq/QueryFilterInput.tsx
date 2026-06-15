import { useMemo, useState } from "react";
import { submitNlqQuery } from "../../api/nlq";
import type { NlqIr } from "../../api/nlq";
import { SignalQueryForm } from "../../components/shared/SignalQueryForm";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
import { useTenantContext } from "../../hooks/useTenantContext";
import type { NlqIrLike } from "./queryFilters";

interface QueryFilterInputProps {
  /**
   * The page base IR. Sent as `base_ir` in interpret requests so the LLM
   * receives correct page context. Also forwarded on `onSubmit` for execute calls.
   */
  baseIr: NlqIrLike;
  serviceName?: string;
  placeholder?: string;
  /**
   * Called with the raw text (NLQ or raw IR JSON) after the user submits.
   * The page uses this text in its own execute request, merged with `baseIr` server-side.
   */
  onSubmit?: (rawText: string) => void;
  /** @deprecated Use `onSubmit` instead. Called with the interpreted IR for debug purposes. */
  onIr?: (ir: NlqIrLike | Record<string, unknown>) => void;
}

export function QueryFilterInput({
  baseIr,
  serviceName,
  placeholder,
  onSubmit,
  onIr,
}: QueryFilterInputProps) {
  const { fromMs, toMs } = useGlobalDateRange();
  const { tenantId } = useTenantContext();
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

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const question = query.trim();
    if (!question) return;

    setState({ status: "loading" });
    try {
      const response = await submitNlqQuery(tenantId, {
        question,
        mode: "interpret",
        service_name: serviceName,
        base_ir: effectiveBaseIr,
      });
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
      // Notify parent with the raw text so it can drive its own execute call.
      onSubmit?.(question);
      // Legacy: also call onIr if provided.
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
        placeholder={placeholder ?? "Filter this view with natural language or raw NLQ IR JSON"}
        idleLabel="Apply query"
        loadingLabel="Interpreting..."
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
