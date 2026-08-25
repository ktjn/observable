import { describe, expect, it, vi } from "vitest";
import { DuckDbStorageWriter } from "./duckdbStorageWriter";

describe("DuckDbStorageWriter", () => {
  it("writes each processed telemetry stream through its DuckDB table boundary", async () => {
    const query = vi.fn().mockResolvedValue(undefined);
    const writer = new DuckDbStorageWriter({ query });

    await writer.write({
      spans: [
        {
          tenant_id: "tenant",
          trace_id: "trace",
          span_id: "span",
          parent_span_id: null,
          service_name: "api",
          operation_name: "GET /health",
          duration_ns: "12",
          status_code: "OK",
          environment: "test",
          start_time_unix_nano: "100",
        },
      ],
      logs: [],
      series: [],
      points: [],
    });

    expect(query).toHaveBeenCalledTimes(1);
    expect(query.mock.calls[0][0]).toContain("INSERT INTO spans VALUES");
    expect(query.mock.calls[0][0]).toContain("'tenant'");
  });

  it("does not issue empty inserts", async () => {
    const query = vi.fn();
    await new DuckDbStorageWriter({ query }).write({ spans: [], logs: [], series: [], points: [] });
    expect(query).not.toHaveBeenCalled();
  });
});
