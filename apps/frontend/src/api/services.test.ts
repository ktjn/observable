import { describe, it, expect, vi, beforeEach } from "vitest";
import { listServiceSummaries, getServiceSummary, getTopology, getServiceResponseTimeHistory } from "./services";

const MOCK_TENANT_ID = "00000000-0000-0000-0000-000000000001";

describe("services API from/to params", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ items: [] }),
    }));
    vi.stubGlobal("window", { location: { origin: "http://localhost" } });
  });

  it("listServiceSummaries sends from and to as ISO strings", async () => {
    await listServiceSummaries(MOCK_TENANT_ID, { from: 1_000_000, to: 3_600_000 });
    const url = new URL((vi.mocked(fetch).mock.calls[0][0] as string));
    expect(url.searchParams.get("from")).toBe(new Date(1_000_000).toISOString());
    expect(url.searchParams.get("to")).toBe(new Date(3_600_000).toISOString());
    expect(url.searchParams.has("lookback_minutes")).toBe(false);
  });

  it("getServiceSummary sends from and to as ISO strings", async () => {
    vi.mocked(fetch).mockResolvedValueOnce({
      ok: true,
      json: async () => ({ service: {} }),
    } as unknown as Response);
    await getServiceSummary(MOCK_TENANT_ID, "checkout", { from: 1_000_000, to: 3_600_000 });
    const url = new URL((vi.mocked(fetch).mock.calls[0][0] as string));
    expect(url.searchParams.get("from")).toBe(new Date(1_000_000).toISOString());
    expect(url.searchParams.get("to")).toBe(new Date(3_600_000).toISOString());
    expect(url.searchParams.has("lookback_minutes")).toBe(false);
  });

  it("getTopology sends from and to as ISO strings", async () => {
    await getTopology(MOCK_TENANT_ID, { from: 1_000_000, to: 3_600_000 });
    const url = new URL((vi.mocked(fetch).mock.calls[0][0] as string));
    expect(url.searchParams.get("from")).toBe(new Date(1_000_000).toISOString());
    expect(url.searchParams.get("to")).toBe(new Date(3_600_000).toISOString());
    expect(url.searchParams.has("lookback_minutes")).toBe(false);
  });

  it("getServiceResponseTimeHistory sends from and to as ISO strings", async () => {
    await getServiceResponseTimeHistory(MOCK_TENANT_ID, "checkout", { from: 1_000_000, to: 3_600_000 });
    const url = new URL((vi.mocked(fetch).mock.calls[0][0] as string));
    expect(url.searchParams.get("from")).toBe(new Date(1_000_000).toISOString());
    expect(url.searchParams.get("to")).toBe(new Date(3_600_000).toISOString());
    expect(url.searchParams.has("lookback_minutes")).toBe(false);
  });
});
