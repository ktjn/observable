import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SelfObservabilityRoute } from "./selfObservability";
import {
  initSelfObservabilityRuntime,
  recordSelfObservabilityRouteChange,
} from "./selfObservabilityRuntime";

const route: SelfObservabilityRoute = {
  mode: "observer_instance",
  otlpEndpoint: "https://observer.example/otlp",
  tenant: "system",
  deploymentEnvironment: "staging",
  cluster: "kind-observable",
  release: "observable-2026-04-22",
};

const capturedEvents: CustomEvent[] = [];

describe("selfObservabilityRuntime", () => {
  let cleanup: (() => void) | undefined;

  beforeEach(() => {
    vi.spyOn(console, "info").mockImplementation(() => undefined);
    window.addEventListener("observable:self-observability", captureEvent as EventListener);
  });

  afterEach(() => {
    cleanup?.();
    cleanup = undefined;
    capturedEvents.length = 0;
    window.removeEventListener("observable:self-observability", captureEvent as EventListener);
    vi.restoreAllMocks();
  });

  it("emits an init event with the configured route", () => {
    cleanup = initSelfObservabilityRuntime(route);

    expect(capturedEvents[0]?.detail).toMatchObject({
      kind: "init",
      mode: "observer_instance",
      tenant: "system",
      otlpEndpoint: "https://observer.example/otlp",
      deploymentEnvironment: "staging",
      cluster: "kind-observable",
      release: "observable-2026-04-22",
    });
  });

  it("emits route changes without exposing secret material", () => {
    recordSelfObservabilityRouteChange(route, "/services/payments");

    expect(capturedEvents[0]?.detail).toMatchObject({
      kind: "route-change",
      pathname: "/services/payments",
      tenant: "system",
    });
    expect(capturedEvents[0]?.detail).not.toHaveProperty("headers");
  });

  it("captures runtime errors", () => {
    cleanup = initSelfObservabilityRuntime(route);

    window.dispatchEvent(new ErrorEvent("error", { message: "render failed" }));

    expect(capturedEvents[capturedEvents.length - 1]?.detail).toMatchObject({
      kind: "runtime-error",
      message: "render failed",
    });
  });

  it("captures resource load errors", () => {
    cleanup = initSelfObservabilityRuntime(route);

    const script = document.createElement("script");
    script.src = "https://cdn.example/app.js";
    document.body.appendChild(script);
    script.dispatchEvent(new Event("error"));

    expect(capturedEvents[capturedEvents.length - 1]?.detail).toMatchObject({
      kind: "resource-load-error",
      resourceUrl: "https://cdn.example/app.js",
    });
  });
});

function captureEvent(event: Event) {
  capturedEvents.push(event as CustomEvent);
}
