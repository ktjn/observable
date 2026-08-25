import { describe, expect, it, vi } from "vitest";
import { playgroundRuntime } from "./playgroundRuntime";

// Regression test for the "locked-service" bug found via the live Service
// Detail page: LogSearch.tsx/TraceSearch.tsx pre-set a service filter by
// JSON-encoding a raw NlqIr as `question` (the "Simple IR Shorthand" power-
// user bypass, ADR-034) rather than leaving `question` unset. Before this
// fix, playgroundRuntime.nlq.execute treated any non-empty `question` as
// free text and fell through to fixture data shaped for the wrong signal.
// It must instead detect the raw-IR shorthand, merge it into `base_ir`
// (mirroring `llm_adapter.rs::merge_irs`), resolve any still-relative
// "now"/"now-Xh" time_range before it reaches the DuckDB dialect renderer
// (which doesn't understand ClickHouse's `parse_time_expr` output), and
// route to the real engine exactly as the unlocked page-load path does.
vi.mock("../playground/engineClient", () => ({
  executeTraceTable: vi.fn(async (ir: unknown) => ({
    rows: [],
    sql: "-- mocked",
    __ir: ir,
  })),
  executeLogTable: vi.fn(async (ir: unknown) => ({
    rows: [],
    sql: "-- mocked",
    __ir: ir,
  })),
}));

const TENANT_ID = "00000000-0000-0000-0000-000000000001";

describe("playgroundRuntime.nlq.execute — locked-service raw IR shorthand", () => {
  it("merges a raw-IR question's service filter into base_ir for a traces table query", async () => {
    const { executeTraceTable } = await import("../playground/engineClient");
    const baseIr = {
      operation: "table",
      signals: ["traces"],
      filters: [],
      time_range: { from: "1700000000000000000", to: "1700003600000000000" },
    };
    const rawIrQuestion = JSON.stringify({
      ...baseIr,
      filters: [{ field: "service_name", op: "=", value: "checkout" }],
    });

    await playgroundRuntime.nlq.execute(TENANT_ID, {
      question: rawIrQuestion,
      mode: "execute",
      base_ir: baseIr,
    });

    expect(executeTraceTable).toHaveBeenCalledTimes(1);
    const calledWithIr = vi.mocked(executeTraceTable).mock.calls[0][0] as {
      filters: Array<{ field: string; value: string }>;
      time_range: { from: string; to: string };
    };
    expect(calledWithIr.filters).toEqual([{ field: "service_name", op: "=", value: "checkout" }]);
    // Base's already-resolved absolute-ns time_range wins because the raw
    // IR's own time_range came from the base too — both are digit strings.
    expect(calledWithIr.time_range).toEqual(baseIr.time_range);
  });

  it("resolves a still-relative now-1h time_range to absolute nanoseconds before querying", async () => {
    const { executeLogTable } = await import("../playground/engineClient");
    const baseIr = {
      operation: "table",
      signals: ["logs"],
      filters: [],
      time_range: { from: "1700000000000000000", to: "1700003600000000000" },
    };
    // Mirrors LogSearch.tsx's LOG_BASE_IR: relative time_range baked into the
    // locked-service raw-IR question, distinct from the page's real base_ir.
    const rawIrQuestion = JSON.stringify({
      operation: "table",
      signals: ["logs"],
      filters: [{ field: "service_name", op: "=", value: "web" }],
      time_range: { from: "now-1h", to: "now" },
    });

    await playgroundRuntime.nlq.execute(TENANT_ID, {
      question: rawIrQuestion,
      mode: "execute",
      base_ir: baseIr,
    });

    expect(executeLogTable).toHaveBeenCalledTimes(1);
    const calledWithIr = vi.mocked(executeLogTable).mock.calls[0][0] as {
      time_range: { from: string; to: string };
    };
    // Must be plain nanosecond-epoch decimal strings, never a
    // ClickHouse-flavored expression like "toUnixTimestamp64Nano(now64())".
    expect(calledWithIr.time_range.from).toMatch(/^\d+$/);
    expect(calledWithIr.time_range.to).toMatch(/^\d+$/);
    expect(BigInt(calledWithIr.time_range.to) - BigInt(calledWithIr.time_range.from)).toBe(
      3_600_000_000_000n,
    );
  });

  it("reports genuine free-text questions as an unsupported local capability", async () => {
    const { executeTraceTable, executeLogTable } = await import("../playground/engineClient");
    vi.mocked(executeTraceTable).mockClear();
    vi.mocked(executeLogTable).mockClear();

    const response = await playgroundRuntime.nlq.execute(TENANT_ID, {
      question: "checkout errors in prod",
      mode: "execute",
      base_ir: {
        operation: "table",
        signals: ["traces"],
        filters: [],
        time_range: { from: "1700000000000000000", to: "1700003600000000000" },
      },
    });

    expect(executeTraceTable).not.toHaveBeenCalled();
    expect(executeLogTable).not.toHaveBeenCalled();
    expect(response.type).toBe("capabilities");
  });
});
