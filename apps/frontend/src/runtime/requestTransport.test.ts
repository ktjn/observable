import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { dispatch } from "./requestTransport";
import { httpTransport } from "./httpTransport";

const executeTopologyMock = vi.fn(async () => ({ edges: [] }));
const executeChangeEventsMock = vi.fn(async () => ({ changeEvents: [] }));
const executeMetricCatalogMock = vi.fn(async () => ({ metrics: [] }));
const executeMetricGroupPointsMock = vi.fn(async () => ({ points: [] }));
const executeLogHistogramMock = vi.fn(async () => ({ buckets: [] }));
const executeLogsSearchMock = vi.fn(async () => ({ logs: [] }));
const executeLogsContextMock = vi.fn(async () => ({ logs: [] }));
const executeLogsTailMock = vi.fn(async () => ({ logs: [] }));
const executeTraceTableMock = vi.fn(async (): Promise<{ rows: unknown[]; sql: string }> => ({ rows: [], sql: "" }));
const executeTraceDetailMock = vi.fn(async (): Promise<{ spans: unknown[] }> => ({ spans: [] }));
const executeTraceHistogramMock = vi.fn(async () => ({ buckets: [] }));
const executeServiceSummariesMock = vi.fn(async (): Promise<{ items: unknown[] }> => ({ items: [] }));
const executeServiceNamesMock = vi.fn(async (): Promise<{ names: string[] }> => ({ names: [] }));
const executeResponseTimeHistogramMock = vi.fn(async () => ({ buckets: [] }));

vi.mock("../playground/engineClient", () => ({
  executeTopology: executeTopologyMock,
  executeChangeEvents: executeChangeEventsMock,
  executeMetricCatalog: executeMetricCatalogMock,
  executeMetricGroupPoints: executeMetricGroupPointsMock,
  executeLogHistogram: executeLogHistogramMock,
  executeLogsSearch: executeLogsSearchMock,
  executeLogsContext: executeLogsContextMock,
  executeLogsTail: executeLogsTailMock,
  executeTraceTable: executeTraceTableMock,
  executeTraceDetail: executeTraceDetailMock,
  executeTraceHistogram: executeTraceHistogramMock,
  executeServiceSummaries: executeServiceSummariesMock,
  executeServiceNames: executeServiceNamesMock,
  executeResponseTimeHistogram: executeResponseTimeHistogramMock,
}));

const mockAlertRepo = {
  list: vi.fn(),
  get: vi.fn(),
  getRunbook: vi.fn(),
  create: vi.fn(),
  setSilenced: vi.fn(),
  setRunbook: vi.fn(),
};

vi.mock("./sqliteAlertRuleRepository", () => ({
  SqliteAlertRuleRepository: { open: vi.fn(async () => mockAlertRepo) },
}));

const mockIncidentRepo = {
  list: vi.fn(),
  get: vi.fn(),
  getDetail: vi.fn(),
  acknowledge: vi.fn(),
  updateStatus: vi.fn(),
  resolve: vi.fn(),
  addComment: vi.fn(),
  linkDeployment: vi.fn(),
};

vi.mock("./sqliteIncidentRepository", () => ({
  SqliteIncidentRepository: { open: vi.fn(async () => mockIncidentRepo) },
}));

const mockSloRepo = { list: vi.fn(), create: vi.fn() };

vi.mock("./sqliteSloRepository", () => ({
  SqliteSloRepository: { open: vi.fn(async () => mockSloRepo) },
}));

const mockNotificationChannelRepo = { list: vi.fn(), create: vi.fn(), delete: vi.fn() };

vi.mock("./sqliteNotificationChannelRepository", () => ({
  SqliteNotificationChannelRepository: { open: vi.fn(async () => mockNotificationChannelRepo) },
}));

// Imported after the mock so wasmTransport's dynamic `import("../playground/engineClient")`
// resolves to the mock above.
const { wasmTransport } = await import("./wasmTransport");

