import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { vi, describe, test, expect, afterEach } from "vitest";
import { NlqPanel } from "./NlqPanel";
import type { NlqResponse } from "../../api/nlq";
import { TenantContextProvider } from "../../hooks/useTenantContext";

// ── Mocks ─────────────────────────────────────────────────────────────────────

vi.mock("../../api/nlq", () => ({
  submitNlqQuery: vi.fn(),
}));

import { submitNlqQuery } from "../../api/nlq";
const mockSubmit = vi.mocked(submitNlqQuery);

const DEFAULT_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function wrapper({ children }: { children: React.ReactNode }) {
  return <TenantContextProvider>{children}</TenantContextProvider>;
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
    nlq_ir: { operation: "timeseries", metric: "latency_ms" },
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
});
