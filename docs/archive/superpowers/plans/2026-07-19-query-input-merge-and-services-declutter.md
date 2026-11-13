# Query Input Merge, Services Declutter, Testbench Markers, Workbench WebLLM Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Merge the quick-filter and NLQ inputs into one mode-detecting `QueryInput`, declutter the Services pages, wire testbench to the existing deployment-marker API, and fix Workbench's WebLLM provider bug.

**Architecture:** Frontend-only changes in `apps/frontend/src` (React 19 + TanStack Query/Router, Vitest + Testing Library) except Task 7, which is a bash script change in `scripts/testbench.sh`. No backend/API changes anywhere — every task routes into existing query-api/ingest-gateway behavior (ADR-029 shorthand grammar, `/v1/deployments`).

**Tech Stack:** React 19, TypeScript, TanStack Query/Router, Vitest, Testing Library, bash (testbench).

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-19-query-input-merge-and-services-declutter-design.md`
- No backend/API changes — all client-side or bash-script changes only.
- `npm` only for frontend dependency/test commands (no yarn/pnpm/bun), per `AGENTS.md`.
- Every new/changed component keeps existing `data-testid`/`aria-label` conventions used by its predecessor so existing test patterns keep working.
- Run frontend tests with: `cd apps/frontend && npm test -- --run <path>` (Vitest). Run the full suite with `npm test -- --run` before the final task's commit.

---

## File Structure

New files:
- `apps/frontend/src/features/nlq/detectQueryMode.ts` — pure mode-detection function
- `apps/frontend/src/features/nlq/detectQueryMode.test.ts` — its unit tests
- `apps/frontend/src/features/nlq/QueryInput.tsx` — merged component (replaces `QueryFilterInput.tsx` and `NlqPanel.tsx`)
- `apps/frontend/src/features/nlq/QueryInput.test.tsx` — its tests (replaces `QueryFilterInput.test.tsx` and `NlqPanel.test.tsx`)

Deleted files:
- `apps/frontend/src/features/nlq/QueryFilterInput.tsx`, `QueryFilterInput.test.tsx`
- `apps/frontend/src/features/nlq/NlqPanel.tsx`, `NlqPanel.test.tsx`

Modified files:
- `apps/frontend/src/components/shared/SignalQueryForm.tsx` — add optional `badge` slot
- 7 `QueryFilterInput` call sites (import/JSX rename only): `apps/frontend/src/components/LogLiveTail.tsx`, `apps/frontend/src/components/shared/SignalExplorer.tsx`, `apps/frontend/src/features/metrics/ServiceMetricsWorkspace.tsx`, `apps/frontend/src/pages/InfrastructureInventoryPage.tsx`, `apps/frontend/src/pages/ProductAreaPage.tsx`, `apps/frontend/src/pages/ServicesPage.tsx`, `apps/frontend/src/pages/ServiceTopologyPage.tsx`
- `apps/frontend/src/pages/ServicesPage.tsx` — also drop the standalone search `<input>`
- `apps/frontend/src/pages/ServiceDetailPage.tsx` — declutter (Task 5)
- `apps/frontend/src/features/services/ServiceReliabilityTab.tsx` — add health badge
- `apps/frontend/src/router.ts` — add `/services/$serviceId/infrastructure` route
- `apps/frontend/src/features/workbench/QueryWorkbench.tsx` — WebLLM fix
- `apps/frontend/src/features/workbench/QueryWorkbench.test.tsx` — mock updates + regression test
- `scripts/testbench.sh` — deployment marker hook

---

### Task 1: `detectQueryMode` — client-side mode detection

**Files:**
- Create: `apps/frontend/src/features/nlq/detectQueryMode.ts`
- Test: `apps/frontend/src/features/nlq/detectQueryMode.test.ts`

**Interfaces:**
- Produces: `export type QueryMode = "filter" | "search" | "ai";` and `export function detectQueryMode(text: string): QueryMode` — used by Task 2's `QueryInput`.

- [ ] **Step 1: Write the failing tests**

```typescript
// apps/frontend/src/features/nlq/detectQueryMode.test.ts
import { describe, expect, test } from "vitest";
import { detectQueryMode } from "./detectQueryMode";

