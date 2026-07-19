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
