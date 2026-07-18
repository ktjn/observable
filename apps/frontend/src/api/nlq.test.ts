import { describe, test, expect, vi, afterEach } from "vitest";
import { prepareNlqQuery, completeNlqQuery } from "./nlq";
import type { NlqResponse } from "./nlq";

const DEFAULT_TENANT_ID = "00000000-0000-0000-0000-000000000001";

const FRAME_RESPONSE: NlqResponse = {
  type: "frame",
  frame: {
    frame_type: "timeseries",
    x_field: "bucket",
    y_field: "value",
    series_field: null,
    unit: "ms",
    suggested_visualization: "timeseries",
    field_roles: [],
    data: [],
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
    source_sql: "SELECT 1",
    time_range: { from: "now-1h", to: "now" },
    signal_types: ["metrics"],
    sample_rate: null,
    approximation_statement: "approx",
  },
};

function mockFetchOnce(status: number, body: unknown) {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue({
      ok: status >= 200 && status < 300,
      status,
      json: async () => body,
    })
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

describe("prepareNlqQuery", () => {
  test("returns a 'final' result", async () => {
    mockFetchOnce(200, { type: "final", response: FRAME_RESPONSE });
    const result = await prepareNlqQuery(DEFAULT_TENANT_ID, { question: "p99 latency" });
    expect(result).toEqual({ type: "final", response: FRAME_RESPONSE });
  });

  test("returns a 'prepared' result", async () => {
    mockFetchOnce(200, {
      type: "prepared",
      session_token: "session-1",
      system_prompt: "You are an NLQ assistant.",
      question: "p99 latency",
    });
    const result = await prepareNlqQuery(DEFAULT_TENANT_ID, { question: "p99 latency" });
    expect(result).toEqual({
      type: "prepared",
      session_token: "session-1",
      system_prompt: "You are an NLQ assistant.",
      question: "p99 latency",
    });
  });

  test("throws a descriptive error on non-ok response", async () => {
    mockFetchOnce(400, { error: "bad request" });
    await expect(
      prepareNlqQuery(DEFAULT_TENANT_ID, { question: "p99 latency" })
    ).rejects.toThrow("bad request");
  });

  test("throws a specific error on 503", async () => {
    mockFetchOnce(503, {});
    await expect(
      prepareNlqQuery(DEFAULT_TENANT_ID, { question: "p99 latency" })
    ).rejects.toThrow("NLQ service is not configured on this server");
  });

  test("sends the request via POST with tenant headers", async () => {
    mockFetchOnce(200, { type: "final", response: FRAME_RESPONSE });
    await prepareNlqQuery(DEFAULT_TENANT_ID, { question: "p99 latency", service_name: "checkout" });
    expect(fetch).toHaveBeenCalledWith(
      "/v1/nlq/prepare",
      expect.objectContaining({
        credentials: "include",
        method: "POST",
        headers: expect.objectContaining({
          "Content-Type": "application/json",
          "X-Tenant-ID": DEFAULT_TENANT_ID,
        }),
        body: JSON.stringify({ question: "p99 latency", service_name: "checkout" }),
      })
    );
  });
});

describe("completeNlqQuery", () => {
  test("returns a 'final' result", async () => {
    mockFetchOnce(200, { type: "final", response: FRAME_RESPONSE });
    const result = await completeNlqQuery(DEFAULT_TENANT_ID, "session-1", "raw response");
    expect(result).toEqual({ type: "final", response: FRAME_RESPONSE });
  });

  test("returns a 'needs_repair' result", async () => {
    mockFetchOnce(200, { type: "needs_repair", repair_prompt: "please fix your JSON" });
    const result = await completeNlqQuery(DEFAULT_TENANT_ID, "session-1", "not json");
    expect(result).toEqual({ type: "needs_repair", repair_prompt: "please fix your JSON" });
  });

  test("throws a descriptive error on non-ok response", async () => {
    mockFetchOnce(404, { error: "unknown or expired NLQ session" });
    await expect(
      completeNlqQuery(DEFAULT_TENANT_ID, "session-1", "raw response")
    ).rejects.toThrow("unknown or expired NLQ session");
  });

  test("sends session_token and raw_llm_response in the body", async () => {
    mockFetchOnce(200, { type: "final", response: FRAME_RESPONSE });
    await completeNlqQuery(DEFAULT_TENANT_ID, "session-1", "raw response");
    expect(fetch).toHaveBeenCalledWith(
      "/v1/nlq/complete",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({ session_token: "session-1", raw_llm_response: "raw response" }),
      })
    );
  });
});