describe("detectQueryMode", () => {
  test("empty string is ai (caller should no-op on empty anyway)", () => {
    expect(detectQueryMode("")).toBe("ai");
  });

  test("explicit slash prefix is always filter", () => {
    expect(detectQueryMode("/anything goes here")).toBe("filter");
  });

  test("field:value shorthand is filter", () => {
    expect(detectQueryMode("service:checkout")).toBe("filter");
    expect(detectQueryMode("environment:prod")).toBe("filter");
  });

  test("m: metric shorthand is filter", () => {
    expect(detectQueryMode("m:http_requests")).toBe("filter");
  });

  test("op: operation shorthand is filter", () => {
    expect(detectQueryMode("op:topk")).toBe("filter");
  });

  test("bare word is search", () => {
    expect(detectQueryMode("error")).toBe("search");
  });

  test("wildcard-wrapped word is search", () => {
    expect(detectQueryMode("*error*")).toBe("search");
    expect(detectQueryMode("error*")).toBe("search");
    expect(detectQueryMode("*error")).toBe("search");
  });

  test("word with dots/dashes/underscores is still search", () => {
    expect(detectQueryMode("checkout-service")).toBe("search");
    expect(detectQueryMode("http.server.errors")).toBe("search");
  });

  test("multi-word phrase is ai", () => {
    expect(detectQueryMode("show checkout services")).toBe("ai");
  });

  test("a full question is ai", () => {
    expect(detectQueryMode("what is p99 latency over the last hour?")).toBe("ai");
  });

  test("quoted phrase with spaces is ai (not a single shorthand token)", () => {
    expect(detectQueryMode('"timeout error"')).toBe("ai");
  });

  test("raw IR JSON text is ai (passed through untouched to the existing raw-IR path)", () => {
    expect(detectQueryMode('{"operation":"catalog"}')).toBe("ai");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/frontend && npm test -- --run src/features/nlq/detectQueryMode.test.ts`
Expected: FAIL with "Cannot find module './detectQueryMode'"

- [ ] **Step 3: Write the implementation**

```typescript
// apps/frontend/src/features/nlq/detectQueryMode.ts
/**
 * Client-side heuristic that decides which of query-api's three query paths
 * (ADR-029 shorthand filter, shorthand free-text search, or full NLQ) a raw
 * input string should take, so QueryInput can silently prefix `/` for the
 * first two and skip the LLM round trip. Mirrors (does not reimplement) the
 * server-side grammar in services/query-api/src/llm_adapter.rs
 * (parse_shorthand_ir) — this only decides routing, the server still does
 * all real parsing.
 */
export type QueryMode = "filter" | "search" | "ai";

const SHORTHAND_FILTER_TOKEN = /^([A-Za-z_][\w.-]*:\S+|m:\S+|op:\S+)$/;
const SEARCH_TOKEN = /^\*?[\w.-]+\*?$/;

export function detectQueryMode(text: string): QueryMode {
  const trimmed = text.trim();
  if (trimmed.startsWith("/")) return "filter";
  if (!trimmed || /\s/.test(trimmed)) return "ai";
  if (SHORTHAND_FILTER_TOKEN.test(trimmed)) return "filter";
  if (SEARCH_TOKEN.test(trimmed)) return "search";
  return "ai";
}

/**
 * Converts detected filter/search text into the `/`-prefixed shorthand
 * string query-api's pre-LLM bypass expects. No-op passthrough for "ai".
 */
export function toShorthandQuery(text: string, mode: QueryMode): string {
  const trimmed = text.trim();
  if (mode === "ai") return trimmed;
  if (trimmed.startsWith("/")) return trimmed;
  if (mode === "search") {
    return `/${trimmed.replace(/^\*+/, "").replace(/\*+$/, "")}`;
  }
  return `/${trimmed}`;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/frontend && npm test -- --run src/features/nlq/detectQueryMode.test.ts`
Expected: PASS (12 tests)

- [ ] **Step 5: Add tests for `toShorthandQuery` and re-run**

```typescript
// append to apps/frontend/src/features/nlq/detectQueryMode.test.ts
import { toShorthandQuery } from "./detectQueryMode";

describe("toShorthandQuery", () => {
  test("filter mode gets a bare slash prefix", () => {
    expect(toShorthandQuery("service:checkout", "filter")).toBe("/service:checkout");
  });

  test("search mode strips wildcards and prefixes slash", () => {
    expect(toShorthandQuery("*error*", "search")).toBe("/error");
    expect(toShorthandQuery("error*", "search")).toBe("/error");
    expect(toShorthandQuery("*error", "search")).toBe("/error");
    expect(toShorthandQuery("error", "search")).toBe("/error");
  });

  test("already-slash-prefixed text is not double-prefixed", () => {
    expect(toShorthandQuery("/service:checkout", "filter")).toBe("/service:checkout");
  });

  test("ai mode passes text through unchanged", () => {
    expect(toShorthandQuery("show checkout services", "ai")).toBe("show checkout services");
  });
});
```

Run: `cd apps/frontend && npm test -- --run src/features/nlq/detectQueryMode.test.ts`
Expected: PASS (16 tests)

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/features/nlq/detectQueryMode.ts apps/frontend/src/features/nlq/detectQueryMode.test.ts
git commit -m "feat(frontend): add client-side query mode detection for merged input"
```

---

### Task 2: Add a `badge` slot to `SignalQueryForm`

**Files:**
- Modify: `apps/frontend/src/components/shared/SignalQueryForm.tsx`

**Interfaces:**
- Consumes: nothing new.
- Produces: `SignalQueryFormProps.badge?: ReactNode`, rendered inside the `ShorthandHint` wrapper, before the `Input`. Task 3's `QueryInput` passes a mode badge here.

This component has no dedicated test file today (it's exercised indirectly through `QueryFilterInput.test.tsx`/`NlqPanel.test.tsx`); Task 3's `QueryInput.test.tsx` covers the badge rendering, so no separate test file is added here.

- [ ] **Step 1: Add the `badge` prop and render it**

```typescript
// apps/frontend/src/components/shared/SignalQueryForm.tsx
import type { FormEvent, ReactNode } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { ShorthandHint } from "../../features/nlq/ShorthandHint";

interface SignalQueryFormProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit: (event: FormEvent) => void;
  isLoading: boolean;
  inputLabel: string;
  formLabel: string;
  placeholder: string;
  idleLabel: string;
  loadingLabel: string;
  inputTestId?: string;
  submitTestId?: string;
  /** Clears the query text and any submitted results. Shown only when there's something to reset. */
  onReset?: () => void;
  resetTestId?: string;
  /** Optional small indicator rendered next to the input (e.g. a detected-mode badge). */
  badge?: ReactNode;
}

export function SignalQueryForm({
  value,
  onChange,
  onSubmit,
  isLoading,
  inputLabel,
  formLabel,
  placeholder,
  idleLabel,
  loadingLabel,
  inputTestId,
  submitTestId,
  onReset,
  resetTestId,
  badge,
}: SignalQueryFormProps) {
  return (
    <form
      aria-label={formLabel}
      role="form"
      onSubmit={onSubmit}
      className="flex gap-2 max-[640px]:flex-col"
    >
      <ShorthandHint className="relative z-30 group min-w-[260px] flex-1">
        <div className="relative">
          <Input
            aria-label={inputLabel}
            value={value}
            onChange={(event) => onChange(event.target.value)}
            placeholder={placeholder}
            disabled={isLoading}
            className="w-full"
            data-testid={inputTestId}
          />
          {badge && (
            <span className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2">
              {badge}
            </span>
          )}
        </div>
      </ShorthandHint>
      <Button
        type="submit"
        disabled={isLoading || !value.trim()}
        data-testid={submitTestId}
      >
        {isLoading ? loadingLabel : idleLabel}
      </Button>
      {onReset && value.trim() && (
        <Button
          type="button"
          variant="secondary"
          onClick={onReset}
          disabled={isLoading}
          aria-label={`Reset ${inputLabel.toLowerCase()}`}
          data-testid={resetTestId}
        >
          Reset
        </Button>
      )}
    </form>
  );
}
```

- [ ] **Step 2: Type-check**

Run: `cd apps/frontend && npx tsc --noEmit`
Expected: no new errors (badge is optional; no existing caller passes it yet, matching the prior signature).

- [ ] **Step 3: Commit**

```bash
git add apps/frontend/src/components/shared/SignalQueryForm.tsx
git commit -m "feat(frontend): add optional mode-badge slot to SignalQueryForm"
```

---

### Task 3: `QueryInput` — the merged component

**Files:**
- Create: `apps/frontend/src/features/nlq/QueryInput.tsx`
- Test: `apps/frontend/src/features/nlq/QueryInput.test.tsx`

**Interfaces:**
- Consumes: `detectQueryMode`, `toShorthandQuery` (Task 1); `submitNlqWithProvider` (existing, `apps/frontend/src/features/nlq/submitNlqWithProvider.ts`); `SignalQueryForm` with its new `badge` prop (Task 2); `getConfig` (existing, `apps/frontend/src/api/setup.ts`); `NlqIrLike` (existing, `apps/frontend/src/features/nlq/queryFilters.ts`).
- Produces: `QueryInputProps` — identical shape to today's `QueryFilterInputProps` (`baseIr`, `serviceName?`, `placeholder?`, `onSubmit?`, `onIr?`) so every call site swaps in with no prop changes. Exported as `export function QueryInput(props: QueryInputProps)`.

This is `QueryFilterInput.tsx` (read in full during planning) with mode detection spliced into `handleSubmit` and a mode badge added. Behavior for the existing "ai" path (raw IR JSON passthrough, interpreted-IR disclosure, error states, reset) is unchanged from today's `QueryFilterInput` — only the filter/search bypass and badge are new.

- [ ] **Step 1: Write the failing tests**

```typescript
// apps/frontend/src/features/nlq/QueryInput.test.tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryInput } from "./QueryInput";
import { TenantContextProvider } from "../../hooks/useTenantContext";

vi.mock("../../api/nlq", () => ({
  submitNlqQuery: vi.fn(),
  prepareNlqQuery: vi.fn(),
  completeNlqQuery: vi.fn(),
}));

vi.mock("../../api/setup", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../api/setup")>();
  return {
    ...actual,
    getConfig: vi.fn(),
  };
});

vi.mock("../../lib/webllm/webllmEngine", () => ({
  checkWebGpuSupport: vi.fn(),
  getOrCreateEngine: vi.fn(),
}));

vi.mock("../../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: () => ({ fromMs: Date.now() - 3600_000, toMs: Date.now() }),
}));

import { submitNlqQuery, prepareNlqQuery, completeNlqQuery } from "../../api/nlq";
import { getConfig } from "../../api/setup";
import { checkWebGpuSupport, getOrCreateEngine } from "../../lib/webllm/webllmEngine";
const mockSubmit = vi.mocked(submitNlqQuery);
const mockPrepare = vi.mocked(prepareNlqQuery);
const mockComplete = vi.mocked(completeNlqQuery);
const mockGetConfig = vi.mocked(getConfig);
const mockCheckWebGpuSupport = vi.mocked(checkWebGpuSupport);
const mockGetOrCreateEngine = vi.mocked(getOrCreateEngine);

const DEFAULT_TENANT_ID = "00000000-0000-0000-0000-000000000001";

const REMOTE_CONFIG = {
  llm_key_configured: true,
  llm_url: null,
  llm_model: null,
  llm_provider: "remote" as const,
  webllm_model: null,
};

const WEBLLM_CONFIG = {
  llm_key_configured: false,
  llm_url: null,
  llm_model: null,
  llm_provider: "webllm" as const,
  webllm_model: "Phi-3-mini-4k-instruct-q4f16_1-MLC",
};

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <TenantContextProvider>
      <QueryClientProvider client={qc}>{children}</QueryClientProvider>
    </TenantContextProvider>
  );
}

const SERVICES_BASE_IR = {
  operation: "catalog",
  signals: ["metrics"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};

const IR_RESPONSE = {
  type: "ir" as const,
  ir: {
    operation: "catalog" as const,
    signals: ["metrics" as const],
    filters: [{ field: "service_name", op: "=", value: "checkout" }],
    group_by: [],
    time_range: { from: "now-1h", to: "now" },
    metric: null,
    window: null,
    resolution: null,
    visualization_hint: null,
  },
};

beforeEach(() => {
  mockGetConfig.mockResolvedValue(REMOTE_CONFIG);
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("QueryInput", () => {
  test("shows a Filter badge and sends a slash-prefixed shorthand query for field:value input", async () => {
    mockSubmit.mockResolvedValue(IR_RESPONSE);
    render(<QueryInput baseIr={SERVICES_BASE_IR} />, { wrapper });

    fireEvent.change(screen.getByRole("textbox", { name: "Query current view input" }), {
      target: { value: "service:checkout" },
    });
    expect(screen.getByTestId("query-mode-badge")).toHaveTextContent("Filter");

    fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));

    await waitFor(() =>
      expect(mockSubmit).toHaveBeenCalledWith(
        DEFAULT_TENANT_ID,
        expect.objectContaining({ question: "/service:checkout", mode: "interpret" }),
      ),
    );
  });

  test("shows a Search badge and strips wildcards for *word* input", async () => {
    mockSubmit.mockResolvedValue(IR_RESPONSE);
    render(<QueryInput baseIr={SERVICES_BASE_IR} />, { wrapper });

    fireEvent.change(screen.getByRole("textbox", { name: "Query current view input" }), {
      target: { value: "*error*" },
    });
    expect(screen.getByTestId("query-mode-badge")).toHaveTextContent("Search");

    fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));

    await waitFor(() =>
      expect(mockSubmit).toHaveBeenCalledWith(
        DEFAULT_TENANT_ID,
        expect.objectContaining({ question: "/error", mode: "interpret" }),
      ),
    );
  });

  test("shows an AI badge and sends multi-word text unchanged", async () => {
    mockSubmit.mockResolvedValue(IR_RESPONSE);
    render(<QueryInput baseIr={SERVICES_BASE_IR} />, { wrapper });

    fireEvent.change(screen.getByRole("textbox", { name: "Query current view input" }), {
      target: { value: "show checkout services" },
    });
    expect(screen.getByTestId("query-mode-badge")).toHaveTextContent("AI");

    fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));

    await waitFor(() =>
      expect(mockSubmit).toHaveBeenCalledWith(
        DEFAULT_TENANT_ID,
        expect.objectContaining({ question: "show checkout services", mode: "interpret" }),
      ),
    );
  });

  test("no badge is shown when the input is empty", () => {
    render(<QueryInput baseIr={SERVICES_BASE_IR} />, { wrapper });
    expect(screen.queryByTestId("query-mode-badge")).not.toBeInTheDocument();
  });

  test("onSubmit/onIr receive the raw (non-shorthand) text and interpreted IR, same as before", async () => {
    const onIr = vi.fn();
    const onSubmit = vi.fn();
    mockSubmit.mockResolvedValue(IR_RESPONSE);

    render(<QueryInput onIr={onIr} onSubmit={onSubmit} baseIr={SERVICES_BASE_IR} />, { wrapper });
    fireEvent.change(screen.getByRole("textbox", { name: "Query current view input" }), {
      target: { value: "service:checkout" },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));

    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith("service:checkout"));
    expect(onIr).toHaveBeenCalledWith(IR_RESPONSE.ir);
  });

  test("reset clears text and badge", async () => {
    mockSubmit.mockResolvedValue(IR_RESPONSE);
    render(<QueryInput baseIr={SERVICES_BASE_IR} />, { wrapper });
    const input = screen.getByRole("textbox", { name: "Query current view input" }) as HTMLInputElement;

    fireEvent.change(input, { target: { value: "error" } });
    fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));
    await waitFor(() => expect(mockSubmit).toHaveBeenCalled());

    fireEvent.click(screen.getByRole("button", { name: /reset/i }));
    expect(input.value).toBe("");
    expect(screen.queryByTestId("query-mode-badge")).not.toBeInTheDocument();
  });

  // Regression test for the Workbench WebLLM bug (Task 6): every NLQ-submitting
  // surface must route through submitNlqWithProvider, not submitNlqQuery directly.
  test("routes through the two-phase WebLLM flow instead of submitNlqQuery when provider is webllm", async () => {
    mockGetConfig.mockResolvedValue(WEBLLM_CONFIG);
    mockCheckWebGpuSupport.mockResolvedValue({ supported: true });
    mockPrepare.mockResolvedValue({
      type: "prepared",
      session_token: "token-1",
      system_prompt: "sys",
      question: "show checkout services",
    });
    mockGetOrCreateEngine.mockResolvedValue({
      complete: vi.fn().mockResolvedValue('{"operation":"catalog"}'),
      dispose: vi.fn(),
    });
    mockComplete.mockResolvedValue({ type: "final", response: IR_RESPONSE });

    render(<QueryInput baseIr={SERVICES_BASE_IR} />, { wrapper });
    await waitFor(() => expect(mockGetConfig).toHaveBeenCalled());
    fireEvent.change(screen.getByRole("textbox", { name: "Query current view input" }), {
      target: { value: "show checkout services" },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));

    await waitFor(() => expect(mockPrepare).toHaveBeenCalled());
    expect(mockSubmit).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/frontend && npm test -- --run src/features/nlq/QueryInput.test.tsx`
Expected: FAIL with "Cannot find module './QueryInput'"

- [ ] **Step 3: Write the implementation**

```typescript
// apps/frontend/src/features/nlq/QueryInput.tsx
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/frontend && npm test -- --run src/features/nlq/QueryInput.test.tsx`
Expected: PASS (7 tests)

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/features/nlq/QueryInput.tsx apps/frontend/src/features/nlq/QueryInput.test.tsx
git commit -m "feat(frontend): add merged QueryInput (filter/search/AI mode detection)"
```

---

### Task 4: Migrate all `QueryFilterInput` call sites to `QueryInput`, delete old files

**Files:**
- Modify: `apps/frontend/src/components/LogLiveTail.tsx`, `apps/frontend/src/components/shared/SignalExplorer.tsx`, `apps/frontend/src/features/metrics/ServiceMetricsWorkspace.tsx`, `apps/frontend/src/pages/InfrastructureInventoryPage.tsx`, `apps/frontend/src/pages/ProductAreaPage.tsx`, `apps/frontend/src/pages/ServiceTopologyPage.tsx`
- Delete: `apps/frontend/src/features/nlq/QueryFilterInput.tsx`, `apps/frontend/src/features/nlq/QueryFilterInput.test.tsx`

`ServicesPage.tsx` is handled separately in Task 5 because it also needs the standalone search box removed. Each file below needs exactly two changes: the import line and the JSX tag name — props are unchanged (verified during planning: all 6 pass a subset of `baseIr`/`placeholder`/`serviceName`/`onIr`/`onSubmit`, matching `QueryInputProps` exactly).

- [ ] **Step 1: `LogLiveTail.tsx`**

Change line 9 from:
```typescript
import { QueryFilterInput } from "../features/nlq/QueryFilterInput";
```
to:
```typescript
import { QueryInput } from "../features/nlq/QueryInput";
```
Change the JSX tag at line 68 from `<QueryFilterInput` to `<QueryInput` (closing tag, if any, likewise).

- [ ] **Step 2: `SignalExplorer.tsx`**

Change line 3 from:
```typescript
import { QueryFilterInput } from "../../features/nlq/QueryFilterInput";
```
to:
```typescript
import { QueryInput } from "../../features/nlq/QueryInput";
```
Change the JSX tag at line 95 from `<QueryFilterInput` to `<QueryInput`. Update the doc comment at lines 17-19 to say `QueryInput` instead of `QueryFilterInput`.

- [ ] **Step 3: `ServiceMetricsWorkspace.tsx`**

Change line 19 from:
```typescript
import { QueryFilterInput } from "../nlq/QueryFilterInput";
```
to:
```typescript
import { QueryInput } from "../nlq/QueryInput";
```
Change the JSX tag at line 186 from `<QueryFilterInput` to `<QueryInput`.

- [ ] **Step 4: `InfrastructureInventoryPage.tsx`**

Change line 22 from:
```typescript
import { QueryFilterInput } from "../features/nlq/QueryFilterInput";
```
to:
```typescript
import { QueryInput } from "../features/nlq/QueryInput";
```
Change the JSX tag at line 168 from `<QueryFilterInput` to `<QueryInput`.

- [ ] **Step 5: `ProductAreaPage.tsx`**

Change line 10 from:
```typescript
import { QueryFilterInput } from "../features/nlq/QueryFilterInput";
```
to:
```typescript
import { QueryInput } from "../features/nlq/QueryInput";
```
Change the JSX tag at line 105 from `<QueryFilterInput` to `<QueryInput`.

- [ ] **Step 6: `ServiceTopologyPage.tsx`**

Change line 12 from:
```typescript
import { QueryFilterInput } from "../features/nlq/QueryFilterInput";
```
to:
```typescript
import { QueryInput } from "../features/nlq/QueryInput";
```
Change the JSX tag at line 57 from `<QueryFilterInput` to `<QueryInput`.

- [ ] **Step 7: Delete the old component and its test**

```bash
git rm apps/frontend/src/features/nlq/QueryFilterInput.tsx apps/frontend/src/features/nlq/QueryFilterInput.test.tsx
```

- [ ] **Step 8: Type-check and run the full frontend test suite**

Run: `cd apps/frontend && npx tsc --noEmit && npm test -- --run`
Expected: no TypeScript errors; all tests pass (no remaining references to `QueryFilterInput`).

- [ ] **Step 9: Commit**

```bash
git add apps/frontend/src/components/LogLiveTail.tsx apps/frontend/src/components/shared/SignalExplorer.tsx apps/frontend/src/features/metrics/ServiceMetricsWorkspace.tsx apps/frontend/src/pages/InfrastructureInventoryPage.tsx apps/frontend/src/pages/ProductAreaPage.tsx apps/frontend/src/pages/ServiceTopologyPage.tsx
git commit -m "refactor(frontend): migrate QueryFilterInput call sites to QueryInput"
```

---

### Task 5: `ServicesPage` — drop the redundant search box, migrate to `QueryInput`

**Files:**
- Modify: `apps/frontend/src/pages/ServicesPage.tsx`
- Modify: `apps/frontend/src/pages/ServicesPage.test.tsx` (only if the existing test breaks — see Step 3)

**Interfaces:**
- Consumes: `QueryInput` from Task 3.

- [ ] **Step 1: Swap the import and JSX tag**

Change line 23 from:
```typescript
import { QueryFilterInput } from "../features/nlq/QueryFilterInput";
```
to:
```typescript
import { QueryInput } from "../features/nlq/QueryInput";
```
Change the JSX tag at line 155 from `<QueryFilterInput` to `<QueryInput`.

- [ ] **Step 2: Delete the standalone search box**

Remove this block (originally lines 186-192):
```tsx
            <input
              type="search"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search services…"
              className="ml-auto min-w-[180px] px-2.5 py-1 text-xs border border-[var(--border)] bg-transparent text-[var(--text)] placeholder:text-[var(--muted)] rounded focus:outline-none focus:border-[var(--brand)]"
            />
```
Leave the `PillFilter` above it in place, and remove the now-empty wrapping div's trailing whitespace if the `<input>` was its only other child — check the surrounding `<div className="toolbar-row flex-wrap gap-y-2">` still renders correctly with just the `PillFilter` inside (it will — `PillFilter` doesn't depend on a sibling).

The `search` state (`useState`) and `setSearch` stay — they're still written by `QueryInput`'s `onIr` callback (`deriveViewFiltersFromIr(ir, "services")` → `filters.text`) and read by `filteredAndSorted`'s filter logic. Only the manual `<input>` element is removed.

- [ ] **Step 3: Run existing tests**

Run: `cd apps/frontend && npm test -- --run src/pages/ServicesPage.test.tsx`
Expected: PASS — the existing test (`renders active alert count and latest deploy columns`) doesn't touch the search box, so no changes needed to the test file itself.

- [ ] **Step 4: Type-check**

Run: `cd apps/frontend && npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add apps/frontend/src/pages/ServicesPage.tsx
git commit -m "refactor(frontend): drop redundant search box on ServicesPage, use QueryInput"
```

---

### Task 6: Delete `NlqPanel`; declutter `ServiceDetailPage` into tabs

**Files:**
- Delete: `apps/frontend/src/features/nlq/NlqPanel.tsx`, `apps/frontend/src/features/nlq/NlqPanel.test.tsx`
- Modify: `apps/frontend/src/pages/ServiceDetailPage.tsx`
- Modify: `apps/frontend/src/features/services/ServiceReliabilityTab.tsx`
- Modify: `apps/frontend/src/router.ts`

**Interfaces:**
- Produces: `ServiceReliabilityTab` gains a new required prop `healthState: "healthy" | "watch" | "breach"` (the `ServiceSummary["health_state"]` type already used elsewhere in `ServiceDetailPage.tsx`).
- Consumes: `ServiceInfraPanel` (existing, `apps/frontend/src/components/ServiceInfraPanel.tsx`) is reused as tab content instead of being always-rendered.

This task removes: the "Signal Entry Points" panel, the "Current State" panel (its health-state info moves into `ServiceReliabilityTab`; its latest-deployment info is already shown in that tab's existing Deployments table — no code needed for that part), the "Ask/NLQ" panel, and all NLQ-frame-in-tab plumbing (`nlqFrame`/`nlqTab`/`NlqTabFrame`/`signalTabFromFrame`) that only existed to support the now-deleted Ask panel. It adds an "Infrastructure" tab and a small "Ask in Workbench" header link.

- [ ] **Step 1: Delete `NlqPanel` and its test**

```bash
git rm apps/frontend/src/features/nlq/NlqPanel.tsx apps/frontend/src/features/nlq/NlqPanel.test.tsx
```

- [ ] **Step 2: Add the health badge to `ServiceReliabilityTab`**

In `apps/frontend/src/features/services/ServiceReliabilityTab.tsx`, add a `healthState` prop and render a badge next to the existing title block. Change the props type and the header JSX (the `deploymentTone` helper pattern already in this file is the model to follow):

```typescript
// add near the top, alongside deploymentTone/formatMinutes
type HealthState = "healthy" | "watch" | "breach";

function healthTone(state: HealthState): "good" | "warn" | "bad" {
  if (state === "breach") return "bad";
  if (state === "watch") return "warn";
  return "good";
}

function healthLabel(state: HealthState): string {
  if (state === "breach") return "Breach";
  if (state === "watch") return "Watch";
  return "Healthy";
}
```

Change the function signature:
```typescript
export function ServiceReliabilityTab({
  serviceName,
  healthState,
}: {
  serviceName: string;
  healthState: HealthState;
}) {
```

Change the header block (originally lines 83-92) to include the badge:
```tsx
      <div className="space-y-1">
        <div className="flex items-center gap-2 text-xs font-bold uppercase text-[var(--muted)]">
          <span>Reliability</span>
          <Badge tone={healthTone(healthState)}>{healthLabel(healthState)}</Badge>
        </div>
        <h3 className="text-lg font-semibold text-[var(--text-strong)]">
          {serviceName}
        </h3>
        <p className="text-sm text-[var(--muted)]">
          Window: {formatTimestamp(isoToNs(data.from), format)} to{" "}
          {formatTimestamp(isoToNs(data.to), format)}
        </p>
      </div>
```

- [ ] **Step 3: Check for existing `ServiceReliabilityTab` tests and update the call**

Run: `cd apps/frontend && grep -rn "ServiceReliabilityTab" src --include="*.test.tsx"`

If `ServiceReliabilityTab.test.tsx` exists, add `healthState="healthy"` (or the case-appropriate value) to every `render(<ServiceReliabilityTab .../>)` call so the required prop is satisfied. Run that test file and fix any failures before continuing:

Run: `cd apps/frontend && npm test -- --run src/features/services/ServiceReliabilityTab.test.tsx`
Expected: PASS

- [ ] **Step 4: Add the Infrastructure route**

In `apps/frontend/src/router.ts`, add after `serviceReliabilityRoute` (originally lines 125-129):
```typescript
const serviceInfrastructureRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/services/$serviceId/infrastructure",
  component: ServiceDetailPage,
});
```
And add `serviceInfrastructureRoute,` to the route-tree children array, next to the other `service*Route` entries (originally lines 268-274).

- [ ] **Step 5: Rewrite `ServiceDetailPage.tsx`**

Remove the `NlqPanel`, `VisualizationFrame`, and `VisualizationPanel` imports (originally lines 21-23) — `VisualizationPanel` stays imported nowhere in this file once `NlqTabFrame` is deleted in Step 6. Remove the `ServiceInfraPanel` top-level import is *kept* (still used, just moved into the tab switch).

Change `ServiceSignalTab` (originally line 249) to add `"infrastructure"`:
```typescript
type ServiceSignalTab = "reliability" | "logs" | "metrics" | "traces" | "infrastructure" | "deployments" | "alerts";
```

Change `signalTabFromPath` (originally lines 251-258) to add the new path:
```typescript
function signalTabFromPath(pathname: string): ServiceSignalTab {
  if (pathname.endsWith("/reliability")) return "reliability";
  if (pathname.endsWith("/metrics")) return "metrics";
  if (pathname.endsWith("/traces")) return "traces";
  if (pathname.endsWith("/infrastructure")) return "infrastructure";
  if (pathname.endsWith("/deployments")) return "deployments";
  if (pathname.endsWith("/alerts")) return "alerts";
  return "logs";
}
```

In `ServiceDetailView` (originally starting line 90): remove the `nlqFrame`/`nlqTab`/`displayedTab` state (originally lines 101-103) — replace:
```typescript
  const [nlqFrame, setNlqFrame] = useState<VisualizationFrame | null>(null);
  const [nlqTab, setNlqTab] = useState<ServiceSignalTab | null>(null);
  const displayedTab = nlqTab ?? activeTab;
  const { tenantId } = useTenantContext();
```
with:
```typescript
  const { tenantId } = useTenantContext();
```
(drop the `useState` import too if nothing else in this file uses it — check first: `useState` is used only for `nlqFrame`/`nlqTab` in this file per Step 5, so remove it from the top `import { useState } from "react";` at line 3 if it becomes unused. Verify with `grep -n "useState" apps/frontend/src/pages/ServiceDetailPage.tsx` after this step — if no remaining uses, delete the import line.)

Add the header "Ask in Workbench" link — change the `page-header` block (originally lines 125-131):
```tsx
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Service Overview</div>
          <h1>{service.service_name}</h1>
        </div>
        <div className="flex items-center gap-3">
          <Link to="/workbench" className="secondary-link">Ask in Workbench →</Link>
          <Link to="/services" className="secondary-link">Back to services</Link>
        </div>
      </div>
```
(`/workbench` is `workbenchRoute` in `apps/frontend/src/router.ts:195-197` — confirmed during planning, no further verification needed.)

Delete the `detail-grid` block entirely (originally lines 167-223 — the "Current State" and "Signal Entry Points" panels).

Delete the `ServiceInfraPanel` top-level render (originally line 225: `<ServiceInfraPanel serviceName={service.service_name} />`) and the "Ask" panel block (originally lines 227-238).

Change the `ServiceSignalTabs` call (originally lines 240-244) to:
```tsx
      <ServiceSignalTabs
        serviceName={service.service_name}
        activeTab={activeTab}
        healthState={service.health_state}
      />
```

- [ ] **Step 6: Update `ServiceSignalTabs` to drop NLQ-frame plumbing and add the Infrastructure tab**

Replace the function signature and tab list (originally lines 267-284):
```typescript
function ServiceSignalTabs({
  serviceName,
  activeTab,
  healthState,
}: {
  serviceName: string;
  activeTab: ServiceSignalTab;
  healthState: ServiceSummary["health_state"];
}) {
  const encodedService = encodeURIComponent(serviceName);
  const tabLinks = [
    { tab: "reliability" as const, label: "Reliability", to: "/services/$serviceId/reliability" },
    { tab: "logs" as const,         label: "Logs",        to: "/services/$serviceId/logs" },
    { tab: "metrics" as const,      label: "Metrics",     to: "/services/$serviceId/metrics" },
    { tab: "traces" as const,       label: "Traces",      to: "/services/$serviceId/traces" },
    { tab: "infrastructure" as const, label: "Infrastructure", to: "/services/$serviceId/infrastructure" },
    { tab: "deployments" as const,  label: "Deployments", to: "/services/$serviceId/deployments" },
    { tab: "alerts" as const,       label: "Alerts",      to: "/services/$serviceId/alerts" },
  ];
```

Replace the tab-content switch (originally lines 301-328) to drop every `nlqFrame`/`NlqTabFrame` branch and add the infrastructure case:
```tsx
      {activeTab === "logs" && <ServiceLogsTab serviceName={serviceName} />}
      {activeTab === "metrics" && <ServiceMetricsWorkspace initialService={serviceName} />}
      {activeTab === "traces" && <ServiceTracesTab serviceName={serviceName} />}
      {activeTab === "infrastructure" && <ServiceInfraPanel serviceName={serviceName} />}
      {activeTab === "deployments" && <ServiceDeploymentsTab serviceName={serviceName} />}
      {activeTab === "reliability" && (
        <ServiceReliabilityTab serviceName={serviceName} healthState={healthState} />
      )}
      {activeTab === "alerts" && <ServiceAlertsTab />}
```

- [ ] **Step 7: Delete now-dead code**

Delete `signalTabFromFrame` (originally lines 333-338) and `NlqTabFrame` (originally lines 340-349) — both only existed to support the removed Ask panel.

Delete `healthLabel` (originally lines 386-390) and `describeRange` (originally lines 260-265) — both only existed to support the removed "Current State" panel. Before deleting, run `grep -n "healthLabel\|describeRange" apps/frontend/src/pages/ServiceDetailPage.tsx` to confirm no remaining call sites (the `HealthStatus` component at the bottom of the file is a *different*, still-used function — do not delete it, it's still used by `ResponseTimeGraphSection`/elsewhere in this file).

- [ ] **Step 8: Type-check**

Run: `cd apps/frontend && npx tsc --noEmit`
Expected: no errors — this will catch any leftover reference to deleted state/functions/imports.

- [ ] **Step 9: Write/update a `ServiceDetailPage` smoke test**

No test file exists for this page today. Add a minimal one covering the new tab set:

```typescript
// apps/frontend/src/pages/ServiceDetailPage.test.tsx
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import * as servicesApi from "../api/services";
import ServiceDetailPage from "./ServiceDetailPage";

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    useParams: () => ({ serviceId: "checkout" }),
    useLocation: () => ({ pathname: "/services/checkout" }),
    Link: ({
      children,
      to,
      ...props
    }: React.AnchorHTMLAttributes<HTMLAnchorElement> & { to?: string; children?: React.ReactNode }) => (
      <a href={to ?? "#"} {...props}>
        {children}
      </a>
    ),
  };
});

vi.mock("../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: () => ({
    preset: "1h",
    fromMs: 0,
    toMs: 3_600_000,
    setPreset: vi.fn(),
    setCustomRange: vi.fn(),
    clearCustomRange: vi.fn(),
  }),
}));

