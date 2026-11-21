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

function toNanos(ms: number): string {
  return String(BigInt(Math.floor(ms)) * 1_000_000n);
}

function isoToMs(iso: unknown, fallback: number): number {
  return typeof iso === "string" ? Date.parse(iso) : fallback;
}

registerWasmRoute("GET", "/v1/topology", async (_params, route) => {
  const { executeTopology } = await import("../playground/engineClient");
  const query = route.query ?? {};
  const nowMs = Date.now();
  const fromMs = isoToMs(query.from, nowMs - 3_600_000);
  const toMs = isoToMs(query.to, nowMs);
  const { edges } = await executeTopology({
    fromNs: toNanos(fromMs),
    toNs: toNanos(toMs),
    environment: query.environment as string | undefined,
    service: query.service as string | undefined,
  });
  return { edges };
});

registerWasmRoute("GET", "/v1/events/changes", async (_params, route) => {
  const { executeChangeEvents } = await import("../playground/engineClient");
  const query = route.query ?? {};
  const nowMs = Date.now();
  const fromMs = isoToMs(query.start_time, nowMs - 3_600_000);
  const toMs = isoToMs(query.end_time, nowMs);
  const { changeEvents } = await executeChangeEvents({
    fromNs: toNanos(fromMs),
    toNs: toNanos(toMs),
    service: query.service_name as string | undefined,
    environment: query.environment as string | undefined,
    eventType: query.event_type as string | undefined,
    limit: typeof query.limit === "number" ? query.limit : 50,
  });
  return { items: changeEvents };
});
