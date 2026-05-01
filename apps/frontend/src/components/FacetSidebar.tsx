import { Facets } from "../api/logs";

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