const sampleSummary: servicesApi.ServiceSummary = {
  service_name: "checkout",
  request_rate: 12.5,
  error_rate: 0.02,
  p95_latency_ms: 245,
  health_state: "healthy",
  active_alert_count: 0,
  latest_deployment: "v2.3.1",
};

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <ServiceDetailPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
});

test("shows the Infrastructure tab and no Signal Entry Points or Ask panel", async () => {
  vi.spyOn(servicesApi, "getServiceSummary").mockResolvedValue({ service: sampleSummary });
  vi.spyOn(servicesApi, "getServiceResponseTimeHistory").mockResolvedValue({ buckets: [] });

  renderPage();

  await waitFor(() => expect(screen.getByRole("heading", { name: "checkout" })).toBeInTheDocument());
  expect(screen.getByRole("link", { name: "Infrastructure" })).toBeInTheDocument();
  expect(screen.queryByText("Signal Entry Points")).not.toBeInTheDocument();
  expect(screen.queryByText("Natural Language Query")).not.toBeInTheDocument();
  expect(screen.getByRole("link", { name: /Ask in Workbench/i })).toBeInTheDocument();
});
```

`getServiceSummary` returns `Promise<ServiceDetailResponse>` = `{ service: ServiceSummary }`, and `getServiceResponseTimeHistory` returns `Promise<ResponseTimeHistoryResponse>` = `{ buckets: ResponseTimeHistoryBucket[] }` (confirmed in `apps/frontend/src/api/services.ts:23-25,120-122` during planning) — the mocked values above already match.

Run: `cd apps/frontend && npm test -- --run src/pages/ServiceDetailPage.test.tsx`
Expected: PASS

- [ ] **Step 10: Run the full frontend suite**

Run: `cd apps/frontend && npm test -- --run`
Expected: PASS — this catches any other file that imported `NlqPanel`, `signalTabFromFrame`, etc.

- [ ] **Step 11: Commit**

```bash
git add apps/frontend/src/pages/ServiceDetailPage.tsx apps/frontend/src/pages/ServiceDetailPage.test.tsx apps/frontend/src/features/services/ServiceReliabilityTab.tsx apps/frontend/src/router.ts
git commit -m "refactor(frontend): declutter ServiceDetailPage into a single tab strip"
```

---

### Task 7: Fix Workbench's WebLLM routing bug

**Files:**
- Modify: `apps/frontend/src/features/workbench/QueryWorkbench.tsx`
- Modify: `apps/frontend/src/features/workbench/QueryWorkbench.test.tsx`

**Interfaces:**
- Consumes: `submitNlqWithProvider` (existing), `getConfig` (existing).

- [ ] **Step 1: Add the `getConfig` mock and a webllm regression test to the existing test file**

Add near the top of `QueryWorkbench.test.tsx`, alongside the existing mocks:
```typescript
vi.mock("../../api/setup", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../api/setup")>();
  return {
    ...actual,
    getConfig: vi.fn(),
  };
});

