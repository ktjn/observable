import { Facets } from "../api/logs";

export const AUTHORIZED_FACET_DIMENSIONS = new Set([
  "service",
  "service_name",
  "environment",
  "status",
  "level",
  "channel",
]);

export function filterAuthorizedFacets(
  facets: Facets | undefined,
  allowed: Set<string> = AUTHORIZED_FACET_DIMENSIONS,
): Facets | undefined {
  if (!facets) return undefined;
  const filtered: Facets = {};
  for (const [field, values] of Object.entries(facets)) {
    if (allowed.has(field) && values.length > 0) {
      filtered[field] = values;
    }
  }
  return Object.keys(filtered).length > 0 ? filtered : undefined;
}

export function findRestrictedFilterParam(
  search: string,
  allowed: Set<string> = AUTHORIZED_FACET_DIMENSIONS,
): string | null {
  const params = new URLSearchParams(search);
  const dim = params.get("dimension")?.trim();
  if (dim && !allowed.has(dim)) return dim;
  const filter = params.get("filter")?.trim();
  if (filter && filter.includes(":")) {
    const field = filter.split(":")[0]?.trim();
    if (field && !allowed.has(field)) return field;
  }
  return null;
}

interface FacetSidebarProps {
  facets?: Facets;
  onFacetClick: (field: string, value: string) => void;
  ariaLabel?: string;
}

export function FacetSidebar({
  facets,
  onFacetClick,
  ariaLabel = "Facets",
}: FacetSidebarProps) {
  if (!facets || Object.keys(facets).length === 0) {
    return null;
  }

  return (
    <aside className="facet-sidebar" aria-label={ariaLabel}>
      <div className="field-label mb-4">Facets</div>
      {Object.entries(facets).map(([field, values]) => (
        <div key={field} className="facet-group">
          <h2 className="facet-title">
            {field.replace("_", " ")}
          </h2>
          <div className="facet-list">
            {values.map((v) => (
              <div
                key={v.value}
                role="button"
                tabIndex={0}
                aria-label={`${v.value} ${v.count}`}
                onClick={() => onFacetClick(field, v.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    onFacetClick(field, v.value);
                  }
                }}
                className="facet-item"
              >
                <span className="facet-value">{v.value}</span>
                <span className="facet-count">{v.count}</span>
              </div>
            ))}
          </div>
        </div>
      ))}
    </aside>
  );
}
