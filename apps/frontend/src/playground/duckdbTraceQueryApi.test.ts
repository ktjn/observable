import { describe, expect, it, vi } from "vitest";
import { DuckDbTraceQueryApi } from "./duckdbTraceQueryApi";

describe("DuckDbTraceQueryApi", () => {
  it("executes a planned trace query through the DuckDB boundary", async () => {
    const query = vi.fn().mockResolvedValue({ toArray: () => [{ trace_id: "trace" }] });
    const api = new DuckDbTraceQueryApi({ query });

    await expect(api.executePlanned("SELECT trace_id FROM spans")).resolves.toEqual([
      { trace_id: "trace" },
    ]);
    expect(query).toHaveBeenCalledWith("SELECT trace_id FROM spans");
  });

  it("escapes trace ids before querying the local store", async () => {
    const query = vi.fn().mockResolvedValue({ toArray: () => [] });
    const api = new DuckDbTraceQueryApi({ query });

    await api.findTrace("trace' OR '1'='1");

    expect(query.mock.calls[0][0]).toContain("trace'' OR ''1''=''1");
    expect(query.mock.calls[0][0]).not.toContain("WHERE trace_id = 'trace' OR");
  });
});