vi.mock("../../lib/webllm/webllmEngine", () => ({
  checkWebGpuSupport: vi.fn(),
  getOrCreateEngine: vi.fn(),
}));
```
Change the `"../../api/nlq"` mock to also export `prepareNlqQuery`/`completeNlqQuery` (needed by `submitNlqWithProvider`'s webllm branch):
```typescript
vi.mock("../../api/nlq", () => ({
  submitNlqQuery: vi.fn(),
  prepareNlqQuery: vi.fn(),
  completeNlqQuery: vi.fn(),
}));
```
Add the corresponding imports/mocked handles near the existing `import { submitNlqQuery } ...` line:
```typescript
import { submitNlqQuery, prepareNlqQuery, completeNlqQuery } from "../../api/nlq";
import { getConfig } from "../../api/setup";
import { checkWebGpuSupport, getOrCreateEngine } from "../../lib/webllm/webllmEngine";

const mockSubmit = vi.mocked(submitNlqQuery);
const mockPrepare = vi.mocked(prepareNlqQuery);
const mockComplete = vi.mocked(completeNlqQuery);
const mockGetConfig = vi.mocked(getConfig);
const mockCheckWebGpuSupport = vi.mocked(checkWebGpuSupport);
const mockGetOrCreateEngine = vi.mocked(getOrCreateEngine);
```
Add a `beforeEach` so every existing (remote-path) test keeps passing once `QueryWorkbench` starts calling `getConfig`:
```typescript
const REMOTE_CONFIG = {
  llm_key_configured: true,
  llm_url: null,
  llm_model: null,
  llm_provider: "remote" as const,
  webllm_model: null,
};

