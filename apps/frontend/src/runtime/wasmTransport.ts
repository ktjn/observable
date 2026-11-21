import type { HttpMethod, RequestTransport, RouteRequest } from "./requestTransport";

/**
 * The in-process local-engine transport (ADR-047 §1). Each capability's
 * routes are registered here as they migrate off their own hand-written
 * playgroundRuntime.ts implementation; `wasmTransport` is additive and does
 * not yet cover every route `httpTransport` does.
 */

type WasmHandler = (params: Record<string, string>, route: RouteRequest) => Promise<unknown>;

interface RegisteredRoute {
  method: HttpMethod;
  segments: string[];
  handler: WasmHandler;
}

const routes: RegisteredRoute[] = [];

/** `pattern` segments prefixed with `:` bind that segment into `params`. */
export function registerWasmRoute(method: HttpMethod, pattern: string, handler: WasmHandler): void {
  routes.push({ method, segments: pattern.split("/").filter(Boolean), handler });
}

function matchPath(routeSegments: string[], pathSegments: string[]): Record<string, string> | null {
  if (routeSegments.length !== pathSegments.length) return null;
  const params: Record<string, string> = {};
  for (let i = 0; i < routeSegments.length; i++) {
    const routeSegment = routeSegments[i];
    const pathSegment = pathSegments[i];
    if (routeSegment.startsWith(":")) {
      params[routeSegment.slice(1)] = decodeURIComponent(pathSegment);
    } else if (routeSegment !== pathSegment) {
      return null;
    }
  }
  return params;
}

export const wasmTransport: RequestTransport = {
  async dispatch<T>(route: RouteRequest): Promise<T> {
    const pathSegments = route.path.split("/").filter(Boolean);
    for (const candidate of routes) {
      if (candidate.method !== route.method) continue;
      const params = matchPath(candidate.segments, pathSegments);
      if (params === null) continue;
      return candidate.handler(params, route) as Promise<T>;
    }
    throw new Error(`no wasm handler registered for ${route.method} ${route.path}`);
  },
};

const DEMO_TENANT_ID = "00000000-0000-0000-0000-000000000001";
const DEMO_TENANT_NAME = "observable";

registerWasmRoute("GET", "/v1/tenants", async () => ({
  tenants: [{ id: DEMO_TENANT_ID, name: DEMO_TENANT_NAME }],
}));

registerWasmRoute("GET", "/v1/tenants/:id/environments", async () => ({
  environments: [{ environment: "production" }, { environment: "staging" }],
}));
