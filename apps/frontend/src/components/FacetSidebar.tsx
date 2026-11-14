import { useState } from "react";
import { Facets } from "../api/logs";

export const AUTHORIZED_FACET_DIMENSIONS = new Set([
  "service",
  "service_name",
  "environment",
  "status",
  "level",
  "channel",
]);

export const RESTRICTED_ATTRIBUTE_NAMES = new Set([
  "password",
  "secret",
  "token",
  "api_key",
  "ssn",
  "credit_card",
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
  onCustomAttributeFilter?: (field: string, value: string) => void;
  restrictedAttributes?: Set<string>;
  ariaLabel?: string;
}

export function FacetSidebar({
  facets,
  onFacetClick,
  onCustomAttributeFilter,
  restrictedAttributes = RESTRICTED_ATTRIBUTE_NAMES,
  ariaLabel = "Facets",
}: FacetSidebarProps) {
  const [attrField, setAttrField] = useState("");
  const [attrValue, setAttrValue] = useState("");
  const [attrError, setAttrError] = useState<string | null>(null);

  if ((!facets || Object.keys(facets).length === 0) && !onCustomAttributeFilter) {
    return null;
  }

  function handleCustomSubmit(e: React.FormEvent) {
    e.preventDefault();
    const field = attrField.trim();
    const value = attrValue.trim();
    if (!field || !value) return;
    if (restrictedAttributes.has(field.toLowerCase())) {
      setAttrError(`Attribute "${field}" is restricted by authorization policy.`);
      return;
    }
    setAttrError(null);
    onCustomAttributeFilter?.(field, value);
    setAttrValue("");
  }

  return (
    <aside className="facet-sidebar" aria-label={ariaLabel}>
      <div className="field-label mb-4">Facets</div>
      {facets && Object.entries(facets).map(([field, values]) => (
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
      {onCustomAttributeFilter && (
        <form
          onSubmit={handleCustomSubmit}
          data-testid="custom-attribute-filter"
          className="mt-4 border-t border-[var(--border)] pt-3 space-y-1.5"
        >
          <div className="text-[11px] font-bold uppercase text-[var(--muted)]">
            Any recorded attribute
          </div>
          <input
            type="text"
            aria-label="Attribute name"
            placeholder="Attribute (e.g. build_sha)"
            value={attrField}
            onChange={(e) => setAttrField(e.target.value)}
            className="w-full rounded border border-[var(--border)] bg-[var(--surface-1)] px-2 py-1 text-xs"
          />
          <input
            type="text"
            aria-label="Attribute value"
            placeholder="Value (e.g. canary-99)"
            value={attrValue}
            onChange={(e) => setAttrValue(e.target.value)}
            className="w-full rounded border border-[var(--border)] bg-[var(--surface-1)] px-2 py-1 text-xs"
          />
          {attrError && (
            <div className="text-[11px] text-[var(--bad)]" role="alert">
              {attrError}
            </div>
          )}
          <button
            type="submit"
            className="w-full rounded border border-[var(--border)] px-2 py-1 text-xs font-medium hover:bg-[var(--surface-2)]"
          >
            Filter attribute
          </button>
        </form>
      )}
    </aside>
  );
}
