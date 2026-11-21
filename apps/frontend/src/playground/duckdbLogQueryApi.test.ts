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

    await api.search({ traceId: "trace' OR '1'='1", service: "api's", limit: 0 });

    expect(query.mock.calls[0][0]).toContain("trace'' OR ''1''=''1");
    expect(query.mock.calls[0][0]).toContain("service_name = 'api''s'");
    expect(query.mock.calls[0][0]).toContain("LIMIT 1");
  });

  it("binds span_id, severity, and the time window into every query", async () => {
    const query = vi.fn().mockResolvedValue({ toArray: () => [] });
    const api = new DuckDbLogQueryApi({ query });

    await api.search({ spanId: "span-1", severity: 13, from: "100", to: "200", limit: 10 });

    const [rowsSql, countSql] = [query.mock.calls[0][0], query.mock.calls[1][0]];
    for (const sql of [rowsSql, countSql]) {
      expect(sql).toContain("span_id = 'span-1'");
      expect(sql).toContain("severity_number >= 13");
      expect(sql).toContain("timestamp_unix_nano >= 100");
      expect(sql).toContain("timestamp_unix_nano <= 200");
    }
  });

  it("filters on tenant_id when one is supplied", async () => {
    const query = vi.fn().mockResolvedValue({ toArray: () => [] });
    const api = new DuckDbLogQueryApi({ query });

    await api.search({ tenantId: "t-1", limit: 10 });

    expect(query.mock.calls[0][0]).toContain("tenant_id = 't-1'");
  });

  it("returns the real total count alongside the page of rows", async () => {
    const rows = [{ log_id: "a" }, { log_id: "b" }];
    const query = vi
      .fn()
      .mockResolvedValueOnce({ toArray: () => rows })
      .mockResolvedValueOnce({ toArray: () => [{ total: 42 }] });
    const api = new DuckDbLogQueryApi({ query });

    const result = await api.search({ limit: 2 });

    expect(result.rows).toEqual(rows);
    expect(result.total).toBe(42);
  });

  it("computes requested facets, ignoring fields outside the allowed set", async () => {
    const query = vi
      .fn()
      .mockResolvedValueOnce({ toArray: () => [] })
      .mockResolvedValueOnce({ toArray: () => [{ total: 0 }] })
      .mockResolvedValueOnce({
        toArray: () => [
          { value: "checkout", count: 5 },
          { value: "web", count: 2 },
        ],
      });
    const api = new DuckDbLogQueryApi({ query });

    const result = await api.search({ limit: 10, facets: ["service_name", "not_a_real_column"] });

    expect(result.facets).toEqual({
      service_name: [
        { value: "checkout", count: 5 },
        { value: "web", count: 2 },
      ],
    });
    expect(query).toHaveBeenCalledTimes(3);
  });
});
