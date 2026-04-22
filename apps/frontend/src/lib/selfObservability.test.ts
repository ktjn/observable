import { describe, expect, it } from "vitest";
import { resolveSelfObservabilityRoute } from "./selfObservability";

describe("resolveSelfObservabilityRoute", () => {
  it("defaults to self mode without an endpoint", () => {
    const route = resolveSelfObservabilityRoute({});

    expect(route).toEqual({ mode: "self", tenant: "system", otlpEndpoint: undefined });
  });

  it("uses observer instance endpoint when configured", () => {
    const route = resolveSelfObservabilityRoute({
      VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE: "observer_instance",
      VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT: "https://observer.example/otlp",
    });

    expect(route).toEqual({
      mode: "observer_instance",
      tenant: "system",
      otlpEndpoint: "https://observer.example/otlp",
    });
  });

  it("includes non-secret runtime routing metadata", () => {
    const route = resolveSelfObservabilityRoute({
      VITE_OBSERVABLE_SELF_OBSERVABILITY_TENANT: "system",
      VITE_OBSERVABLE_SELF_OBSERVABILITY_DEPLOYMENT_ENVIRONMENT: "staging",
      VITE_OBSERVABLE_SELF_OBSERVABILITY_CLUSTER: "kind-observable",
      VITE_OBSERVABLE_SELF_OBSERVABILITY_RELEASE: "observable-2026-04-22",
    });

    expect(route).toMatchObject({
      tenant: "system",
      deploymentEnvironment: "staging",
      cluster: "kind-observable",
      release: "observable-2026-04-22",
    });
  });

  it("rejects unknown modes", () => {
    expect(() =>
      resolveSelfObservabilityRoute({
        VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE: "mirror",
      })
    ).toThrow("unsupported self-observability mode");
  });
});
