# ADR-036: Frontend Module Boundaries

**Date:** 2026-10-01
**Status:** Accepted
**Authors:** OpenAI, ktjn
**Deciders:** Project Stakeholders
**Review date:** 2026-10-01

## Context

ADR-035 establishes `observable-web` as an independently released top-level component, but the
current frontend remains one private Vite package with one central router, one application shell,
one broad `RuntimeApi`, and feature ownership spread across `pages`, `features`, `components`,
`hooks`, `utils`, `api`, and `runtime`.

The codebase already has useful modularity primitives:

- feature directories;
- a production HTTP runtime and browser-local playground runtime;
- runtime contract tests;
- TanStack Router route isolation;
- reusable design-system components.

However, these are source-organization seams rather than enforceable module boundaries. A feature
cannot yet build, run, or test independently from the whole frontend package, and the central router
and shell know every feature entry point.

The architectural goal is small UI components that are independently buildable, runnable, and
testable with a clearly defined contract, without requiring a micro-frontend platform.

## Decision

`observable-web` will be decomposed into a thin application shell plus independently buildable UI
module packages.

The detailed target and migration plan are defined in
[docs/frontend-module-decomposition.md](../../docs/frontend-module-decomposition.md).

### Static composition first

Modules are composed as normal versioned JavaScript/TypeScript packages at build time.

Module Federation, runtime remote loading, iframes, or separate deployments are not required for
module independence and will not be introduced without a separate ADR and demonstrated need.

### Required module properties

Every UI module must:

- have its own package/build/test boundary;
- expose one public module manifest;
- declare required backend/runtime capabilities;
- run in a standard standalone Vite harness;
- have module-local unit/component tests and a standalone browser smoke test;
- avoid imports from another module's private implementation;
- integrate with the shell through explicit route/navigation/command/context contracts.

### Shell ownership

The application shell owns only cross-cutting composition concerns:

- bootstrap;
- global application chrome;
- auth/session bootstrap;
- tenant/environment/time context;
- route/navigation/command aggregation;
- runtime capability registry;
- global error boundaries and frontend observability.

The shell must not own feature pages or hard-code feature navigation.

### Module manifest

Each module exports a stable manifest describing its integration surface. The exact TypeScript
syntax is implementation detail, but it must cover:

- module identity;
- route contributions;
- optional navigation contributions;
- optional command-palette contributions;
- required runtime capabilities.

The shell imports the manifest, not internal pages/components.

### Runtime capabilities

The current broad `RuntimeApi` is retained temporarily as a migration facade but is not the target
contract.

Backend access will be split into narrow versioned capability interfaces such as traces, logs,
metrics, services, dashboards, alerting, control, auth, and NLQ. A module receives only the
capabilities it declares.

Production HTTP and browser-local playground implementations are adapters for the same capability
contracts.

Transport adapters are not module dependencies. Modules do not import `runtime-http`,
`runtime-playground`, or raw fetch implementations.

### Backend DTO ownership

HTTP transport files must not remain the implicit shared domain-model package.

UI-facing generated/public contract types belong in the released contracts/runtime-contracts
boundary. Modules may define view models internally, but cross-component wire types come from the
contract package.

### Shared UI boundary

A shared `ui-core` package may contain only domain-agnostic design tokens, primitives, layout
components, accessibility helpers, and generic visualization frames.

Domain concepts such as Trace, Service, Incident, Deployment, AlertRule, or NotificationChannel do
not belong in `ui-core`.

### Cross-module communication

Use, in order of preference:

1. URL/deep-link contracts;
2. typed host context;
3. runtime capability calls;
4. narrowly typed application events only where the first three are unsuitable.

A general-purpose frontend event bus is explicitly rejected because it would replace visible source
coupling with hidden runtime coupling.

### Repository topology

UI modules may initially remain in one npm workspace and later be published independently.
Separate repositories are optional and come after build/test/runtime boundaries are proven.

## Consequences

**Easier:**

- a module can be developed and tested without booting the full product;
- feature ownership and dependencies become explicit;
- backend contract usage becomes visible and scoped;
- the shell becomes a composition root rather than a feature dependency hub;
- browser playground/fake adapters become reusable module-test infrastructure;
- individual modules can later be versioned or extracted without redesigning their integration
  surface.

**Harder:**

- more package manifests, build configs, and compatibility checks;
- shared code must be classified instead of casually placed in global folders;
- cross-feature workflows require deliberate URL/capability contracts;
- temporary compatibility adapters are needed while migrating from the existing global runtime and
  router.

**Constrained:**

- new feature modules must not depend on the full `RuntimeApi` once capability contracts exist;
- modules must not import another module's private source;
- adding a module must not require editing shell implementation code after manifest composition is
  complete;
- folder placement alone is not considered a boundary: dependency rules must be mechanically
  enforced.

## Alternatives Considered

### Option A: Keep one SPA package and feature folders only

Rejected. It improves organization but does not provide independent build, run, test, or contract
boundaries.

### Option B: Micro-frontends with Module Federation immediately

Rejected. Runtime distribution solves a deployment/organizational problem that Observable does not
yet have and adds failure modes, dependency negotiation, duplicate runtimes, and operational
complexity. Static package composition achieves the requested testability and independence with a
smaller contract surface.

### Option C: One package per page

Rejected. Page-level packages are too fine-grained and would create excessive coordination and
shared-state contracts. Modules should represent cohesive operator capabilities.

### Option D: One global frontend event bus

Rejected. It makes dependencies harder to discover, type, test, and remove. URL and capability
contracts provide more explicit integration.

## Related

- [ADR-006](ADR-006-react-vite-frontend.md)
- [ADR-031](ADR-031-global-tenant-environment-context.md)
- [ADR-035](ADR-035-component-independence.md)
- [Frontend architecture](../05-frontend.md)
- [Frontend module decomposition](../../docs/frontend-module-decomposition.md)
- [Component decomposition](../../docs/component-decomposition.md)
