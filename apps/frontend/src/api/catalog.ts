import { catalogPageSize, type CatalogRuntime } from "../runtime/catalogContract";

async function get<T>(tenant: string, path: string, signal?: AbortSignal, root = "/v1/catalog/"): Promise<T> {
  const response = await fetch(`${root}${path}`, {
    credentials: "include",
    cache: "no-store",
    headers: { "X-Tenant-ID": tenant },
    signal,
  });
  if (!response.ok) {
    throw new CatalogHttpError(response.status);
  }
  return response.json() as Promise<T>;
}

class CatalogHttpError extends Error {
  constructor(readonly status: number) {
    super(`Catalog request failed (${status})`);
  }
}

async function record<T>(tenant: string, path: string, signal?: AbortSignal, root?: string): Promise<T | null> {
  try {
    return await get<T>(tenant, path, signal, root);
  } catch (error) {
    if (error instanceof CatalogHttpError && error.status === 404) return null;
    throw error;
  }
}

function paging(offset: number) {
  return new URLSearchParams({ offset: String(offset), limit: String(catalogPageSize) });
}

const reviewRoot = "/v1/review/services/";

export const httpCatalog: CatalogRuntime = {
  services(tenant, offset, query, signal) {
    const params = paging(offset);
    if (query) params.set("q", query);
    return get(tenant, `${query ? "search" : "services"}?${params}`, signal);
  },
  service: (tenant, id, signal) => record(tenant, `services/${encodeURIComponent(id)}`, signal),
  coverage: (tenant, signal) => get(tenant, "coverage", signal),
  assessment: (tenant, id, signal) => record(tenant, `services/${encodeURIComponent(id)}/coverage`, signal),
  gaps: (tenant, signal) => get(tenant, "gaps", signal),
  providers: (tenant, offset, signal) => get(tenant, `providers?${paging(offset)}`, signal),
  provider: (tenant, id, signal) => record(tenant, `providers/${encodeURIComponent(id)}`, signal),
  providerGaps: (tenant, signal) => get(tenant, "providers/gaps", signal),
  ownerlessAssets: (tenant, signal) => get(tenant, "assets?ownerless=true", signal),
  // The service review is composed by observable-query from catalog, health and objectives.
  review: (tenant, id, environmentClass, signal) => record(tenant,
    `${encodeURIComponent(id)}?${new URLSearchParams({ environment_class: environmentClass })}`, signal, reviewRoot),
  reviews: (tenant, environmentClass, scope, offset, signal, options) => {
    const params = new URLSearchParams({
      environment_class: environmentClass,
      context: scope,
      offset: String(offset),
    });
    if (options?.from && options?.to) {
      params.set("from", options.from);
      params.set("to", options.to);
    }
    if (options?.accountable) {
      params.set("accountable", "true");
    }
    return get(tenant, `?${params}`, signal, reviewRoot.slice(0, -1));
  },
  overview: (tenant, environmentClass, offset, signal) => get(tenant,
    `?${new URLSearchParams({ environment_class: environmentClass, offset: String(offset) })}`, signal, "/v1/overview/services"),
};
