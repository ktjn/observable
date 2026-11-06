import { describe, it, expect, vi, beforeEach } from "vitest";
import { searchLogs, fetchLogHistogram, tailLogs, getLogContext } from "./logs";
import { fetchTraceHistogram, searchTraces, getTrace } from "./traces";
import { listMetrics, getMetricGroupPoints, type MetricCatalogEntry } from "./metrics";
import {
  listServiceSummaries,
  getServiceSummary,
  getTopology,
  listEnvironments,
  listServices,
  getServiceResponseTimeHistory,
} from "./services";
import { listInfrastructure, getInfrastructureDetail } from "./infrastructure";
import { submitNlqQuery, prepareNlqQuery } from "./nlq";

const SESSION_TENANT_ID = "00000000-0000-0000-0000-0000000000a1";

const METRIC: MetricCatalogEntry = {
  tenant_id: SESSION_TENANT_ID,
  metric_name: "http.server.duration",
  description: "",
  unit: "ms",
  metric_type: "histogram",
  service_name: "checkout",
  environment: "prod",
  series_count: 1,
};

const TELEMETRY_QUERIES: Array<[string, (tenantId: string) => Promise<unknown>]> = [
  ["searchLogs", (t) => searchLogs(t, {})],
  ["fetchLogHistogram", (t) => fetchLogHistogram(t, { from: "a", to: "b" })],
  ["tailLogs", (t) => tailLogs(t, {})],
  ["getLogContext", (t) => getLogContext(t, "log-1")],
  ["fetchTraceHistogram", (t) => fetchTraceHistogram(t, {})],
  ["searchTraces", (t) => searchTraces(t, {})],
  ["getTrace", (t) => getTrace(t, "4bf92f3577b34da6a3ce929d0e0e4736")],
  ["listMetrics", (t) => listMetrics(t)],
  ["getMetricGroupPoints", (t) => getMetricGroupPoints(t, METRIC)],
  ["listServiceSummaries", (t) => listServiceSummaries(t)],
  ["getServiceSummary", (t) => getServiceSummary(t, "checkout")],
  ["getTopology", (t) => getTopology(t)],
  ["listEnvironments", (t) => listEnvironments(t)],
  ["listServices", (t) => listServices(t)],
  ["getServiceResponseTimeHistory", (t) => getServiceResponseTimeHistory(t, "checkout")],
  ["listInfrastructure", (t) => listInfrastructure(t)],
  ["getInfrastructureDetail", (t) => getInfrastructureDetail(t, "host", "host-1")],
  ["submitNlqQuery", (t) => submitNlqQuery(t, { question: "error rate" })],
  ["prepareNlqQuery", (t) => prepareNlqQuery(t, { question: "error rate" })],
];

function tenantHeader(init: RequestInit | undefined): string | null {
  return new Headers(init?.headers).get("X-Tenant-ID");
}

describe("telemetry query API tenant binding", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({ ok: true, status: 200, json: async () => ({}) }),
    );
    vi.stubGlobal("window", { location: { origin: "http://localhost" } });
  });

  it("every telemetry query function sends the active session tenant_id", async () => {
    for (const [name, call] of TELEMETRY_QUERIES) {
      vi.mocked(fetch).mockClear();
      await call(SESSION_TENANT_ID).catch(() => undefined);
      const calls = vi.mocked(fetch).mock.calls;
      expect(calls.length, `${name} issued no request`).toBeGreaterThan(0);
      for (const [, init] of calls) {
        expect(tenantHeader(init), name).toBe(SESSION_TENANT_ID);
      }
    }
  });
});
