import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { dispatch } from "./requestTransport";
import { httpTransport } from "./httpTransport";

const executeTopologyMock = vi.fn(async () => ({ edges: [] }));
const executeChangeEventsMock = vi.fn(async () => ({ changeEvents: [] }));
const executeMetricCatalogMock = vi.fn(async () => ({ metrics: [] }));
const executeMetricGroupPointsMock = vi.fn(async () => ({ points: [] }));

vi.mock("../playground/engineClient", () => ({
  executeTopology: executeTopologyMock,
  executeChangeEvents: executeChangeEventsMock,
  executeMetricCatalog: executeMetricCatalogMock,
  executeMetricGroupPoints: executeMetricGroupPointsMock,
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
