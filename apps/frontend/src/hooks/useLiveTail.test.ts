import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { appendAndTrim, useLiveTail } from "./useLiveTail";
import * as logsApi from "../api/logs";
import type { LogRecord } from "../api/logs";

// ── appendAndTrim ──────────────────────────────────────────────────────────

describe("appendAndTrim", () => {
  it("keeps last N when combined length exceeds max", () => {
    expect(appendAndTrim(["a", "b", "c"], ["d", "e"], 3)).toEqual(["c", "d", "e"]);
  });

  it("keeps all when combined length is within max", () => {
    expect(appendAndTrim(["a"], ["b", "c"], 5)).toEqual(["a", "b", "c"]);
  });

  it("empty prev — returns last N of next", () => {
    expect(appendAndTrim([], ["a", "b", "c"], 2)).toEqual(["b", "c"]);
  });
});

// ── useLiveTail ────────────────────────────────────────────────────────────

function makeLog(id: string, timestampNano: string): LogRecord {
  return {
    tenant_id: "t1",
    log_id: id,
    timestamp_unix_nano: timestampNano,
    severity_number: 9,
    severity_text: "INFO",
    body: {},
    service_name: "svc",
  };
}

describe("useLiveTail", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.spyOn(logsApi, "tailLogs").mockResolvedValue({
      logs: [],
      total: 0,
      facets: {},
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("does not fetch when disabled", async () => {
    renderHook(() => useLiveTail({ tenantId: "t1", enabled: false }));
    await act(async () => {
      vi.advanceTimersByTime(10_000);
    });
    expect(logsApi.tailLogs).not.toHaveBeenCalled();
  });

  it("fetches immediately on enable and accumulates rows", async () => {
    vi.spyOn(logsApi, "tailLogs").mockResolvedValue({
      logs: [makeLog("1", "1000"), makeLog("2", "2000")],
      total: 2,
      facets: {},
    });
    const { result } = renderHook(() =>
      useLiveTail({ tenantId: "t1", enabled: true })
    );
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.logs).toHaveLength(2);
  });

  it("advances cursor to newest timestamp after fetch", async () => {
    const ts1 = String(Date.now() * 1_000_000 + 1_000_000);
    const ts2 = String(Date.now() * 1_000_000 + 5_000_000);
    vi.spyOn(logsApi, "tailLogs").mockResolvedValue({
      logs: [makeLog("1", ts1), makeLog("2", ts2)],
      total: 2,
      facets: {},
    });
    renderHook(() => useLiveTail({ tenantId: "t1", enabled: true }));
    await act(async () => { await Promise.resolve(); });
    await act(async () => {
      vi.advanceTimersByTime(5_000);
      await Promise.resolve();
    });
    const calls = vi.mocked(logsApi.tailLogs).mock.calls;
    expect(calls[1][1]).toMatchObject({ since_unix_nano: ts2 });
  });

  it("caps accumulator at 500 rows across two ticks", async () => {
    const make300 = (offset: number) =>
      Array.from({ length: 300 }, (_, i) =>
        makeLog(String(offset + i), String(offset + i + 1))
      );
    vi.spyOn(logsApi, "tailLogs")
      .mockResolvedValueOnce({ logs: make300(0), total: 300, facets: {} })
      .mockResolvedValue({ logs: make300(300), total: 300, facets: {} });

    const { result } = renderHook(() =>
      useLiveTail({ tenantId: "t1", enabled: true })
    );
    await act(async () => { await Promise.resolve(); });
    expect(result.current.logs).toHaveLength(300);
    await act(async () => {
      vi.advanceTimersByTime(5_000);
      await Promise.resolve();
    });
    expect(result.current.logs).toHaveLength(500);
  });

  it("resets state when disabled", async () => {
    vi.spyOn(logsApi, "tailLogs").mockResolvedValue({
      logs: [makeLog("1", "1000")],
      total: 1,
      facets: {},
    });
    const { result, rerender } = renderHook(
      ({ enabled }) => useLiveTail({ tenantId: "t1", enabled }),
      { initialProps: { enabled: true } }
    );
    await act(async () => { await Promise.resolve(); });
    expect(result.current.logs).toHaveLength(1);
    rerender({ enabled: false });
    expect(result.current.logs).toEqual([]);
    expect(result.current.error).toBeNull();
  });

  it("surfaces error and keeps accumulator on fetch failure", async () => {
    vi.spyOn(logsApi, "tailLogs").mockRejectedValue(new Error("network error"));
    const { result } = renderHook(() =>
      useLiveTail({ tenantId: "t1", enabled: true })
    );
    await act(async () => { await Promise.resolve(); });
    expect(result.current.error?.message).toBe("network error");
    expect(result.current.logs).toEqual([]);
  });
});