beforeEach(() => {
  mockGetConfig.mockResolvedValue(REMOTE_CONFIG);
});
```
(Add the `beforeEach` import to the existing `import { describe, expect, test, vi, afterEach } from "vitest";` line — change to `import { describe, expect, test, vi, afterEach, beforeEach } from "vitest";`.)

Add the regression test at the end of the `describe("QueryWorkbench", ...)` block:
```typescript
  // Regression test for the bug where QueryWorkbench called submitNlqQuery
  // directly instead of the shared provider-aware submitNlqWithProvider,
  // so it ignored the user's WebLLM provider selection on the Setup page.
  test("routes through the two-phase WebLLM flow when the configured provider is webllm", async () => {
    mockGetConfig.mockResolvedValue({
      llm_key_configured: false,
      llm_url: null,
      llm_model: null,
      llm_provider: "webllm" as const,
      webllm_model: "Phi-3-mini-4k-instruct-q4f16_1-MLC",
    });
    mockCheckWebGpuSupport.mockResolvedValue({ supported: true });
    mockPrepare.mockResolvedValue({
      type: "prepared",
      session_token: "token-1",
      system_prompt: "sys",
      question: "p95 latency",
    });
    mockGetOrCreateEngine.mockResolvedValue({
      complete: vi.fn().mockResolvedValue('{"operation":"timeseries"}'),
      dispose: vi.fn(),
    });
    mockComplete.mockResolvedValue({ type: "final", response: FRAME_RESPONSE });

    renderWorkbench();
    await waitFor(() => expect(mockGetConfig).toHaveBeenCalled());

    const metricsBlock = screen.getByTestId("workbench-block-metrics");
    fireEvent.change(within(metricsBlock).getByRole("textbox"), {
      target: { value: "p95 latency" },
    });
    fireEvent.click(within(metricsBlock).getByTestId("workbench-run-metrics"));

    await waitFor(() => expect(mockPrepare).toHaveBeenCalled());
    expect(mockSubmit).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(within(metricsBlock).getByTestId("workbench-results-frame")).toBeInTheDocument(),
    );
  });
