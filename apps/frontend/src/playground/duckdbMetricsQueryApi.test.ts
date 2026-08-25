import { describe, expect, it, vi } from "vitest";
import { DuckDbMetricsQueryApi } from "./duckdbMetricsQueryApi";

describe("DuckDbMetricsQueryApi", () => {
  it("executes planned metric catalog and point reads through DuckDB", async () => {
    const query = vi.fn().mockResolvedValue({ toArray: () => [{ metric_name: "requests" }] });
    const api = new DuckDbMetricsQueryApi({ query });

    await expect(api.executePlanned("SELECT metric_name FROM metric_series")).resolves.toEqual([
      { metric_name: "requests" },
    ]);
    expect(query).toHaveBeenCalledWith("SELECT metric_name FROM metric_series");
  });
});
