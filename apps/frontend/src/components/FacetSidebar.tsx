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
    <aside className="facet-sidebar" style={{ width: "250px", paddingRight: "1rem", borderRight: "1px solid var(--border-dim)" }}>
      <div className="field-label" style={{ marginBottom: "1rem" }}>Facets</div>
      {Object.entries(facets).map(([field, values]) => (
        <div key={field} style={{ marginBottom: "1.5rem" }}>
          <h3 style={{ fontSize: "0.75rem", textTransform: "uppercase", color: "var(--text-muted)", marginBottom: "0.5rem" }}>
            {field.replace("_", " ")}
          </h3>
          <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
            {values.map((v) => (
              <li 
                key={v.value} 
                onClick={() => onFacetClick(field, v.value)}
                style={{ 
                  display: "flex", 
                  justifyContent: "space-between", 
                  fontSize: "0.85rem", 
                  cursor: "pointer",
                  padding: "0.25rem 0",
                  color: "var(--text-primary)"
                }}
                className="facet-item"
              >
                <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", marginRight: "0.5rem" }}>
                  {v.value}
                </span>
                <span style={{ color: "var(--text-muted)", flexShrink: 0 }}>{v.count}</span>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </aside>
  );
}