describe("httpTransport", () => {
  const fetchMock = vi.fn();

  beforeEach(() => {
    vi.stubGlobal("fetch", fetchMock);
    fetchMock.mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("issues a GET with credentials included and no body", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ ok: 1 }) });
    const result = await httpTransport.dispatch({ method: "GET", path: "/v1/tenants" });
    expect(result).toEqual({ ok: 1 });
    expect(fetchMock).toHaveBeenCalledWith(
      "/v1/tenants",
      expect.objectContaining({ method: "GET", credentials: "include" }),
    );
  });

  it("appends query params, omitting undefined values", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({}) });
    await httpTransport.dispatch({
      method: "GET",
      path: "/v1/logs",
      query: { service: "checkout", limit: undefined },
    });
    expect(fetchMock).toHaveBeenCalledWith("/v1/logs?service=checkout", expect.anything());
  });

  it("sends a JSON body on POST", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 200, json: async () => ({ id: "1" }) });
    await httpTransport.dispatch({ method: "POST", path: "/v1/alerts", body: { name: "x" } });
    const call = fetchMock.mock.calls[0];
    expect(call[1].method).toBe("POST");
    expect(call[1].body).toBe(JSON.stringify({ name: "x" }));
    expect(call[1].headers).toMatchObject({ "Content-Type": "application/json" });
  });

  it("throws a labeled error on a non-ok response", async () => {
    fetchMock.mockResolvedValue({ ok: false, status: 503 });
    await expect(httpTransport.dispatch({ method: "GET", path: "/v1/tenants" })).rejects.toThrow(
      "GET /v1/tenants failed: 503",
    );
  });

  it("returns undefined for a 204 with no body", async () => {
    fetchMock.mockResolvedValue({ ok: true, status: 204 });
    const result = await httpTransport.dispatch({ method: "DELETE", path: "/v1/tokens/1" });
    expect(result).toBeUndefined();
  });
});

