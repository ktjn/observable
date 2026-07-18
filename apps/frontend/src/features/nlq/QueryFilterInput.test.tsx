import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { QueryFilterInput } from "./QueryFilterInput";
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

beforeEach(() => {
  mockGetConfig.mockResolvedValue(REMOTE_CONFIG);
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("QueryFilterInput", () => {
  test("submits natural language in interpret mode and calls onSubmit with raw text and onIr with IR", async () => {
    const onIr = vi.fn();
    const onSubmit = vi.fn();
    mockSubmit.mockResolvedValue({
      type: "ir",
      ir: {
        operation: "catalog",
        signals: ["metrics"],
        filters: [{ field: "service_name", op: "=", value: "checkout" }],
        group_by: [],
        time_range: { from: "now-1h", to: "now" },
        metric: null,
        window: null,
        resolution: null,
        visualization_hint: null,
      },
    });

    render(<QueryFilterInput onIr={onIr} onSubmit={onSubmit} baseIr={SERVICES_BASE_IR} />, { wrapper });
    fireEvent.change(screen.getByRole("textbox", { name: "Query current view input" }), {
      target: { value: "show checkout services" },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));

    await waitFor(() =>
      expect(mockSubmit).toHaveBeenCalledWith(DEFAULT_TENANT_ID, {
        question: "show checkout services",
        mode: "interpret",
        service_name: undefined,
        base_ir: expect.objectContaining({
          operation: "catalog",
          signals: ["metrics"],
          time_range: expect.objectContaining({
            from: expect.stringMatching(/^\d{15,}$/),
            to: expect.stringMatching(/^\d{15,}$/),
          }),
        }),
      }),
    );
    expect(onSubmit).toHaveBeenCalledWith("show checkout services");
    expect(onIr).toHaveBeenCalledWith({
      operation: "catalog",
      signals: ["metrics"],
      filters: [{ field: "service_name", op: "=", value: "checkout" }],
      group_by: [],
      time_range: { from: "now-1h", to: "now" },
      metric: null,
      window: null,
      resolution: null,
      visualization_hint: null,
    });
  });

  test("raw IR JSON uses the same input and shows query details", async () => {
    const raw = JSON.stringify({
      operation: "catalog",
      signals: ["metrics"],
      filters: [{ field: "environment", op: "=", value: "prod" }],
    });
    mockSubmit.mockResolvedValue({
      type: "ir",
      ir: {
        operation: "catalog",
        signals: ["metrics"],
        filters: [{ field: "environment", op: "=", value: "prod" }],
        group_by: [],
        time_range: { from: "now-1h", to: "now" },
        metric: null,
        window: null,
        resolution: null,
        visualization_hint: null,
      },
    });

    render(<QueryFilterInput onIr={vi.fn()} baseIr={SERVICES_BASE_IR} />, { wrapper });
    fireEvent.change(screen.getByRole("textbox", { name: "Query current view input" }), {
      target: { value: raw },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));

    expect(await screen.findByText("Show interpreted IR")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Show interpreted IR"));
    expect(screen.getByTestId("query-filter-ir")).toHaveTextContent("environment");
  });

  test("reset button clears the query text and notifies onSubmit with an empty string", async () => {
    const onSubmit = vi.fn();
    mockSubmit.mockResolvedValue({
      type: "ir",
      ir: {
        operation: "catalog",
        signals: ["metrics"],
        filters: [],
        group_by: [],
        time_range: { from: "now-1h", to: "now" },
        metric: null,
        window: null,
        resolution: null,
        visualization_hint: null,
      },
    });

    render(<QueryFilterInput onSubmit={onSubmit} baseIr={SERVICES_BASE_IR} />, { wrapper });
    const input = screen.getByRole("textbox", { name: "Query current view input" }) as HTMLInputElement;

    expect(screen.queryByRole("button", { name: /reset/i })).not.toBeInTheDocument();

    fireEvent.change(input, { target: { value: "show checkout services" } });
    fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith("show checkout services"));

    fireEvent.click(screen.getByRole("button", { name: /reset/i }));

    expect(input.value).toBe("");
    expect(onSubmit).toHaveBeenCalledWith("");
    expect(screen.queryByRole("button", { name: /reset/i })).not.toBeInTheDocument();
  });

  // Regression test: QueryFilterInput is a second, independent NLQ-submitting
  // surface (used on Log Search and other structured pages) alongside NlqPanel.
  // It must route through the same provider-aware two-phase WebLLM flow instead
  // of always calling the remote-only submitNlqQuery — this test would have
  // caught the bug where it didn't.
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
    mockComplete.mockResolvedValue({
      type: "final",
      response: {
        type: "ir",
        ir: {
          operation: "catalog",
          signals: ["metrics"],
          filters: [{ field: "service_name", op: "=", value: "checkout" }],
          group_by: [],
          time_range: { from: "now-1h", to: "now" },
          metric: null,
          window: null,
          resolution: null,
          visualization_hint: null,
        },
      },
    });

    render(<QueryFilterInput baseIr={SERVICES_BASE_IR} />, { wrapper });
    await waitFor(() => expect(mockGetConfig).toHaveBeenCalled());
    fireEvent.change(screen.getByRole("textbox", { name: "Query current view input" }), {
      target: { value: "show checkout services" },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));

    await waitFor(() => expect(mockPrepare).toHaveBeenCalled());
    expect(mockSubmit).not.toHaveBeenCalled();
    expect(screen.getByTestId("query-filter-ir")).toBeDefined();
  });
});
