import { describe, expect, it } from "vitest";
import { resolveSelfObservabilityRoute } from "./selfObservability";

describe("resolveSelfObservabilityRoute", () => {
  it("defaults to self mode without an endpoint", () => {
    const route = resolveSelfObservabilityRoute({});

    expect(route).toEqual({ mode: "self", otlpEndpoint: undefined });
  });

  it("uses observer instance endpoint when configured", () => {
    const route = resolveSelfObservabilityRoute({
      VITE_OBSERVABLE_SELF_OBSERVABILITY_MODE: "observer_instance",
      VITE_OBSERVABLE_SELF_OBSERVABILITY_OTLP_ENDPOINT: "https://observer.example/otlp",
    });

    expect(route).toEqual({
      mode: "observer_instance",
      otlpEndpoint: "https://observer.example/otlp",
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
