import { beforeEach, describe, expect, it, vi } from "vitest";
import { httpRuntime } from "./httpRuntime";
import { playgroundRuntime } from "./playgroundRuntime";
import type { RuntimeApi } from "./types";

// playgroundRuntime.nlq.execute delegates trace-table queries to a real Web
// Worker (engineClient), which jsdom doesn't support. Mock it here — the
// worker/DuckDB/wasm pipeline itself is covered by
// apps/frontend/e2e/playground/traces.spec.ts against a real browser.
vi.mock("../playground/engineClient", () => ({
  executeTraceTable: vi.fn(async () => ({
    rows: [
      {
        trace_id: "mock-trace-1",
        root_service: "checkout",
        root_operation: "POST /checkout",
        duration_ms: 12,
        status_code: "OK",
        environment: "production",
        start_time_unix_nano: "0",
      },
    ],
    sql: "-- mocked",
  })),
  executeTraceHistogram: vi.fn(async () => ({
    buckets: [{ start_ms: 0, end_ms: 60_000, count: 1 }],
  })),
  executeLogTable: vi.fn(async () => ({
    rows: [
      {
        tenant_id: "00000000-0000-0000-0000-000000000001",
        log_id: "mock-log-1",
        timestamp_unix_nano: "0",
        observed_timestamp_unix_nano: "0",
        severity_number: 9,
        severity_text: "INFO",
        body: "checkout request completed",
        trace_id: "mock-trace-1",
        span_id: "mock-span-1",
        service_name: "checkout",
        environment: "production",
        host_id: "mock-host",
        attributes: {},
        resource_attributes: {},
      },
    ],
    sql: "-- mocked",
  })),
  executeLogHistogram: vi.fn(async () => ({
    buckets: [{ start_ms: 0, end_ms: 60_000, counts: { "9": 1, "17": 1 } }],
  })),
}));

const MOCK_TENANT_ID = "00000000-0000-0000-0000-000000000001";

/**
 * Shared assertions run against both `httpRuntime` and `playgroundRuntime` so the
 * two transports stay behaviorally compatible for the frontend, per plan section 15
 * (runtime contract tests). `httpRuntime` is exercised against a mocked `fetch`;
 * `playgroundRuntime` is the in-memory stub — this only proves response *shape*
 * compatibility, not query semantics (that requires the real playground engine).
 */
function runContract(name: string, runtime: RuntimeApi) {
  describe(`${name} runtime`, () => {
    it("reports its mode", () => {
      expect(runtime.mode).toBe(name);
    });

    it("tenants.list returns the TenantListResponse shape", async () => {
      const result = await runtime.tenants.list();
      expect(Array.isArray(result.tenants)).toBe(true);
    });

    it("tenants.listEnvironments returns the EnvironmentListResponse shape", async () => {
      const result = await runtime.tenants.listEnvironments(MOCK_TENANT_ID);
      expect(Array.isArray(result.environments)).toBe(true);
    });

    it("traces.search returns the TraceListResponse shape", async () => {
      const result = await runtime.traces.search(MOCK_TENANT_ID, { limit: 10 });
      expect(Array.isArray(result.traces)).toBe(true);
      expect(typeof result.total).toBe("number");
      expect(typeof result.facets).toBe("object");
    });

    it("traces.histogram returns the TraceHistogramResponse shape", async () => {
      const result = await runtime.traces.histogram(MOCK_TENANT_ID, {
        buckets: 12,
        from: "1700000000000000000",
        to: "1700003600000000000",
      });
      expect(Array.isArray(result.buckets)).toBe(true);
      for (const bucket of result.buckets) {
        expect(typeof bucket.start_ms).toBe("number");
        expect(typeof bucket.end_ms).toBe("number");
        expect(typeof bucket.count).toBe("number");
      }
    });

    it("logs.histogram returns the LogHistogramResponse shape", async () => {
      const result = await runtime.logs.histogram(MOCK_TENANT_ID, {
        buckets: 12,
        from: "1700000000000000000",
        to: "1700003600000000000",
      });
      expect(Array.isArray(result.buckets)).toBe(true);
      for (const bucket of result.buckets) {
        expect(typeof bucket.start_ms).toBe("number");
        expect(typeof bucket.end_ms).toBe("number");
        expect(typeof bucket.counts).toBe("object");
      }
    });

    it("nlq.execute returns a discriminated NlqResponse", async () => {
      const result = await runtime.nlq.execute(MOCK_TENANT_ID, {
        base_ir: { operation: "table", signals: ["traces"], filters: [], time_range: { from: "now-1h", to: "now" } },
        mode: "execute",
      });
      expect(typeof result.type).toBe("string");
      if (result.type === "frame") {
        expect(Array.isArray(result.frame.data)).toBe(true);
      }
    });

    it("dashboards.create returns a Dashboard shape", async () => {
      const result = await runtime.dashboards.create(MOCK_TENANT_ID, {
        name: "contract-test dashboard",
        panels: [],
      });
      expect(typeof result.dashboard_id).toBe("string");
      expect(typeof result.name).toBe("string");
    });
  });
}

describe("runtime contract", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({
          traces: [],
          total: 0,
          facets: {},
          buckets: [],
          tenants: [],
          environments: [],
          type: "frame",
          frame: {
            frame_type: "table",
            x_field: null,
            y_field: null,
            series_field: null,
            unit: null,
            suggested_visualization: "table",
            field_roles: [],
            data: [],
            nlq_ir: {
              operation: "table",
              signals: [],
              filters: [],
              group_by: [],
              time_range: { from: "now-1h", to: "now" },
            },
            source_sql: "",
            time_range: { from: "now-1h", to: "now" },
            signal_types: [],
            sample_rate: null,
            approximation_statement: "",
          },
          dashboard_id: "http-dashboard-1",
          name: "contract-test dashboard",
          visibility: "private",
          panels: [],
          created_at: new Date(0).toISOString(),
        }),
      })
    );
    vi.stubGlobal("window", { location: { origin: "http://localhost" } });
  });

  runContract("http", httpRuntime);
  runContract("playground", playgroundRuntime);
});
