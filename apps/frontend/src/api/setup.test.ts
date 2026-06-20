import { describe, it, expect, vi, beforeEach } from "vitest";
import { getFirstSignalStatus } from "./setup";

const MOCK_TENANT_ID = "00000000-0000-0000-0000-000000000001";

describe("getFirstSignalStatus", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn());
  });

  it("calls GET /v1/setup/status with tenant headers", async () => {
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ state: "waiting", traces: 0, logs: 0, metrics: 0 }),
    } as Response);

    await getFirstSignalStatus(MOCK_TENANT_ID);

    const [url, init] = vi.mocked(fetch).mock.calls[0];
    expect(url).toBe("/v1/setup/status");
    expect((init?.headers as Record<string, string>)["X-Tenant-ID"]).toBe(MOCK_TENANT_ID);
  });

  it("returns the parsed status on success", async () => {
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ state: "detected", traces: 3, logs: 1, metrics: 0 }),
    } as Response);

    const result = await getFirstSignalStatus(MOCK_TENANT_ID);

    expect(result).toEqual({ state: "detected", traces: 3, logs: 1, metrics: 0 });
  });

  it("returns an error state when the request fails", async () => {
    vi.mocked(fetch).mockResolvedValueOnce({ ok: false } as Response);

    const result = await getFirstSignalStatus(MOCK_TENANT_ID);

    expect(result).toEqual({ state: "error", traces: 0, logs: 0, metrics: 0 });
  });

  it("returns an error state when fetch throws", async () => {
    vi.mocked(fetch).mockRejectedValueOnce(new Error("network down"));

    const result = await getFirstSignalStatus(MOCK_TENANT_ID);

    expect(result).toEqual({ state: "error", traces: 0, logs: 0, metrics: 0 });
  });
});
