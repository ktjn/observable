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
  executeTraceDetail: vi.fn(async () => ({
    spans: [      {
        span_id: "mock-span-1",
        trace_id: "mock-trace-1",
        parent_span_id: undefined,
        tenant_id: "00000000-0000-0000-0000-000000000001",
        service_name: "checkout",
        service_namespace: "observable-playground",
        service_version: "",
        operation_name: "POST /checkout",
        span_kind: "SERVER",
        start_time_unix_nano: 0,
        end_time_unix_nano: 12_000_000,
        duration_ns: 12_000_000,
        status_code: "OK",
        status_message: "",
        attributes: {},
        resource_attributes: {},
        environment: "production",
        host_id: "",
        workload: "",
        deployment_id: "",
      },
    ],
  })),
  executeLogsSearch: vi.fn(async () => ({ logs: [] })),
  executeLogsContext: vi.fn(async () => ({ logs: [] })),
  executeLogsTail: vi.fn(async () => ({ logs: [] })),
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

    it("traces.get returns the TraceResponse shape", async () => {
      const result = await runtime.traces.get(MOCK_TENANT_ID, "trace-1");
      expect(typeof result.trace_id).toBe("string");
      expect(Array.isArray(result.spans)).toBe(true);
      expect(Array.isArray(result.events)).toBe(true);
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

    it("logs.search returns the LogListResponse shape", async () => {
      const result = await runtime.logs.search(MOCK_TENANT_ID, { trace_id: "trace-1" });
      expect(Array.isArray(result.logs)).toBe(true);
      expect(typeof result.total).toBe("number");
      expect(typeof result.facets).toBe("object");
    });

    it("logs.context returns the LogListResponse shape", async () => {
      const result = await runtime.logs.context(MOCK_TENANT_ID, "log-1");
      expect(Array.isArray(result.logs)).toBe(true);
    });

    it("logs.tail returns the LogListResponse shape", async () => {
      const result = await runtime.logs.tail(MOCK_TENANT_ID, { limit: 10 });
      expect(Array.isArray(result.logs)).toBe(true);
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

    it("alerts.list returns the AlertRuleListResponse shape", async () => {
      const result = await runtime.alerts.list(MOCK_TENANT_ID);
      expect(Array.isArray(result.items)).toBe(true);
    });

    it("incidents.list returns the IncidentListResponse shape", async () => {
      const result = await runtime.incidents.list(MOCK_TENANT_ID, "open");
      expect(Array.isArray(result.items)).toBe(true);
    });

    it("deployments.list returns the ListDeploymentsResponse shape", async () => {
      const result = await runtime.deployments.list(MOCK_TENANT_ID, { limit: 10 });
      expect(Array.isArray(result.items)).toBe(true);
    });

    it("alerts.get returns the AlertRuleDetailResponse shape", async () => {
      const result = await runtime.alerts.get(MOCK_TENANT_ID, "playground-rule-1");
      expect(typeof result.rule_id).toBe("string");
      expect(Array.isArray(result.firings)).toBe(true);
    });

    it("alerts.create returns a CreateRuleResponse shape", async () => {
      const result = await runtime.alerts.create(MOCK_TENANT_ID, {
        name: "contract rule",
        metric_name: "cpu",
        operator: "gt",
        threshold: 90,
      });
      expect(typeof result.rule_id).toBe("string");
    });

    it("alerts.silence resolves without error", async () => {
      await expect(
        runtime.alerts.silence(MOCK_TENANT_ID, "playground-rule-1", true)
      ).resolves.toBeUndefined();
    });

    it("incidents.get returns the IncidentDetailResponse shape", async () => {
      const result = await runtime.incidents.get(MOCK_TENANT_ID, "playground-incident-1");
      expect(typeof result.incident_id).toBe("string");
      expect(Array.isArray(result.timeline)).toBe(true);
    });

    it("slos.list returns the SloListResponse shape", async () => {
      const result = await runtime.slos.list(MOCK_TENANT_ID);
      expect(Array.isArray(result.items)).toBe(true);
    });

    it("notificationChannels.list resolves with a defined result", async () => {
      const result = await runtime.notificationChannels.list(MOCK_TENANT_ID);
      expect(result).toBeDefined();
    });

    it("savedViews.list returns the SavedViewListResponse shape", async () => {
      const result = await runtime.savedViews.list(MOCK_TENANT_ID, "logs");
      expect(Array.isArray(result.items)).toBe(true);
    });

    it("savedViews.create returns a SavedView shape", async () => {
      const result = await runtime.savedViews.create(MOCK_TENANT_ID, {
        name: "contract view",
        signal_kind: "logs",
        config: {
          query: null,
          severity_filter: "all",
          time_range: { mode: "preset", preset: "1h" },
          visible_columns: [],
        },
      });
      expect(typeof result.saved_view_id).toBe("string");
    });

    it("savedViews.listGrants returns the GrantListResponse shape", async () => {
      const result = await runtime.savedViews.listGrants(MOCK_TENANT_ID, "view-1");
      expect(Array.isArray(result.grants)).toBe(true);
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
          trace_id: "trace-1",
          spans: [],
          events: [],
          items: [],
          logs: [],
          total: 0,
          traces: [],
          facets: {},
          rule_id: "rule-1",
          firings: [],
          condition: {},
          name: "rule",
          severity: "warning",
          silenced: false,
          firing: false,
          incident_id: "incident-1",
          title: "incident",
          dedup_key: "dedup",
          triggered_at: "2026-01-01T00:00:00Z",
          resolved_at: null,
          triggered_by_rule_id: null,
          runbook_url: null,
          rule_name: null,
          timeline: [],
          impacted_service: null,
          slo_id: "slo-1",
          service_name: "checkout",
          environment: "production",
          sli_type: "availability",
          target: 99.9,
          window_days: 30,
          burn_rate_fast_threshold: 14.4,
          burn_rate_slow_threshold: 6,
          description: "",
          created_at: new Date(0).toISOString(),
          updated_at: new Date(0).toISOString(),
          channel_id: "chan-1",
          buckets: [],
          tenants: [],
          environments: [],
          saved_view_id: "view-1",
          signal_kind: "logs",
          config: { severity_filter: "all", time_range: { mode: "preset", preset: "1h" }, visible_columns: [] },
          grants: [],
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
          visibility: "private",
          panels: [],
        }),
      })
    );
    vi.stubGlobal("window", { location: { origin: "http://localhost" } });
  });

  runContract("http", httpRuntime);
  runContract("playground", playgroundRuntime);
});
