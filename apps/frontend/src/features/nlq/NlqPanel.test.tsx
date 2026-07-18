import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, describe, test, expect, afterEach, beforeEach } from "vitest";
import { NlqPanel } from "./NlqPanel";
import type { NlqResponse } from "../../api/nlq";
import { TenantContextProvider } from "../../hooks/useTenantContext";

// ── Mocks ─────────────────────────────────────────────────────────────────────

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
  webllm_model: "Llama-3-8B-Instruct-q4f16_1",
};

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <TenantContextProvider>
      <QueryClientProvider client={qc}>{children}</QueryClientProvider>
    </TenantContextProvider>
  );
}

const FRAME_RESPONSE: NlqResponse = {
  type: "frame",
  frame: {
    frame_type: "timeseries",
    x_field: "bucket",
    y_field: "value",
    series_field: null,
    unit: "ms",
    suggested_visualization: "timeseries",
    field_roles: [
      { name: "bucket", role: "time" },
      { name: "value", role: "value" },
    ],
    data: [{ bucket: "2026-01-01 10:00:00", value: 120.5 }],
    nlq_ir: {
      operation: "timeseries",
      signals: ["metrics"],
      filters: [],
      group_by: [],
      time_range: { from: "now-1h", to: "now" },
      metric: "latency_ms",
      window: null,
      resolution: null,
      visualization_hint: null,
    },
    source_sql: "SELECT bucket, avg(value) FROM ...",
    time_range: { from: "now-1h", to: "now" },
    signal_types: ["metrics"],
    sample_rate: null,
    approximation_statement:
      "Advisory result for now-1h to now. This result is approximate and must not be used for billing.",
  },
};

const DECLINE_RESPONSE: NlqResponse = {
  type: "decline",
  reason: "This question involves billing and financial reconciliation.",
};

const INVALID_RESPONSE: NlqResponse = {
  type: "invalid_response",
  reason: "LLM response could not be parsed as NlqIr",
  raw_llm_response: '{"type": "unknown", "data": {}}',
};

beforeEach(() => {
  // Default: remote provider, matching pre-Task-5 behavior for tests that
  // don't care about provider selection.
  mockGetConfig.mockResolvedValue(REMOTE_CONFIG);
});

