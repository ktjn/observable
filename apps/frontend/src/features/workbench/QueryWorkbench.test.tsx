import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { describe, expect, test, vi, afterEach, beforeEach } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import QueryWorkbench from "./QueryWorkbench";
import { TenantContextProvider } from "../../hooks/useTenantContext";

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    useSearch: vi.fn(() => ({})),
    useNavigate: () => vi.fn(),
  };
});

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
  useGlobalDateRange: () => ({
    preset: "1h",
    fromMs: 1_700_000_000_000,
    toMs: 1_700_003_600_000,
    setPreset: vi.fn(),
    setCustomRange: vi.fn(),
    clearCustomRange: vi.fn(),
  }),
}));

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
  TenantContextProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

vi.mock("./NotebookEditor", () => ({
  NotebookEditor: ({
    value,
    mode,
    onChange,
  }: {
    value: string;
    mode: "nlq" | "raw";
    onChange: (value: string) => void;
  }) => (
    <textarea
      aria-label={mode === "raw" ? "Raw IR editor" : "Natural language editor"}
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  ),
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

const FRAME_RESPONSE = {
  type: "frame" as const,
  frame: {
    frame_type: "timeseries" as const,
    x_field: "bucket",
    y_field: "value",
    series_field: null,
    unit: "ms",
    suggested_visualization: "timeseries" as const,
    field_roles: [
      { name: "bucket", role: "time" as const },
      { name: "value", role: "value" as const },
    ],
    data: [{ bucket: "2026-06-01 10:00:00", value: 120.5 }],
    nlq_ir: {
      operation: "timeseries" as const,
      signals: ["metrics" as const],
      filters: [],
      group_by: [],
      time_range: { from: "1700000000000000000", to: "1700003600000000000" },
      metric: "latency_ms",
      window: null,
      resolution: null,
      visualization_hint: null,
    },
    source_sql: "SELECT bucket, avg(value) FROM ...",
    time_range: { from: "1700000000000000000", to: "1700003600000000000" },
    signal_types: ["metrics" as const],
    sample_rate: null,
    approximation_statement:
      "Advisory result for now-1h to now. This result is approximate and must not be used for billing.",
  },
};

const DECLINE_RESPONSE = {
  type: "decline" as const,
  reason: "This question is outside the NLQ scope",
};

afterEach(() => {
  vi.clearAllMocks();
});

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

function renderWorkbench() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  function wrapper({ children }: { children: React.ReactNode }) {
    return (
      <TenantContextProvider>
        <QueryClientProvider client={qc}>{children}</QueryClientProvider>
      </TenantContextProvider>
    );
  }
  return render(<QueryWorkbench />, { wrapper });
}

describe("QueryWorkbench", () => {
  test("renders the fixed three-block starter notebook", () => {
    renderWorkbench();

    expect(screen.getByRole("heading", { name: "Query Workbench" })).toBeInTheDocument();
    expect(screen.getByTestId("workbench-block-metrics")).toBeInTheDocument();
    expect(screen.getByTestId("workbench-block-logs")).toBeInTheDocument();
    expect(screen.getByTestId("workbench-block-traces")).toBeInTheDocument();
    expect(within(screen.getByTestId("workbench-block-metrics")).getByRole("textbox")).toHaveValue("");
  });

  test("runs the metrics block against the metrics base IR and renders the result", async () => {
    let resolveQuery!: (value: typeof FRAME_RESPONSE) => void;
    mockSubmit.mockReturnValue(
      new Promise((resolve) => {
        resolveQuery = resolve;
      }),
    );
    renderWorkbench();

    const metricsBlock = screen.getByTestId("workbench-block-metrics");
    fireEvent.change(within(metricsBlock).getByRole("textbox"), {
      target: { value: "p95 latency" },
    });
    fireEvent.click(within(metricsBlock).getByTestId("workbench-run-metrics"));

    expect(await within(metricsBlock).findByTestId("workbench-results-loading")).toBeInTheDocument();
    resolveQuery(FRAME_RESPONSE);
    await waitFor(() => expect(within(metricsBlock).getByTestId("workbench-results-frame")).toBeInTheDocument());

    expect(mockSubmit).toHaveBeenCalledWith(
      "test-tenant",
      expect.objectContaining({
        mode: "execute",
        question: "p95 latency",
          base_ir: expect.objectContaining({
            operation: "catalog",
            signals: ["metrics"],
            time_range: {
              from: "1700000000000000000",
              to: "1700003600000000000",
            },
          }),
      }),
    );
    expect(within(metricsBlock).getByTestId("workbench-question")).toHaveTextContent("p95 latency");
    expect(within(metricsBlock).getByTestId("workbench-approximation")).toHaveTextContent("approximate");
  });

  test("keeps block runtime state independent across signals", async () => {
    mockSubmit.mockImplementation(async (_tenantId, request) => {
      if (request.question === "p95 latency") return FRAME_RESPONSE;
      return DECLINE_RESPONSE;
    });

    renderWorkbench();

    const metricsBlock = screen.getByTestId("workbench-block-metrics");
    const logsBlock = screen.getByTestId("workbench-block-logs");

    fireEvent.change(within(metricsBlock).getByRole("textbox"), {
      target: { value: "p95 latency" },
    });
    fireEvent.click(within(metricsBlock).getByTestId("workbench-run-metrics"));
    await waitFor(() => expect(within(metricsBlock).getByTestId("workbench-results-frame")).toBeInTheDocument());

    fireEvent.change(within(logsBlock).getByRole("textbox"), {
      target: { value: "error logs" },
    });
    fireEvent.click(within(logsBlock).getByTestId("workbench-run-logs"));
    await waitFor(() => expect(within(logsBlock).getByTestId("workbench-results-decline")).toBeInTheDocument());

    expect(within(metricsBlock).getByTestId("workbench-results-frame")).toBeInTheDocument();
    expect(within(logsBlock).getByTestId("workbench-results-decline")).toHaveTextContent("outside the NLQ scope");
  });

  test("switching a block to raw mode preserves the draft and submits raw JSON directly", async () => {
    mockSubmit.mockResolvedValue(FRAME_RESPONSE);
    renderWorkbench();

    const metricsBlock = screen.getByTestId("workbench-block-metrics");
    const editor = within(metricsBlock).getByRole("textbox");
    fireEvent.change(editor, {
      target: { value: '{"metric":"latency_ms"}' },
    });
    fireEvent.click(within(metricsBlock).getByRole("button", { name: "Raw" }));

    expect(within(metricsBlock).getByLabelText("Raw IR editor")).toHaveValue('{"metric":"latency_ms"}');

    fireEvent.click(within(metricsBlock).getByTestId("workbench-run-metrics"));
    await waitFor(() => expect(within(metricsBlock).getByTestId("workbench-results-frame")).toBeInTheDocument());

    expect(mockSubmit).toHaveBeenCalledWith(
      "test-tenant",
      expect.objectContaining({
        mode: "execute",
        base_ir: expect.objectContaining({
          operation: "catalog",
          signals: ["metrics"],
        }),
      }),
    );
    expect(mockSubmit.mock.calls[0]?.[1].question).toBeUndefined();
  });

  test("invalid raw JSON blocks submission and shows a validation message", async () => {
    renderWorkbench();

    const metricsBlock = screen.getByTestId("workbench-block-metrics");
    fireEvent.click(within(metricsBlock).getByRole("button", { name: "Raw" }));
    fireEvent.change(within(metricsBlock).getByLabelText("Raw IR editor"), {
      target: { value: "{not valid json" },
    });
    fireEvent.click(within(metricsBlock).getByTestId("workbench-run-metrics"));

    expect(within(metricsBlock).getByTestId("workbench-results-error")).toHaveTextContent(
      "Raw mode expects valid JSON.",
    );
    expect(mockSubmit).not.toHaveBeenCalled();
  });

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
});
