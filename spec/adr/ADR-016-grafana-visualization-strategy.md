# ADR-016: Owned visualization contracts; Grafana interoperability optional

**Original decision:** 2026-04-16  
**Revised:** 2026-11-07  
**Status:** Accepted — supersedes the mandatory Grafana library proposal

## Context

The previous document required `@grafana/ui` and `@grafana/scenes`, although neither is a current frontend dependency. Requiring those libraries would couple core Observable UI behavior and design tokens to an external visualization framework. The Experience Specification defines user-visible semantics independently of rendering implementation.

## Decision

- Observable owns versioned visualization and interaction contracts: series, units, scope/time window, sampling, aggregation, provenance, freshness, annotations, selection, drill-down and failure states.
- React components render the contracts through adapters, using focused visualization libraries (currently including D3) where appropriate. No particular chart library is required by the metamodel.
- Charts, tables, service/dependency maps and responsive layouts must preserve authorization and explicit unknown/stale/partial states. Visual regression alone is not functional proof.
- Keep design-system tokens, accessible primitives and chart interaction behavior in Observable-owned modules. Avoid coupling domain modules to Grafana's `DataFrame`, `PanelData` or Scenes state tree.
- Grafana may be supported as an **optional, separately versioned integration**, e.g. through datasource APIs/plugins or explicitly adopted chart adapters. Do not require Grafana services, iframes or internal npm libraries for the core UI.
- Any optional integration requires independent licensing, security and compatibility review. Do not assume every package or plugin shares the same license as Grafana's core application.
- For new UI modules follow [frontend module isolation](../21-frontend-modules.md), including standalone render and fake capability providers.

## Verification

Verify the same UX interactions and acceptance criteria against substitute chart renderers and real/programmable data providers. Include keyboard navigation, WCAG 2.2 AA, high-cardinality visual density, explicit units, point-to-evidence drill-down and unknown/stale/restricted states. Assess any Grafana integration against bundle size, upgrade impact and versioned adapters before adoption.

## Consequences

Observable can change rendering libraries without rewriting business-facing UX contracts. Sophisticated visualization types still need implementation and tests; package reuse remains an option but not a mandatory dependency.

## Related

- [Frontend specification](../05-frontend.md)
- [Experience definitions](../business/experience/ux-workflows.yaml)
- [ADR-006](ADR-006-react-vite-frontend.md)
