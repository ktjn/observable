// WebLLM (in-browser, WebGPU-backed) engine wrapper.
//
// IMPORTANT: `@mlc-ai/web-llm` is a large package (~6.8MB / ~2.4MB gzip). It must
// only ever be loaded via dynamic `import()` inside the functions below — never as
// a static top-level import — so bundlers code-split it into its own chunk that
// only loads when a caller actually exercises the WebLLM path (Setup page WebLLM
// branch, or a future WebLLM NLQ query).

export interface WebLlmInitProgress {
  progress: number; // 0-1
  text: string;
}

export interface WebLlmEngineHandle {
  complete(systemPrompt: string, userTurn: string): Promise<string>;
  dispose(): void;
}

export interface WebGpuSupport {
  supported: boolean;
  reason?: string;
}

export interface WebLlmModelOption {
  modelId: string;
  label: string;
}

/**
 * Checks WebGPU availability using only the browser's own `navigator.gpu` — does
 * NOT import `@mlc-ai/web-llm`, since this check needs to be cheap and safe to run
 * before committing to loading the (large) engine package.
 */
export async function checkWebGpuSupport(): Promise<WebGpuSupport> {
  if (typeof navigator === "undefined" || !("gpu" in navigator)) {
    return { supported: false, reason: "WebGPU is not available in this browser" };
  }
  try {
    const gpu = (navigator as unknown as { gpu: { requestAdapter(): Promise<unknown> } }).gpu;
    const adapter = await gpu.requestAdapter();
    return adapter ? { supported: true } : { supported: false, reason: "No compatible GPU adapter found" };
  } catch (e) {
    return { supported: false, reason: String(e) };
  }
}

/** Prebuilt model catalog for the Setup page picker: id + a short display label. */
export async function listAvailableModels(): Promise<WebLlmModelOption[]> {
  const webllm = await import("@mlc-ai/web-llm");
  return webllm.prebuiltAppConfig.model_list.map((m) => ({
    modelId: m.model_id,
    label: m.model_id,
  }));
}

// Module-level cache so repeated calls with the same modelId reuse the already
// loaded engine instead of re-downloading/reloading it.
let cached: { modelId: string; handle: WebLlmEngineHandle } | null = null;

export async function getOrCreateEngine(
  modelId: string,
  onProgress: (p: WebLlmInitProgress) => void,
): Promise<WebLlmEngineHandle> {
  if (cached && cached.modelId === modelId) {
    return cached.handle;
  }
  if (cached) {
    cached.handle.dispose();
    cached = null;
  }

  const webllm = await import("@mlc-ai/web-llm");
  const engine = await webllm.CreateMLCEngine(modelId, {
    initProgressCallback: (report) => {
      const progress = Number.isFinite(report.progress)
        ? Math.min(1, Math.max(0, report.progress))
        : 0;
      onProgress({ progress, text: report.text });
    },
  });

  let disposed = false;
  const handle: WebLlmEngineHandle = {
    async complete(systemPrompt: string, userTurn: string): Promise<string> {
      if (disposed) {
        throw new Error("WebLLM engine has been disposed");
      }
      const result = await engine.chat.completions.create({
        model: modelId,
        temperature: 0,
        response_format: { type: "json_object" },
        messages: [
          { role: "system", content: systemPrompt },
          { role: "user", content: userTurn },
        ],
      });
      return result.choices[0]?.message.content ?? "";
    },
    dispose(): void {
      if (disposed) return;
      disposed = true;
      void engine.unload();
    },
  };

  cached = { modelId, handle };
  return handle;
}