afterEach(() => {
  vi.clearAllMocks();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("NlqPanel", () => {
  test("submit button shows 'Ask' label in idle state", () => {
    render(<NlqPanel />, { wrapper });
    expect(screen.getByTestId("nlq-submit")).toHaveTextContent("Ask");
  });

  test("renders query input and submit button", () => {
    render(<NlqPanel />, { wrapper });
    expect(screen.getByTestId("nlq-input")).toBeInTheDocument();
    expect(screen.getByTestId("nlq-submit")).toBeInTheDocument();
  });

  test("submit button is disabled when input is empty", () => {
    render(<NlqPanel />, { wrapper });
    expect(screen.getByTestId("nlq-submit")).toBeDisabled();
  });

  test("submit button enables when input has text", async () => {
    render(<NlqPanel />, { wrapper });
    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "p99 latency" },
    });
    expect(screen.getByTestId("nlq-submit")).not.toBeDisabled();
  });

  test("shows loading state while query is in flight", async () => {
    let resolveQuery!: (v: NlqResponse) => void;
    mockSubmit.mockReturnValue(
      new Promise<NlqResponse>((res) => {
        resolveQuery = res;
      })
    );

    render(<NlqPanel />, { wrapper });
    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "p99 latency" },
    });
    fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);

    expect(await screen.findByText("Querying…")).toBeInTheDocument();
    resolveQuery(FRAME_RESPONSE);
  });

  test("renders visualization frame after successful query", async () => {
    mockSubmit.mockResolvedValue(FRAME_RESPONSE);
    render(<NlqPanel />, { wrapper });
    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "p99 latency" },
    });
    fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);

    await waitFor(() =>
      expect(screen.getByTestId("nlq-result")).toBeInTheDocument()
    );
    expect(screen.getByTestId("viz-panel")).toBeInTheDocument();
  });

  test("always shows approximation statement when frame is returned", async () => {
    mockSubmit.mockResolvedValue(FRAME_RESPONSE);
    render(<NlqPanel />, { wrapper });
    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "p99 latency" },
    });
    fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);

    await waitFor(() =>
      expect(screen.getByTestId("nlq-approximation")).toBeInTheDocument()
    );
    expect(screen.getByTestId("nlq-approximation")).toHaveTextContent(
      "billing"
    );
  });

  test("hides source SQL and NLQ IR until Show details clicked", async () => {
    mockSubmit.mockResolvedValue(FRAME_RESPONSE);
    render(<NlqPanel />, { wrapper });
    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "p99 latency" },
    });
    fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);

    await waitFor(() =>
      expect(screen.getByTestId("nlq-show-details")).toBeInTheDocument()
    );

    // Provenance section hidden initially
    expect(screen.queryByTestId("nlq-provenance")).not.toBeVisible();

    // Click to expand
    fireEvent.click(screen.getByTestId("nlq-show-details"));
    await waitFor(() =>
      expect(screen.getByTestId("nlq-provenance")).toBeVisible()
    );
    expect(screen.getByTestId("nlq-provenance")).toHaveTextContent(
      "SELECT bucket"
    );
  });

  test("provenance shows NLQ question first", async () => {
    mockSubmit.mockResolvedValue(FRAME_RESPONSE);
    render(<NlqPanel />, { wrapper });
    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "p99 latency last hour" },
    });
    fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);

    await waitFor(() =>
      expect(screen.getByTestId("nlq-show-details")).toBeInTheDocument()
    );
    fireEvent.click(screen.getByTestId("nlq-show-details"));
    await waitFor(() =>
      expect(screen.getByTestId("nlq-provenance")).toBeVisible()
    );

    expect(screen.getByTestId("nlq-question")).toHaveTextContent(
      "p99 latency last hour"
    );
  });

  test("provenance order: NLQ question, NLQ IR, SQL, time range, signals", async () => {
    mockSubmit.mockResolvedValue(FRAME_RESPONSE);
    render(<NlqPanel />, { wrapper });
    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "p99 latency" },
    });
    fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);

    await waitFor(() =>
      expect(screen.getByTestId("nlq-show-details")).toBeInTheDocument()
    );
    fireEvent.click(screen.getByTestId("nlq-show-details"));
    await waitFor(() =>
      expect(screen.getByTestId("nlq-provenance")).toBeVisible()
    );

    const provenance = screen.getByTestId("nlq-provenance");
    const text = provenance.textContent ?? "";
    const nlqIdx = text.indexOf("NLQ:");
    const irIdx = text.indexOf("NLQ IR:");
    const sqlIdx = text.indexOf("SQL:");
    const timeIdx = text.indexOf("Time range:");
    expect(nlqIdx).toBeLessThan(irIdx);
    expect(irIdx).toBeLessThan(sqlIdx);
    expect(sqlIdx).toBeLessThan(timeIdx);
  });

  test("renders decline message with reason", async () => {
    mockSubmit.mockResolvedValue(DECLINE_RESPONSE);
    render(<NlqPanel />, { wrapper });
    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "total billing this month" },
    });
    fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);

    await waitFor(() =>
      expect(screen.getByTestId("nlq-decline")).toBeInTheDocument()
    );
    expect(screen.getByTestId("nlq-decline")).toHaveTextContent("billing");
  });

  test("renders error message on API failure", async () => {
    mockSubmit.mockRejectedValue(new Error("NLQ service is not configured"));
    render(<NlqPanel />, { wrapper });
    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "latency" },
    });
    fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);

    await waitFor(() =>
      expect(screen.getByTestId("nlq-error")).toBeInTheDocument()
    );
    expect(screen.getByTestId("nlq-error")).toHaveTextContent(
      "NLQ service is not configured"
    );
  });

  test("passes service_name to API when provided", async () => {
    mockSubmit.mockResolvedValue(DECLINE_RESPONSE);
    render(<NlqPanel serviceName="checkout" />, { wrapper });
    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "latency" },
    });
    fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);

    await waitFor(() => expect(mockSubmit).toHaveBeenCalledOnce());
    expect(mockSubmit).toHaveBeenCalledWith(DEFAULT_TENANT_ID, {
      question: "latency",
      service_name: "checkout",
    });
  });

  test("renders invalid response panel with reason and raw LLM response", async () => {
    mockSubmit.mockResolvedValue(INVALID_RESPONSE);
    render(<NlqPanel />, { wrapper });
    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "something confusing" },
    });
    fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);

    await waitFor(() =>
      expect(screen.getByTestId("nlq-invalid-response")).toBeInTheDocument()
    );
    expect(screen.getByTestId("nlq-invalid-response")).toHaveTextContent(
      "Could not interpret the LLM response"
    );
    expect(screen.getByTestId("nlq-invalid-response")).toHaveTextContent(
      "LLM response could not be parsed"
    );
  });

  test("invalid response panel shows raw LLM text in expandable details", async () => {
    mockSubmit.mockResolvedValue(INVALID_RESPONSE);
    render(<NlqPanel />, { wrapper });
    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "something confusing" },
    });
    fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);

    await waitFor(() =>
      expect(screen.getByTestId("nlq-invalid-response")).toBeInTheDocument()
    );
    fireEvent.click(screen.getByText("Show raw LLM response"));
    expect(screen.getByTestId("nlq-raw-llm-response")).toHaveTextContent(
      '"type": "unknown"'
    );
  });

  test("reset button clears the query text and result", async () => {
    mockSubmit.mockResolvedValue(FRAME_RESPONSE);
    render(<NlqPanel />, { wrapper });

    expect(screen.queryByTestId("nlq-reset")).not.toBeInTheDocument();

    fireEvent.change(screen.getByTestId("nlq-input"), {
      target: { value: "p99 latency" },
    });
    fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);
    await waitFor(() => expect(screen.getByTestId("nlq-result")).toBeInTheDocument());

    fireEvent.click(screen.getByTestId("nlq-reset"));

    expect(screen.getByTestId("nlq-input")).toHaveValue("");
    expect(screen.queryByTestId("nlq-result")).not.toBeInTheDocument();
    expect(screen.queryByTestId("nlq-reset")).not.toBeInTheDocument();
  });
});

