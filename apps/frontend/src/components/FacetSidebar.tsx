import { Facets } from "../api/logs";

interface FacetSidebarProps {
  facets?: Facets;
  onFacetClick: (field: string, value: string) => void;
}

export function FacetSidebar({ facets, onFacetClick }: FacetSidebarProps) {
  if (!facets || Object.keys(facets).length === 0) {
    return null;
  }

  return (
    <aside className="facet-sidebar">
      <div className="field-label" style={{ marginBottom: "16px" }}>Facets</div>
      {Object.entries(facets).map(([field, values]) => (
        <div key={field} className="facet-group">
          <h3 className="facet-title">
            {field.replace("_", " ")}
          </h3>
          <ul className="facet-list">
            {values.map((v) => (
              <li 
                key={v.value} 
                onClick={() => onFacetClick(field, v.value)}
                className="facet-item"
              >
                <span className="facet-value">{v.value}</span>
                <span className="facet-count">{v.count}</span>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </aside>
  );
}
