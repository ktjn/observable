import { catalogPageSize, type CatalogRuntime } from "../runtime/catalogContract";

async function get<T>(tenant: string, path: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(`/v1/catalog/${path}`, {
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

async function record<T>(tenant: string, path: string, signal?: AbortSignal): Promise<T | null> {
  try {
    return await get<T>(tenant, path, signal);
  } catch (error) {
    if (error instanceof CatalogHttpError && error.status === 404) return null;
    throw error;
  }
}

function paging(offset: number) {
  return new URLSearchParams({ offset: String(offset), limit: String(catalogPageSize) });
}

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
};
