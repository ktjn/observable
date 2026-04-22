export type SelfObservabilityMode = "self" | "observer_instance";

export type SelfObservabilityRoute = {
  mode: SelfObservabilityMode;
  otlpEndpoint?: string;
};

type FrontendEnv = Record<string, string | undefined>;

export function resolveSelfObservabilityRoute(env: FrontendEnv): SelfObservabilityRoute {
  const mode = env.VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE ?? "self";
  if (mode !== "self" && mode !== "observer_instance") {
    throw new Error(`unsupported self-observability mode: ${mode}`);
  }

  const otlpEndpoint = env.VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT?.trim();

  return {
    mode,
    otlpEndpoint: otlpEndpoint ? otlpEndpoint : undefined,
  };
}

export const selfObservabilityRoute = resolveSelfObservabilityRoute(import.meta.env);
