import { lazy } from "react";
import { BookOpen } from "lucide-react";

export const catalogViews = [
  { id: "services", label: "Service records" },
  { id: "overview", label: "Service overview" },
  { id: "review", label: "Service review" },
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
  const review = view === "review";
  const overview = view === "overview";
  const sorts = ["name", "state", "age"] as const;
  const states = ["normal", "degraded", "unavailable", "planned_maintenance", "unknown"] as const;
  return {
    view: view === "services" ? undefined : view,
    q: view === "services" && validQuery ? q : undefined,
    offset: Number.isSafeInteger(offset) && offset > 0 ? offset : undefined,
    // Reviews always name one class; production is the default, never a mix.
    env: (review || overview) && search.env === "non_production" ? "non_production" as const : undefined,
    scope: review && search.scope === "all" ? "all" as const : undefined,
    // Overview sort, state filter and inspected service live in the URL, so a refresh keeps them.
    sort: overview ? sorts.find((s) => s === search.sort && s !== "state") : undefined,
    state: overview ? states.find((s) => s === search.state) : undefined,
    selected: overview && typeof search.selected === "string" && /^[0-9a-f-]{36}$/i.test(search.selected)
      ? search.selected : undefined,
  };
}
