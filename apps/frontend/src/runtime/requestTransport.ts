/**
 * The seam `api/*.ts` functions dispatch through instead of calling `fetch`
 * directly. `httpTransport` and `wasmTransport` are the only two
 * implementations (ADR-047 §1): the real backend, and the in-process local
 * engine. A route is the same `{method, path}` key in both — that's what
 * lets a capability move from one transport to the other without its
 * callers changing.
 */

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

export interface RouteRequest {
  method: HttpMethod;
  path: string;
  query?: Record<string, string | number | boolean | undefined>;
  body?: unknown;
  /**
   * Extra headers for routes outside the normal cookie-session model (e.g.
   * the pre-login setup/bootstrap flow's `x-api-key`). Ignored by
   * `wasmTransport` — there's no real request to attach them to locally.
   */
  headers?: Record<string, string>;
}

export interface RequestTransport {
  dispatch<T>(route: RouteRequest): Promise<T>;
}

/**
 * Mode is strict and boot-time (ADR-047 §1): reuses the same
 * `VITE_OBSERVABLE_RUNTIME` flag `useRuntime.tsx` already selects
 * `httpRuntime`/`playgroundRuntime` with, so there is exactly one switch for
 * the whole app, not a per-module decision.
 */
export function isPlaygroundMode(): boolean {
  return import.meta.env.VITE_OBSERVABLE_RUNTIME === "playground";
}

/** The transport every migrated `api/*.ts` function dispatches through. */
export async function dispatch<T>(route: RouteRequest): Promise<T> {
  const { httpTransport } = await import("./httpTransport");
  if (!isPlaygroundMode()) return httpTransport.dispatch<T>(route);
  const { wasmTransport } = await import("./wasmTransport");
  return wasmTransport.dispatch<T>(route);
}
