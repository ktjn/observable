import { useState } from "react";
import { submitNlqQuery } from "../../api/nlq";
import { Button } from "../../components/ui/button";
import { Input } from "../../components/ui/input";
import type { NlqIrLike, QuerySurface } from "./queryFilters";

interface QueryFilterInputProps {
  surface: QuerySurface;
  serviceName?: string;
  placeholder?: string;
  onIr: (ir: NlqIrLike | Record<string, unknown>) => void;
}

export function QueryFilterInput({
  surface,
  serviceName,
  placeholder,
  onIr,
}: QueryFilterInputProps) {
  const [query, setQuery] = useState("");
  const [state, setState] = useState<
    | { status: "idle" }
    | { status: "loading" }
    | { status: "error"; message: string }
    | { status: "interpreted"; ir: Record<string, unknown> }
  >({ status: "idle" });

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    const question = query.trim();
    if (!question) return;

    setState({ status: "loading" });
    try {
      const response = await submitNlqQuery({
        question,
        mode: "interpret",
        service_name: serviceName,
        surface_hint: surface,
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
      onIr(response.ir);
    } catch (error) {
      setState({
        status: "error",
        message: error instanceof Error ? error.message : "Query failed",
      });
    }
  }

  return (
    <section className="grid gap-2" aria-label={`${surface} query filter`}>
      <form
        aria-label="Query current view"
        role="form"
        onSubmit={handleSubmit}
        className="flex gap-2 max-[640px]:flex-col"
      >
        <Input
          aria-label="Query current view input"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={placeholder ?? "Filter this view with natural language or raw NLQ IR JSON"}
          disabled={state.status === "loading"}
          className="min-w-[260px] flex-1"
        />
        <Button type="submit" disabled={state.status === "loading" || !query.trim()}>
          {state.status === "loading" ? "Interpreting..." : "Apply query"}
        </Button>
      </form>

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
