import { afterEach, describe, expect, test, vi } from "vitest";
import { checkWebGpuSupport } from "./webllmEngine";

describe("checkWebGpuSupport", () => {
  afterEach(() => {
    // @ts-expect-error -- test cleanup of a non-standard navigator property
    delete navigator.gpu;
  });

  test("not supported when navigator.gpu is absent", async () => {
    const result = await checkWebGpuSupport();
    expect(result).toEqual({
      supported: false,
      reason: "WebGPU is not available in this browser",
    });
  });

  test("supported when requestAdapter resolves an adapter", async () => {
    Object.defineProperty(navigator, "gpu", {
      value: { requestAdapter: vi.fn().mockResolvedValue({}) },
      configurable: true,
    });
    const result = await checkWebGpuSupport();
    expect(result).toEqual({ supported: true });
  });

  test("not supported when requestAdapter resolves null", async () => {
    Object.defineProperty(navigator, "gpu", {
      value: { requestAdapter: vi.fn().mockResolvedValue(null) },
      configurable: true,
    });
    const result = await checkWebGpuSupport();
    expect(result).toEqual({
      supported: false,
      reason: "No compatible GPU adapter found",
    });
  });

  test("not supported and surfaces the error when requestAdapter rejects", async () => {
    Object.defineProperty(navigator, "gpu", {
      value: { requestAdapter: vi.fn().mockRejectedValue(new Error("boom")) },
      configurable: true,
    });
    const result = await checkWebGpuSupport();
    expect(result.supported).toBe(false);
    expect(result.reason).toContain("boom");
  });
});
