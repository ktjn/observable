export type SelfObservabilityMode = "self" | "observer_instance";

export type SelfObservabilityRoute = {
  mode: SelfObservabilityMode;
  tenant: string;
  otlpEndpoint?: string;
  deploymentEnvironment?: string;
  cluster?: string;
  release?: string;
};

type FrontendEnv = Record<string, string | undefined>;

export function resolveSelfObservabilityRoute(env: FrontendEnv): SelfObservabilityRoute {
  const mode = env.VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE ?? "self";
  if (mode !== "self" && mode !== "observer_instance") {
    throw new Error(`unsupported self-observability mode: ${mode}`);
  }

  const otlpEndpoint = env.VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT?.trim();
  const tenant = env.VITE_OBSERVABLE_SELF_OBSERVABILITY_TENANT?.trim() || "system";

  const route: SelfObservabilityRoute = {
    mode,
    tenant,
    otlpEndpoint: otlpEndpoint ? otlpEndpoint : undefined,
  };

  const deploymentEnvironment = optionalEnv(
    env.VITE_OBSERVABLE_SELF_OBSERVABILITY_DEPLOYMENT_ENVIRONMENT
  );
  const cluster = optionalEnv(env.VITE_OBSERVABLE_SELF_OBSERVABILITY_CLUSTER);
  const release = optionalEnv(env.VITE_OBSERVABLE_SELF_OBSERVABILITY_RELEASE);

  if (deploymentEnvironment) route.deploymentEnvironment = deploymentEnvironment;
  if (cluster) route.cluster = cluster;
  if (release) route.release = release;

  return route;
}

export const selfObservabilityRoute = resolveSelfObservabilityRoute(import.meta.env);

function optionalEnv(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}
