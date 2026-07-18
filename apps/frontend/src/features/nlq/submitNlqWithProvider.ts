import {
  submitNlqQuery,
  prepareNlqQuery,
  completeNlqQuery,
  type NlqRequest,
  type NlqResponse,
} from "../../api/nlq";
import { checkWebGpuSupport, getOrCreateEngine } from "../../lib/webllm/webllmEngine";

// Hard ceiling on repair round-trips. The backend enforces the real cap
// server-side (via the session token's repair_attempt counter) and always
// eventually returns "final", so this loop terminates on its own — this is
// only a defense-in-depth guard against a latent hang if that invariant is
// ever violated by a backend bug.
const MAX_REPAIR_ITERATIONS = 5;

export type NlqLoadingPhase = "checking_gpu" | "preparing" | "downloading_model" | "generating";

export interface NlqProviderConfig {
  provider: "remote" | "webllm";
  webllmModel?: string | null;
}

/**
 * Provider-aware NLQ submission. Routes through the two-phase WebLLM flow
 * (prepare -> local browser inference -> complete, with the server-capped
 * repair loop) when `config.provider === "webllm"`; otherwise the existing
 * single-call remote path, unchanged.
 *
 * Shared by every NLQ-submitting surface (NlqPanel, QueryFilterInput, and any
 * future one) so provider-awareness can't drift between them the way it did
 * when only NlqPanel was wired up initially.
 */
export async function submitNlqWithProvider(
  tenantId: string,
  config: NlqProviderConfig,
  request: NlqRequest,
  onPhase?: (phase: NlqLoadingPhase) => void,
): Promise<NlqResponse> {
  if (config.provider !== "webllm") {
    return submitNlqQuery(tenantId, request);
  }

  onPhase?.("checking_gpu");
  const gpuSupport = await checkWebGpuSupport();
  if (!gpuSupport.supported) {
    throw new Error(
      `WebLLM is configured but this browser doesn't support it: ${gpuSupport.reason}`,
    );
  }

  onPhase?.("preparing");
  const prepared = await prepareNlqQuery(tenantId, request);
  if (prepared.type === "final") {
    return prepared.response;
  }

  const model = config.webllmModel;
  if (!model) {
    throw new Error("No WebLLM model configured. Set one on the Setup page.");
  }

  onPhase?.("downloading_model");
  const engine = await getOrCreateEngine(model, () => {
    // Text-only loading label is sufficient for now; progress detail
    // (byte counts etc.) is intentionally not surfaced.
  });

  onPhase?.("generating");
  let raw = await engine.complete(prepared.system_prompt, prepared.question);
  let result = await completeNlqQuery(tenantId, prepared.session_token, raw);

  let iterations = 0;
  while (result.type === "needs_repair") {
    iterations += 1;
    if (iterations > MAX_REPAIR_ITERATIONS) {
      throw new Error("NLQ repair loop exceeded the maximum number of attempts");
    }
    onPhase?.("generating");
    raw = await engine.complete(prepared.system_prompt, result.repair_prompt);
    result = await completeNlqQuery(tenantId, prepared.session_token, raw);
  }

  return result.response;
}
