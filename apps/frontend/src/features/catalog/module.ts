import { lazy } from "react";
import { BookOpen } from "lucide-react";

export const catalogViews = [
  { id: "services", label: "Service records" },
  { id: "coverage", label: "Coverage" },
  { id: "gaps", label: "Gaps" },
  { id: "providers", label: "Providers" },
  { id: "assets", label: "Ownerless assets" },
] as const;

export const catalogModule = {
  navigation: { id: "catalog", label: "Catalog", to: "/catalog", icon: BookOpen },
  workspace: {
    path: "/catalog",
    component: lazy(() => import("./CatalogPage")),
    validateSearch: validateCatalogSearch,
  },
  service: {
    path: "/catalog/services/$serviceId",
    component: lazy(() => import("./CatalogRecords").then(({ CatalogServicePage }) => ({ default: CatalogServicePage }))),
  },
  provider: {
    path: "/catalog/providers/$providerId",
    component: lazy(() => import("./CatalogRecords").then(({ CatalogProviderPage }) => ({ default: CatalogProviderPage }))),
  },
} as const;

export function validateCatalogSearch(search: Record<string, unknown>) {
  const view = catalogViews.find(({ id }) => id === search.view)?.id ?? "services";
  const q = typeof search.q === "string" ? search.q.trim() : "";
  const validQuery = Array.from(q).length >= 2 && new TextEncoder().encode(q).length <= 200;
  const offset = typeof search.offset === "number" ? search.offset : 0;
  return {
    view: view === "services" ? undefined : view,
    q: view === "services" && validQuery ? q : undefined,
    offset: Number.isSafeInteger(offset) && offset > 0 ? offset : undefined,
  };
}