```

- [ ] **Step 2: Run tests to verify the new test fails and existing tests still pass their old assertions for now**

Run: `cd apps/frontend && npm test -- --run src/features/workbench/QueryWorkbench.test.tsx`
Expected: the new "routes through the two-phase WebLLM flow" test FAILs (still calling `submitNlqQuery` today); other tests PASS (they don't yet require `getConfig` to matter since `QueryWorkbench` doesn't call it yet — this just confirms the mock additions didn't break anything).

- [ ] **Step 3: Fix `QueryWorkbench.tsx`**

Change the import at the top (originally lines 3-4):
```typescript
import type { NlqIrLike } from "../../api/nlq";
import { submitNlqQuery } from "../../api/nlq";
```
to:
```typescript
import type { NlqIrLike } from "../../api/nlq";
import { useQuery } from "@tanstack/react-query";
import { getConfig } from "../../api/setup";
import { submitNlqWithProvider } from "../nlq/submitNlqWithProvider";
```

Add the config query inside `QueryWorkbench()`, alongside the existing `tenantId`/`fromMs`/`toMs` hooks (originally lines 41-42):
```typescript
  const { tenantId } = useTenantContext();
  const { data: config } = useQuery({
    queryKey: ["setup", "config", tenantId],
    queryFn: () => getConfig(tenantId),
  });
  const provider = config?.llm_provider ?? "remote";
