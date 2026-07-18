import { afterEach, describe, expect, test, vi } from "vitest";
import { checkWebGpuSupport } from "./webllmEngine";

// Regression coverage for the "Cannot pass non-string to std::string" crash: this
// build of @mlc-ai/web-llm always calls GrammarCompiler.compileJSONSchema(schema)
// when response_format.type === "json_object", with no fallback for schema being
// unset — passing { type: "json_object" } with no schema string crashes the WASM
// binding. complete() must never send response_format at all.
const mockCreate = vi.fn();
vi.mock("@mlc-ai/web-llm", () => ({
  CreateMLCEngine: vi.fn().mockResolvedValue({
    chat: { completions: { create: mockCreate } },
    unload: vi.fn(),
  }),
}));

describe("getOrCreateEngine complete()", () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  test("never sends response_format (avoids the schema-less json_object crash)", async () => {
    mockCreate.mockResolvedValue({ choices: [{ message: { content: '{"type":"capabilities"}' } }] });
    const { getOrCreateEngine } = await import("./webllmEngine");

    const engine = await getOrCreateEngine("some-model", vi.fn());
    await engine.complete("system prompt", "user question");

    expect(mockCreate).toHaveBeenCalledWith(
      expect.not.objectContaining({ response_format: expect.anything() }),
    );
  });
});

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
