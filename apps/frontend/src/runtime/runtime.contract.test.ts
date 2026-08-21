import { beforeEach, describe, expect, it, vi } from "vitest";
import { httpRuntime } from "./httpRuntime";
import { playgroundRuntime } from "./playgroundRuntime";
import type { RuntimeApi } from "./types";

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

    it("traces.search returns the TraceListResponse shape", async () => {
      const result = await runtime.traces.search(MOCK_TENANT_ID, { limit: 10 });
      expect(Array.isArray(result.traces)).toBe(true);
      expect(typeof result.total).toBe("number");
      expect(typeof result.facets).toBe("object");
    });

    it("traces.histogram returns the TraceHistogramResponse shape", async () => {
      const result = await runtime.traces.histogram(MOCK_TENANT_ID, { buckets: 12 });
      expect(Array.isArray(result.buckets)).toBe(true);
      for (const bucket of result.buckets) {
        expect(typeof bucket.start_ms).toBe("number");
        expect(typeof bucket.end_ms).toBe("number");
        expect(typeof bucket.count).toBe("number");
      }
    });
  });
}

describe("runtime contract", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({
        ok: true,
        json: async () => ({ traces: [], total: 0, facets: {}, buckets: [] }),
      })
    );
    vi.stubGlobal("window", { location: { origin: "http://localhost" } });
  });

  runContract("http", httpRuntime);
  runContract("playground", playgroundRuntime);
});