```

Change both `submitNlqQuery` call sites in `runBlock` (originally lines 121-124 and 135-139) to `submitNlqWithProvider`:
```typescript
        const response = await submitNlqWithProvider(
          tenantId,
          { provider, webllmModel: config?.webllm_model },
          { base_ir: mergedBaseIr, mode: "execute" },
        );
```
and:
```typescript
      const response = await submitNlqWithProvider(
        tenantId,
        { provider, webllmModel: config?.webllm_model },
        { base_ir: baseIr, question, mode: "execute" },
      );
```

- [ ] **Step 4: Run tests to verify everything passes**

Run: `cd apps/frontend && npm test -- --run src/features/workbench/QueryWorkbench.test.tsx`
Expected: PASS (all tests, including the new regression test).

- [ ] **Step 5: Type-check and run the full suite**

Run: `cd apps/frontend && npx tsc --noEmit && npm test -- --run`
Expected: no errors, all tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/frontend/src/features/workbench/QueryWorkbench.tsx apps/frontend/src/features/workbench/QueryWorkbench.test.tsx
git commit -m "fix(frontend): route Workbench through submitNlqWithProvider (webllm support)"
```

---

### Task 8: Testbench deployment markers

**Files:**
- Modify: `scripts/testbench.sh`

**Interfaces:**
- Consumes existing API: `POST /v1/deployments` (`services/ingest-gateway/src/deployments.rs::create_deployment`) — body `{service_name, environment, service_version, deployed_by?, commit_sha?, metadata?}`, returns `201 {deployment_id}`. `PATCH /v1/deployments/{deployment_id}` (`::finish_deployment`) — body `{status: "success"|"failed"|"rolled_back"}`, returns `204`.
- Auth: `Authorization: Bearer <api-key>` where the key's role is `member` or `admin`. The testbench Helm chart's OTLP ingest already uses `dev-api-key-0000` (`charts/observable-testbench/values.yaml:3`) against the dev tenant `00000000-0000-0000-0000-000000000002` (same key/tenant `scripts/testbench.sh` already uses for its admin-service alert-rule seeding, `DEV_KEY`/`DEV_TENANT` at lines 401-402) — reuse both, don't mint a new key.
- ingest-gateway HTTP JSON port is `4318` in-cluster (`INGEST_GATEWAY_HTTP_JSON_PORT` default, `services/ingest-gateway/src/main.rs:150`), exposed as the `http` Service port (`charts/observable/values.yaml:80`), in the `observable` namespace (`$OBSERVABLE_NS`).

