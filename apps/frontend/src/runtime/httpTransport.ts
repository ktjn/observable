import type { RequestTransport, RouteRequest } from "./requestTransport";

/**
 * Carries the response status so callers that need to branch on a specific
 * status (e.g. 404 vs 403 meaning different things) can do so without
 * string-matching the message — additive; every other caller still just
 * reads `.message`.
 */
export class HttpTransportError extends Error {
  constructor(message: string, readonly status: number) {
    super(message);
    this.name = "HttpTransportError";
  }
}

function buildUrl(path: string, query: RouteRequest["query"]): string {
  if (!query) return path;
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined) params.set(key, String(value));
  }
  const qs = params.toString();
  return qs ? `${path}?${qs}` : path;
}

export const httpTransport: RequestTransport = {
  async dispatch<T>(route: RouteRequest): Promise<T> {
    const url = buildUrl(route.path, route.query);
    const headers = {
      ...(route.body !== undefined ? { "Content-Type": "application/json" } : {}),
      ...route.headers,
    };
    const res = await fetch(url, {
      method: route.method,
      credentials: "include",
      ...(Object.keys(headers).length > 0 ? { headers } : {}),
      ...(route.body !== undefined ? { body: JSON.stringify(route.body) } : {}),
    });
    if (!res.ok) {
      throw new HttpTransportError(`${route.method} ${route.path} failed: ${res.status}`, res.status);
    }
    if (res.status === 204) return undefined as T;
    return res.json() as Promise<T>;
  },
};
