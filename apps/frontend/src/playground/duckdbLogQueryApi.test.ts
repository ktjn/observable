import { describe, expect, it, vi } from "vitest";
import { DuckDbLogQueryApi } from "./duckdbLogQueryApi";

describe("DuckDbLogQueryApi", () => {
  it("executes planned log queries through the DuckDB boundary", async () => {
    const query = vi.fn().mockResolvedValue({ toArray: () => [{ log_id: "log" }] });
    const api = new DuckDbLogQueryApi({ query });

    await expect(api.executePlanned("SELECT log_id FROM logs")).resolves.toEqual([{ log_id: "log" }]);
    expect(query).toHaveBeenCalledWith("SELECT log_id FROM logs");
  });

  it("escapes search filters and clamps the result limit", async () => {
    const query = vi.fn().mockResolvedValue({ toArray: () => [] });
    const api = new DuckDbLogQueryApi({ query });

    await api.search("trace' OR '1'='1", "api's", 0);

    expect(query.mock.calls[0][0]).toContain("trace'' OR ''1''=''1");
    expect(query.mock.calls[0][0]).toContain("service_name = 'api''s'");
    expect(query.mock.calls[0][0]).toContain("LIMIT 1");
  });
});