This hook creates one `in_progress` marker per testbench shop service right after the rollout-wait succeeds (line 226 today: `wait_for_rollouts_parallel ...`), then immediately marks each `success` — testbench deploys are synchronous from the script's point of view (it already waited for rollout success), so there's no "long-running deploy" window to track; the marker exists so the platform has *a* record of "this service was deployed at this time" to render on its chart, not to model an in-progress state visible to the user.

- [ ] **Step 1: Add the deployment-marker hook after the rollout wait**

In `scripts/testbench.sh`, immediately after the block ending at (originally) line 231:
```bash
log "Waiting for otel-collector-agent DaemonSet"
kubectl rollout status daemonset/otel-collector-agent \
  --namespace "$TESTBENCH_NS" --timeout 120s \
  || info "WARN: agent DaemonSet not fully ready"
```
insert:
```bash
# ---------------------------------------------------------------------------
# Record deployment markers for the testbench shop services (non-fatal — the
# demo is still usable without them, this just gives the Deployments tab and
# response-time chart annotations something to show).
# ---------------------------------------------------------------------------

log "Recording deployment markers for testbench shop services"
INGEST_GATEWAY_PORT_LOCAL=14318
DEPLOY_KEY="dev-api-key-0000"
DEPLOY_TENANT="00000000-0000-0000-0000-000000000002"
DEPLOY_VERSION="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo "local")"

kubectl port-forward svc/ingest-gateway "${INGEST_GATEWAY_PORT_LOCAL}:4318" \
  --namespace "$OBSERVABLE_NS" >/tmp/testbench-ingest-port-forward.log 2>&1 &
INGEST_PF_PID=$!

record_deployment_marker() {
  local service_name="$1"
  local create_body
  create_body="$(jq -n \
    --arg service_name "$service_name" \
    --arg environment "production" \
    --arg service_version "$DEPLOY_VERSION" \
    '{service_name: $service_name, environment: $environment, service_version: $service_version}')"

  local deployment_id
  deployment_id="$(curl -sf --max-time 10 -X POST "http://localhost:${INGEST_GATEWAY_PORT_LOCAL}/v1/deployments" \
    -H "Authorization: Bearer ${DEPLOY_KEY}" \
    -H "X-Tenant-ID: ${DEPLOY_TENANT}" \
    -H "Content-Type: application/json" \
    -d "$create_body" | jq -r '.deployment_id')" || return 1

  [[ -z "$deployment_id" || "$deployment_id" == "null" ]] && return 1

  curl -sf --max-time 10 -X PATCH "http://localhost:${INGEST_GATEWAY_PORT_LOCAL}/v1/deployments/${deployment_id}" \
    -H "Authorization: Bearer ${DEPLOY_KEY}" \
    -H "X-Tenant-ID: ${DEPLOY_TENANT}" \
    -H "Content-Type: application/json" \
    -d '{"status":"success"}' >/dev/null
}

if timeout 15 bash -c "until curl -sf --max-time 2 http://localhost:${INGEST_GATEWAY_PORT_LOCAL}/health >/dev/null 2>&1; do sleep 1; done"; then
  marker_failures=0
  for svc in shop-api shop-frontend shop-worker; do
    record_deployment_marker "$svc" || marker_failures=$((marker_failures + 1))
  done
  if [[ "$marker_failures" -eq 0 ]]; then
    info "PASS: recorded deployment markers for shop-api, shop-frontend, shop-worker"
  else
    info "WARN: failed to record $marker_failures deployment marker(s) — continuing without them"
  fi
else
  info "WARN: ingest-gateway not reachable for deployment markers — continuing without them"
fi

kill "$INGEST_PF_PID" 2>/dev/null || true
```

ingest-gateway serves `/health` on its HTTP JSON port (confirmed in `services/ingest-gateway/src/http-json/mod.rs:95,127` during planning), matching the admin-service readiness-check pattern this hook mirrors (lines 417-424 today).

- [ ] **Step 2: Shellcheck**

Run: `bash -n scripts/testbench.sh`
Expected: no syntax errors.

Run (if `shellcheck` is available; skip with a note if not installed): `shellcheck scripts/testbench.sh`
Expected: no new warnings introduced by this change (pre-existing warnings elsewhere in the file are out of scope).

- [ ] **Step 3: Manual verification**

Run: `bash scripts/testbench.sh` (full run — this builds images, creates/reuses a kind cluster, and deploys everything; expect this to take several minutes)

After it completes, open the Observable frontend (URL printed at the end of the script, typically `http://localhost:8080/`), navigate to a shop service's detail page → Deployments tab (or Reliability tab, per Task 6), and confirm a deployment marker with today's date and a short-SHA version appears. Also check the response-time graph for a marker annotation.

If markers don't appear: check `/tmp/testbench-ingest-port-forward.log` for port-forward errors, and re-run just the curl calls manually with `-v` to see the actual HTTP response (the script suppresses errors with `-sf` so failures are silent by design — this is the debugging fallback).

- [ ] **Step 4: Commit**

```bash
git add scripts/testbench.sh
git commit -m "feat(testbench): record deployment markers for shop services after deploy"
```

---

## Final Verification

- [ ] Run the full frontend suite once more from a clean state: `cd apps/frontend && npm ci && npx tsc --noEmit && npm test -- --run`
- [ ] Grep for any leftover references to deleted symbols: `grep -rn "QueryFilterInput\|NlqPanel\|signalTabFromFrame\|NlqTabFrame" apps/frontend/src` — expect no matches outside of `CHANGELOG`-style history or comments explicitly discussing the old names (there should be none).
- [ ] Confirm `docs/superpowers/specs/2026-07-19-query-input-merge-and-services-declutter-design.md`'s "Testing" section items are all covered by the tasks above (they are: `detectQueryMode` tests → Task 1; `QueryInput` tests including mode badge and filter/search bypass → Task 3; `ServiceDetailPage` tab-set test → Task 6; Workbench provider regression test → Task 7; testbench manual verification → Task 8).
