import type { RequestTransport, RouteRequest } from "./requestTransport";

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
    const res = await fetch(url, {
      method: route.method,
      credentials: "include",
      ...(route.body !== undefined
        ? { headers: { "Content-Type": "application/json" }, body: JSON.stringify(route.body) }
        : {}),
    });
    if (!res.ok) {
      throw new Error(`${route.method} ${route.path} failed: ${res.status}`);
    }
    if (res.status === 204) return undefined as T;
    return res.json() as Promise<T>;
  },
};
