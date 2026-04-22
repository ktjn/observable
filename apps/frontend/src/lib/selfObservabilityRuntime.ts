import type { SelfObservabilityRoute } from "./selfObservability";

type SelfObservabilityEventKind =
  | "init"
  | "route-change"
  | "runtime-error"
  | "resource-load-error"
  | "unhandled-rejection";

type SelfObservabilityEventDetail = {
  kind: SelfObservabilityEventKind;
  mode: SelfObservabilityRoute["mode"];
  tenant: string;
  otlpEndpoint?: string;
  deploymentEnvironment?: string;
  cluster?: string;
  release?: string;
  pathname?: string;
  message?: string;
  resourceUrl?: string;
};

const EVENT_NAME = "observable:self-observability";

function eventDetail(
  route: SelfObservabilityRoute,
  kind: SelfObservabilityEventKind,
  extra: Omit<
    SelfObservabilityEventDetail,
    | "kind"
    | "mode"
    | "tenant"
    | "otlpEndpoint"
    | "deploymentEnvironment"
    | "cluster"
    | "release"
  > = {}
): SelfObservabilityEventDetail {
  return {
    kind,
    mode: route.mode,
    tenant: route.tenant,
    otlpEndpoint: route.otlpEndpoint,
    deploymentEnvironment: route.deploymentEnvironment,
    cluster: route.cluster,
    release: route.release,
    ...extra,
  };
}

function dispatchSelfObservabilityEvent(detail: SelfObservabilityEventDetail) {
  window.dispatchEvent(new CustomEvent(EVENT_NAME, { detail }));
  console.info("frontend self-observability event", detail);
}

export function initSelfObservabilityRuntime(route: SelfObservabilityRoute): () => void {
  dispatchSelfObservabilityEvent(eventDetail(route, "init", { pathname: window.location.pathname }));

  const onError = (event: ErrorEvent) => {
    dispatchSelfObservabilityEvent(
      eventDetail(route, "runtime-error", {
        pathname: window.location.pathname,
        message: event.message,
      })
    );
  };

  const onUnhandledRejection = (event: PromiseRejectionEvent) => {
    dispatchSelfObservabilityEvent(
      eventDetail(route, "unhandled-rejection", {
        pathname: window.location.pathname,
        message: String(event.reason ?? "unknown"),
      })
    );
  };

  const onResourceError = (event: Event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement)) {
      return;
    }

    let resourceUrl: string | undefined;
    if (target instanceof HTMLScriptElement || target instanceof HTMLImageElement) {
      resourceUrl = target.src;
    } else if (target instanceof HTMLLinkElement) {
      resourceUrl = target.href;
    }

    dispatchSelfObservabilityEvent(
      eventDetail(route, "resource-load-error", {
        pathname: window.location.pathname,
        resourceUrl,
      })
    );
  };

  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onUnhandledRejection);
  window.addEventListener("error", onResourceError, true);

  return () => {
    window.removeEventListener("error", onError);
    window.removeEventListener("unhandledrejection", onUnhandledRejection);
    window.removeEventListener("error", onResourceError, true);
  };
}

export function recordSelfObservabilityRouteChange(
  route: SelfObservabilityRoute,
  pathname: string
) {
  dispatchSelfObservabilityEvent(eventDetail(route, "route-change", { pathname }));
}