// ── WebLLM provider ──────────────────────────────────────────────────────────

function submitQuestion(text: string) {
  fireEvent.change(screen.getByTestId("nlq-input"), { target: { value: text } });
  fireEvent.submit(screen.getByTestId("nlq-input").closest("form")!);
}

describe("NlqPanel — WebLLM provider", () => {
  test("WebGPU unsupported fails closed with a visible error; prepare/complete never called", async () => {
    mockGetConfig.mockResolvedValue(WEBLLM_CONFIG);
    mockCheckWebGpuSupport.mockResolvedValue({
      supported: false,
      reason: "No compatible GPU adapter found",
    });

    render(<NlqPanel />, { wrapper });
    await waitFor(() => expect(mockGetConfig).toHaveBeenCalled());
    submitQuestion("p99 latency");

    await waitFor(() => expect(screen.getByTestId("nlq-error")).toBeInTheDocument());
    expect(screen.getByTestId("nlq-error")).toHaveTextContent(
      "WebLLM is configured but this browser doesn't support it"
    );
    expect(screen.getByTestId("nlq-error")).toHaveTextContent("No compatible GPU adapter found");
    expect(mockPrepare).not.toHaveBeenCalled();
    expect(mockComplete).not.toHaveBeenCalled();
    expect(mockGetOrCreateEngine).not.toHaveBeenCalled();
    // No silent fallback to the remote path either.
    expect(mockSubmit).not.toHaveBeenCalled();
  });

  test("prepare returning 'final' directly renders the result without invoking the engine", async () => {
    mockGetConfig.mockResolvedValue(WEBLLM_CONFIG);
    mockCheckWebGpuSupport.mockResolvedValue({ supported: true });
    mockPrepare.mockResolvedValue({ type: "final", response: DECLINE_RESPONSE });

    render(<NlqPanel />, { wrapper });
    await waitFor(() => expect(mockGetConfig).toHaveBeenCalled());
    submitQuestion("total billing this month");

    await waitFor(() => expect(screen.getByTestId("nlq-decline")).toBeInTheDocument());
    expect(mockGetOrCreateEngine).not.toHaveBeenCalled();
    expect(mockComplete).not.toHaveBeenCalled();
  });

  test("missing webllm_model config fails closed without calling the engine", async () => {
    mockGetConfig.mockResolvedValue({ ...WEBLLM_CONFIG, webllm_model: null });
    mockCheckWebGpuSupport.mockResolvedValue({ supported: true });
    mockPrepare.mockResolvedValue({
      type: "prepared",
      session_token: "session-1",
      system_prompt: "system prompt",
      question: "p99 latency",
    });

    render(<NlqPanel />, { wrapper });
    await waitFor(() => expect(mockGetConfig).toHaveBeenCalled());
    submitQuestion("p99 latency");

    await waitFor(() => expect(screen.getByTestId("nlq-error")).toBeInTheDocument());
    expect(screen.getByTestId("nlq-error")).toHaveTextContent("No WebLLM model configured");
    expect(mockGetOrCreateEngine).not.toHaveBeenCalled();
    expect(mockComplete).not.toHaveBeenCalled();
  });

  test("happy path: prepare -> engine -> complete -> final", async () => {
    mockGetConfig.mockResolvedValue(WEBLLM_CONFIG);
    mockCheckWebGpuSupport.mockResolvedValue({ supported: true });
    mockPrepare.mockResolvedValue({
      type: "prepared",
      session_token: "session-1",
      system_prompt: "system prompt",
      question: "p99 latency",
    });
    const mockComplete_ = vi.fn().mockResolvedValue("raw llm output");
    mockGetOrCreateEngine.mockResolvedValue({
      complete: mockComplete_,
      dispose: vi.fn(),
    });
    mockComplete.mockResolvedValue({ type: "final", response: FRAME_RESPONSE });

    render(<NlqPanel />, { wrapper });
    await waitFor(() => expect(mockGetConfig).toHaveBeenCalled());
    submitQuestion("p99 latency");

    await waitFor(() => expect(screen.getByTestId("nlq-result")).toBeInTheDocument());
    expect(mockPrepare).toHaveBeenCalledWith(DEFAULT_TENANT_ID, {
      question: "p99 latency",
      service_name: undefined,
    });
    expect(mockGetOrCreateEngine).toHaveBeenCalledWith(
      "Llama-3-8B-Instruct-q4f16_1",
      expect.any(Function)
    );
    expect(mockComplete_).toHaveBeenCalledWith("system prompt", "p99 latency");
    expect(mockComplete).toHaveBeenCalledWith(DEFAULT_TENANT_ID, "session-1", "raw llm output");
    expect(screen.getByTestId("viz-panel")).toBeInTheDocument();
  });

  test("needs_repair round-trip: engine invoked again with the repair prompt and same system prompt", async () => {
    mockGetConfig.mockResolvedValue(WEBLLM_CONFIG);
    mockCheckWebGpuSupport.mockResolvedValue({ supported: true });
    mockPrepare.mockResolvedValue({
      type: "prepared",
      session_token: "session-1",
      system_prompt: "system prompt",
      question: "p99 latency",
    });
    const mockEngineComplete = vi
      .fn()
      .mockResolvedValueOnce("bad raw output")
      .mockResolvedValueOnce("fixed raw output");
    mockGetOrCreateEngine.mockResolvedValue({
      complete: mockEngineComplete,
      dispose: vi.fn(),
    });
    mockComplete
      .mockResolvedValueOnce({ type: "needs_repair", repair_prompt: "please emit valid JSON" })
      .mockResolvedValueOnce({ type: "final", response: FRAME_RESPONSE });

    render(<NlqPanel />, { wrapper });
    await waitFor(() => expect(mockGetConfig).toHaveBeenCalled());
    submitQuestion("p99 latency");

    await waitFor(() => expect(screen.getByTestId("nlq-result")).toBeInTheDocument());

    expect(mockEngineComplete).toHaveBeenCalledTimes(2);
    expect(mockEngineComplete).toHaveBeenNthCalledWith(1, "system prompt", "p99 latency");
    expect(mockEngineComplete).toHaveBeenNthCalledWith(
      2,
      "system prompt",
      "please emit valid JSON"
    );
    expect(mockComplete).toHaveBeenCalledTimes(2);
    expect(mockComplete).toHaveBeenNthCalledWith(1, DEFAULT_TENANT_ID, "session-1", "bad raw output");
    expect(mockComplete).toHaveBeenNthCalledWith(
      2,
      DEFAULT_TENANT_ID,
      "session-1",
      "fixed raw output"
    );
  });

  test("repair loop hits the hard ceiling and surfaces an error instead of hanging forever", async () => {
    mockGetConfig.mockResolvedValue(WEBLLM_CONFIG);
    mockCheckWebGpuSupport.mockResolvedValue({ supported: true });
    mockPrepare.mockResolvedValue({
      type: "prepared",
      session_token: "session-1",
      system_prompt: "system prompt",
      question: "p99 latency",
    });
    mockGetOrCreateEngine.mockResolvedValue({
      complete: vi.fn().mockResolvedValue("bad raw output"),
      dispose: vi.fn(),
    });
    // Simulate a server that never converges (backend bug) — always needs_repair.
    mockComplete.mockResolvedValue({ type: "needs_repair", repair_prompt: "please emit valid JSON" });

    render(<NlqPanel />, { wrapper });
    await waitFor(() => expect(mockGetConfig).toHaveBeenCalled());
    submitQuestion("p99 latency");

    await waitFor(() => expect(screen.getByTestId("nlq-error")).toBeInTheDocument(), {
      timeout: 5000,
    });
    expect(screen.queryByTestId("nlq-result")).not.toBeInTheDocument();
  });
});