describe("wasmTransport", () => {
  it("resolves the tenants list route", async () => {
    const result = await wasmTransport.dispatch<{ tenants: { id: string; name: string }[] }>({
      method: "GET",
      path: "/v1/tenants",
    });
    expect(result.tenants).toEqual([{ id: "00000000-0000-0000-0000-000000000001", name: "observable" }]);
  });

  it("resolves the tenant environments route with a path param", async () => {
    const result = await wasmTransport.dispatch<{ environments: { environment: string }[] }>({
      method: "GET",
      path: "/v1/tenants/00000000-0000-0000-0000-000000000001/environments",
    });
    expect(result.environments).toEqual([{ environment: "production" }, { environment: "staging" }]);
  });

  it("rejects an unregistered route rather than silently returning nothing", async () => {
    await expect(
      wasmTransport.dispatch({ method: "GET", path: "/v1/not-a-real-route" }),
    ).rejects.toThrow("no wasm handler registered for GET /v1/not-a-real-route");
  });

  it("resolves /v1/topology, converting ISO query params back to nanoseconds", async () => {
    executeTopologyMock.mockClear();
    await wasmTransport.dispatch({
      method: "GET",
      path: "/v1/topology",
      query: { from: "2026-01-01T00:00:00.000Z", to: "2026-01-01T01:00:00.000Z", environment: "prod", service: "checkout" },
    });
    expect(executeTopologyMock).toHaveBeenCalledWith({
      fromNs: String(BigInt(Date.parse("2026-01-01T00:00:00.000Z")) * 1_000_000n),
      toNs: String(BigInt(Date.parse("2026-01-01T01:00:00.000Z")) * 1_000_000n),
      environment: "prod",
      service: "checkout",
    });
  });

  it("resolves /v1/events/changes with its filters and limit", async () => {
    executeChangeEventsMock.mockClear();
    await wasmTransport.dispatch({
      method: "GET",
      path: "/v1/events/changes",
      query: {
        start_time: "2026-01-01T00:00:00.000Z",
        end_time: "2026-01-01T01:00:00.000Z",
        service_name: "checkout",
        environment: "prod",
        event_type: "migration",
        limit: 10,
      },
    });
    expect(executeChangeEventsMock).toHaveBeenCalledWith({
      fromNs: String(BigInt(Date.parse("2026-01-01T00:00:00.000Z")) * 1_000_000n),
      toNs: String(BigInt(Date.parse("2026-01-01T01:00:00.000Z")) * 1_000_000n),
      service: "checkout",
      environment: "prod",
      eventType: "migration",
      limit: 10,
    });
  });

  it("resolves /v1/metrics with the service filter", async () => {
    executeMetricCatalogMock.mockClear();
    await wasmTransport.dispatch({ method: "GET", path: "/v1/metrics", query: { service: "checkout" } });
    expect(executeMetricCatalogMock).toHaveBeenCalledWith("checkout");
  });

  it("resolves /v1/metrics/points, rebuilding the catalog entry from query params", async () => {
    executeMetricGroupPointsMock.mockClear();
    await wasmTransport.dispatch({
      method: "GET",
      path: "/v1/metrics/points",
      query: {
        metric_name: "http.server.duration",
        service: "checkout",
        environment: "production",
        metric_type: "histogram",
        unit: "ms",
      },
    });
    expect(executeMetricGroupPointsMock).toHaveBeenCalledWith(
      expect.objectContaining({
        metric_name: "http.server.duration",
        service_name: "checkout",
        environment: "production",
        metric_type: "histogram",
        unit: "ms",
      }),
    );
  });

  it("resolves /v1/logs/histogram with fromNs/toNs passed through unchanged", async () => {
    executeLogHistogramMock.mockClear();
    await wasmTransport.dispatch({
      method: "GET",
      path: "/v1/logs/histogram",
      query: { from: "1000", to: "2000", buckets: 30, service: "checkout" },
    });
    expect(executeLogHistogramMock).toHaveBeenCalledWith({
      fromNs: "1000",
      toNs: "2000",
      bucketCount: 30,
      service: "checkout",
    });
  });

  it("resolves /v1/logs search", async () => {
    executeLogsSearchMock.mockClear();
    await wasmTransport.dispatch({
      method: "GET",
      path: "/v1/logs",
      query: { trace_id: "abc", service: "checkout", limit: 50 },
    });
    expect(executeLogsSearchMock).toHaveBeenCalledWith({ traceId: "abc", service: "checkout", limit: 50 });
  });

  it("resolves /v1/logs/:logId/context with the path param", async () => {
    executeLogsContextMock.mockClear();
    await wasmTransport.dispatch({
      method: "GET",
      path: "/v1/logs/log-123/context",
      query: { window: 10 },
    });
    expect(executeLogsContextMock).toHaveBeenCalledWith({ logId: "log-123", window: 10 });
  });

  it("resolves /v1/logs/tail", async () => {
    executeLogsTailMock.mockClear();
    await wasmTransport.dispatch({
      method: "GET",
      path: "/v1/logs/tail",
      query: { service: "checkout", severity: 17, since_unix_nano: "1000", limit: 25 },
    });
    expect(executeLogsTailMock).toHaveBeenCalledWith({
      service: "checkout",
      severity: 17,
      sinceUnixNano: "1000",
      limit: 25,
    });
  });

  it("resolves /v1/traces/histogram, calling the engine when from/to are present", async () => {
    executeTraceHistogramMock.mockClear();
    await wasmTransport.dispatch({
      method: "GET",
      path: "/v1/traces/histogram",
      query: { from: "1000", to: "2000", buckets: 20, service: "checkout" },
    });
    expect(executeTraceHistogramMock).toHaveBeenCalledWith({
      fromNs: "1000",
      toNs: "2000",
      bucketCount: 20,
      service: "checkout",
    });
  });

  it("resolves /v1/traces/histogram with a stub bucket when from/to are absent", async () => {
    executeTraceHistogramMock.mockClear();
    const result = await wasmTransport.dispatch<{ buckets: unknown[] }>({
      method: "GET",
      path: "/v1/traces/histogram",
      query: {},
    });
    expect(executeTraceHistogramMock).not.toHaveBeenCalled();
    expect(result.buckets).toHaveLength(1);
  });

  it("resolves /v1/traces search, defaulting the time range and mapping rows to a Span", async () => {
    executeTraceTableMock.mockClear();
    executeTraceTableMock.mockResolvedValueOnce({
      rows: [
        {
          trace_id: "trace-1",
          root_service: "checkout",
          root_operation: "POST /checkout",
          duration_ms: 12,
          status_code: "OK",
          environment: "production",
          start_time_unix_nano: 0,
        },
      ],
      sql: "",
    });
    const result = await wasmTransport.dispatch<{ traces: { trace_id: string; spans: unknown[] }[]; total: number }>({
      method: "GET",
      path: "/v1/traces",
      query: { service: "checkout", limit: 10 },
    });
    expect(executeTraceTableMock).toHaveBeenCalledWith(
      expect.objectContaining({
        filters: [{ field: "service_name", op: "=", value: "checkout" }],
        time_range: { from: "now-1h", to: "now" },
      }),
    );
    expect(result.traces).toHaveLength(1);
    expect(result.traces[0].trace_id).toBe("trace-1");
    expect(result.traces[0].spans).toHaveLength(1);
  });

  it("resolves /v1/traces/:traceId", async () => {
    executeTraceDetailMock.mockClear();
    executeTraceDetailMock.mockResolvedValueOnce({ spans: [{ span_id: "s1" }] });
    const result = await wasmTransport.dispatch<{ trace_id: string }>({
      method: "GET",
      path: "/v1/traces/trace-1",
    });
    expect(executeTraceDetailMock).toHaveBeenCalledWith("trace-1");
    expect(result.trace_id).toBe("trace-1");
  });

  it("rejects /v1/traces/:traceId when the engine finds no spans", async () => {
    executeTraceDetailMock.mockClear();
    executeTraceDetailMock.mockResolvedValueOnce({ spans: [] });
    await expect(
      wasmTransport.dispatch({ method: "GET", path: "/v1/traces/missing" }),
    ).rejects.toThrow("Not found: missing");
  });

  it("does not let /v1/traces/:traceId swallow the literal /v1/traces/histogram route", async () => {
    executeTraceDetailMock.mockClear();
    executeTraceHistogramMock.mockClear();
    await wasmTransport.dispatch({ method: "GET", path: "/v1/traces/histogram", query: {} });
    expect(executeTraceDetailMock).not.toHaveBeenCalled();
  });

  it("resolves /v1/services/summary, converting ISO from/to back to nanoseconds", async () => {
    executeServiceSummariesMock.mockClear();
    executeServiceSummariesMock.mockResolvedValueOnce({
      items: [{ service_name: "checkout" }, { service_name: "payments" }],
    });
    const result = await wasmTransport.dispatch<{ items: { service_name: string }[] }>({
      method: "GET",
      path: "/v1/services/summary",
      query: { environment: "prod", from: "2026-01-01T00:00:00.000Z", to: "2026-01-01T01:00:00.000Z" },
    });
    expect(executeServiceSummariesMock).toHaveBeenCalledWith({
      fromNs: String(BigInt(Date.parse("2026-01-01T00:00:00.000Z")) * 1_000_000n),
      toNs: String(BigInt(Date.parse("2026-01-01T01:00:00.000Z")) * 1_000_000n),
      environment: "prod",
    });
    expect(result.items).toHaveLength(2);
  });

  it("resolves /v1/services/:serviceName/summary, picking out the one service", async () => {
    executeServiceSummariesMock.mockClear();
    executeServiceSummariesMock.mockResolvedValueOnce({
      items: [{ service_name: "checkout" }, { service_name: "payments" }],
    });
    const result = await wasmTransport.dispatch<{ service: { service_name: string } }>({
      method: "GET",
      path: "/v1/services/payments/summary",
      query: {},
    });
    expect(result.service.service_name).toBe("payments");
  });

  it("rejects /v1/services/:serviceName/summary when the service isn't in the summary set", async () => {
    executeServiceSummariesMock.mockClear();
    executeServiceSummariesMock.mockResolvedValueOnce({ items: [] });
    await expect(
      wasmTransport.dispatch({ method: "GET", path: "/v1/services/missing/summary", query: {} }),
    ).rejects.toThrow("Service not found: missing");
  });

  it("resolves /v1/services to the engine's service names", async () => {
    executeServiceNamesMock.mockClear();
    executeServiceNamesMock.mockResolvedValueOnce({ names: ["checkout", "payments"] });
    const result = await wasmTransport.dispatch<{ items: string[] }>({ method: "GET", path: "/v1/services" });
    expect(result.items).toEqual(["checkout", "payments"]);
  });

  it("resolves /v1/services/:serviceName/response-time-history", async () => {
    executeResponseTimeHistogramMock.mockClear();
    await wasmTransport.dispatch({
      method: "GET",
      path: "/v1/services/checkout/response-time-history",
      query: { from: "2026-01-01T00:00:00.000Z", to: "2026-01-01T01:00:00.000Z", buckets: 60 },
    });
    expect(executeResponseTimeHistogramMock).toHaveBeenCalledWith({
      fromNs: String(BigInt(Date.parse("2026-01-01T00:00:00.000Z")) * 1_000_000n),
      toNs: String(BigInt(Date.parse("2026-01-01T01:00:00.000Z")) * 1_000_000n),
      bucketCount: 60,
      serviceName: "checkout",
    });
  });

  it("resolves GET /v1/alerts/rules", async () => {
    mockAlertRepo.list.mockReset().mockReturnValue({ items: [{ rule_id: "r1" }] });
    const result = await wasmTransport.dispatch<{ items: { rule_id: string }[] }>({
      method: "GET",
      path: "/v1/alerts/rules",
    });
    expect(result.items).toEqual([{ rule_id: "r1" }]);
  });

  it("resolves GET /v1/alerts/rules/:ruleId, mapping to AlertRuleDetailResponse", async () => {
    mockAlertRepo.get.mockReset().mockReturnValue({
      rule_id: "r1",
      name: "checkout p95",
      severity: "warning",
      metric_name: "http.server.duration",
      operator: "gt",
      threshold: 500,
      silenced: false,
      firing: true,
      last_fired_at: "2026-01-01T00:00:00.000Z",
    });
    mockAlertRepo.getRunbook.mockReset().mockReturnValue("https://runbooks/checkout");
    const result = await wasmTransport.dispatch<{
      rule_id: string;
      condition: Record<string, unknown>;
      firings: unknown[];
      runbook_url: string | null;
    }>({ method: "GET", path: "/v1/alerts/rules/r1" });
    expect(result.rule_id).toBe("r1");
    expect(result.condition).toEqual({ metric_name: "http.server.duration", operator: "gt", threshold: 500 });
    expect(result.firings).toHaveLength(1);
    expect(result.runbook_url).toBe("https://runbooks/checkout");
  });

  it("resolves POST /v1/admin/alerts/rules", async () => {
    mockAlertRepo.create.mockReset().mockReturnValue("new-rule-id");
    const result = await wasmTransport.dispatch<{ rule_id: string }>({
      method: "POST",
      path: "/v1/admin/alerts/rules",
      body: { name: "x", metric_name: "m", operator: "gt", threshold: 1 },
    });
    expect(result.rule_id).toBe("new-rule-id");
    expect(mockAlertRepo.create).toHaveBeenCalledWith(
      expect.any(String),
      { name: "x", metric_name: "m", operator: "gt", threshold: 1 },
    );
  });

  it("resolves PATCH /v1/admin/alerts/rules/:ruleId/silence", async () => {
    mockAlertRepo.setSilenced.mockReset();
    await wasmTransport.dispatch({
      method: "PATCH",
      path: "/v1/admin/alerts/rules/r1/silence",
      body: { silenced: true },
    });
    expect(mockAlertRepo.setSilenced).toHaveBeenCalledWith(expect.any(String), "r1", true);
  });

  it("resolves PATCH /v1/admin/alerts/rules/:ruleId/runbook", async () => {
    mockAlertRepo.setRunbook.mockReset();
    await wasmTransport.dispatch({
      method: "PATCH",
      path: "/v1/admin/alerts/rules/r1/runbook",
      body: { runbook_url: "https://runbooks/x" },
    });
    expect(mockAlertRepo.setRunbook).toHaveBeenCalledWith(expect.any(String), "r1", "https://runbooks/x");
  });

  it("resolves GET /v1/incidents, passing the status filter through", async () => {
    mockIncidentRepo.list.mockReset().mockReturnValue({ items: [{ incident_id: "i1" }] });
    const result = await wasmTransport.dispatch<{ items: unknown[] }>({
      method: "GET",
      path: "/v1/incidents",
      query: { status: "open" },
    });
    expect(mockIncidentRepo.list).toHaveBeenCalledWith(expect.any(String), "open");
    expect(result.items).toHaveLength(1);
  });

  it("resolves GET /v1/incidents/:incidentId", async () => {
    mockIncidentRepo.get.mockReset().mockReturnValue({ incident_id: "i1" });
    mockIncidentRepo.getDetail.mockReset().mockReturnValue({ incident_id: "i1", status: "open" });
    const result = await wasmTransport.dispatch<{ incident_id: string }>({
      method: "GET",
      path: "/v1/incidents/i1",
    });
    expect(result.incident_id).toBe("i1");
  });

  it("rejects GET /v1/incidents/:incidentId when the incident doesn't exist", async () => {
    mockIncidentRepo.get.mockReset().mockReturnValue(null);
    await expect(
      wasmTransport.dispatch({ method: "GET", path: "/v1/incidents/missing" }),
    ).rejects.toThrow("Failed to get incident: 404");
  });

  it("resolves POST /v1/incidents/:incidentId/acknowledge", async () => {
    mockIncidentRepo.acknowledge.mockReset().mockReturnValue({ incident_id: "i1", status: "acknowledged" });
    const result = await wasmTransport.dispatch<{ status: string }>({
      method: "POST",
      path: "/v1/incidents/i1/acknowledge",
      body: { actor: "me" },
    });
    expect(mockIncidentRepo.acknowledge).toHaveBeenCalledWith(expect.any(String), "i1", { actor: "me" });
    expect(result.status).toBe("acknowledged");
  });

  it("resolves POST /v1/incidents/:incidentId/status", async () => {
    mockIncidentRepo.updateStatus.mockReset().mockReturnValue({ incident_id: "i1", status: "resolved" });
    await wasmTransport.dispatch({
      method: "POST",
      path: "/v1/incidents/i1/status",
      body: { status: "resolved", actor: "me" },
    });
    expect(mockIncidentRepo.updateStatus).toHaveBeenCalledWith(expect.any(String), "i1", "resolved", { actor: "me" });
  });

  it("resolves POST /v1/incidents/:incidentId/resolve", async () => {
    mockIncidentRepo.resolve.mockReset().mockReturnValue({ incident_id: "i1", status: "resolved" });
    await wasmTransport.dispatch({
      method: "POST",
      path: "/v1/incidents/i1/resolve",
      body: {},
    });
    expect(mockIncidentRepo.resolve).toHaveBeenCalledWith(expect.any(String), "i1", {});
  });

  it("resolves POST /v1/incidents/:incidentId/comments", async () => {
    mockIncidentRepo.addComment.mockReset().mockReturnValue({ incident_id: "i1" });
    await wasmTransport.dispatch({
      method: "POST",
      path: "/v1/incidents/i1/comments",
      body: { message: "hi", actor: "me" },
    });
    expect(mockIncidentRepo.addComment).toHaveBeenCalledWith(expect.any(String), "i1", "hi", "me");
  });

  it("resolves POST /v1/incidents/:incidentId/link-deployment", async () => {
    mockIncidentRepo.linkDeployment.mockReset().mockReturnValue({ incident_id: "i1" });
    await wasmTransport.dispatch({
      method: "POST",
      path: "/v1/incidents/i1/link-deployment",
      body: { deployment_id: "d1", actor: "me" },
    });
    expect(mockIncidentRepo.linkDeployment).toHaveBeenCalledWith(expect.any(String), "i1", "d1", { actor: "me" });
  });

  it("resolves GET /v1/slos", async () => {
    mockSloRepo.list.mockReset().mockReturnValue({ items: [{ slo_id: "s1" }] });
    const result = await wasmTransport.dispatch<{ items: unknown[] }>({ method: "GET", path: "/v1/slos" });
    expect(result.items).toHaveLength(1);
  });

  it("resolves POST /v1/slos", async () => {
    mockSloRepo.create.mockReset().mockReturnValue({ slo_id: "s1" });
    const body = { service_name: "checkout", environment: "prod", target: 99.9 };
    const result = await wasmTransport.dispatch<{ slo_id: string }>({
      method: "POST",
      path: "/v1/slos",
      body,
    });
    expect(mockSloRepo.create).toHaveBeenCalledWith(expect.any(String), body);
    expect(result.slo_id).toBe("s1");
  });

  it("resolves GET /v1/notifications/channels", async () => {
    mockNotificationChannelRepo.list.mockReset().mockReturnValue([{ channel_id: "c1" }]);
    const result = await wasmTransport.dispatch<unknown[]>({
      method: "GET",
      path: "/v1/notifications/channels",
    });
    expect(result).toEqual([{ channel_id: "c1" }]);
  });

  it("resolves POST /v1/notifications/channels", async () => {
    mockNotificationChannelRepo.create.mockReset().mockReturnValue({ channel_id: "c1" });
    const body = { name: "x", channel_type: "webhook", config: { url: "https://x" } };
    const result = await wasmTransport.dispatch<{ channel_id: string }>({
      method: "POST",
      path: "/v1/notifications/channels",
      body,
    });
    expect(mockNotificationChannelRepo.create).toHaveBeenCalledWith(expect.any(String), body);
    expect(result.channel_id).toBe("c1");
  });

  it("resolves DELETE /v1/notifications/channels/:channelId", async () => {
    mockNotificationChannelRepo.delete.mockReset();
    await wasmTransport.dispatch({ method: "DELETE", path: "/v1/notifications/channels/c1" });
    expect(mockNotificationChannelRepo.delete).toHaveBeenCalledWith(expect.any(String), "c1");
  });
});

describe("dispatch", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
    vi.unstubAllGlobals();
  });

  it("uses httpTransport by default (hosted mode)", async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, status: 200, json: async () => ({ ok: 1 }) });
    vi.stubGlobal("fetch", fetchMock);
    const result = await dispatch({ method: "GET", path: "/v1/tenants" });
    expect(result).toEqual({ ok: 1 });
    expect(fetchMock).toHaveBeenCalled();
  });

  it("uses wasmTransport when VITE_OBSERVABLE_RUNTIME is playground", async () => {
    vi.stubEnv("VITE_OBSERVABLE_RUNTIME", "playground");
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    const result = await dispatch<{ tenants: unknown[] }>({ method: "GET", path: "/v1/tenants" });
    expect(result.tenants).toHaveLength(1);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
