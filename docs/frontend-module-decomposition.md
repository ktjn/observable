# Frontend Module Decomposition

> **Status:** Target architecture for roadmap 0.2.
>
> This document extends [component-decomposition.md](component-decomposition.md) and
> [ADR-036](../spec/adr/ADR-036-frontend-module-boundaries.md). The end state is not merely a
> feature-folder layout: each UI module must be independently buildable, runnable, testable, and
> integrated through a small explicit contract.

## Goal

Turn `observable-web` from one large SPA source tree into a thin application shell plus small UI
modules that can be developed and verified in isolation.

A module is considered independent when it can:

1. compile without importing another feature module's implementation;
2. run in a standalone development harness;
3. run unit/component/contract tests without booting the full Observable UI;
4. declare the backend/runtime capabilities it requires;
5. expose routes, navigation, commands, and optional cross-module contributions through a stable
   module contract;
6. be integrated into the shell without the shell importing its internal pages/components;
7. be moved to a separate package/repository later without redesigning its public boundary.

Independent deployment or runtime module federation is **not** required. Static composition of
independently released packages is the initial target.

## Current State Review

The frontend has already moved partway toward modularity.

### Good foundations

- `apps/frontend` is a single React 19/TypeScript/Vite application with TanStack Router and Query.
- Domain folders already exist under `src/features/` for alerts, incidents, dashboards, services,
  metrics, NLQ, workbench, onboarding, administration, and change events.
- `RuntimeApi` provides a transport seam between production HTTP and the browser-local playground.
- `runtime.contract.test.ts` exercises both runtime implementations against a common behavior
  contract.
- The playground proves that the same UI can run against a non-HTTP backend.
- Some pages under `src/pages/` are already thin compatibility wrappers around feature modules.

These are valuable decomposition assets and should be preserved rather than replaced.

### Remaining coupling

#### 1. One package and one build graph

`apps/frontend/package.json` defines one private package, one TypeScript build, one Vite build, and
one Vitest suite. Any source change participates in the same package build/test boundary.

#### 2. Central router owns every page

`src/router.ts` imports and registers nearly every page directly. It therefore knows the internal
entry component for every feature and becomes a compile-time dependency hub.

#### 3. AppShell owns application composition

`AppShell.tsx` hard-codes the complete navigation tree and combines:

- application chrome;
- auth redirect behavior;
- tenant/environment selection;
- global time context;
- theme/time-format preferences;
- command palette;
- playground behavior;
- navigation entries for every feature.

Adding/removing a module currently requires editing the shell.

#### 4. RuntimeApi is a useful seam but is too broad

`RuntimeApi` contains tenants, traces, logs, services, topology, metrics, change events, alerts,
incidents, SLOs, notification channels, saved views, infrastructure, setup, tokens, members, usage,
auth, deployments, NLQ, and dashboards.

A trace module that needs three operations can therefore see the entire application backend
contract. This weakens ownership and makes runtime implementations grow in lockstep.

#### 5. Transport types leak across the UI

Feature-independent components, hooks, runtime repositories, and utilities still import types from
`src/api/*`. The API folder therefore acts as both HTTP adapter and de facto domain-model package.
This makes it difficult to move a module without dragging unrelated transport code with it.

#### 6. Feature structure is incomplete

The codebase currently uses all of these as feature ownership locations:

- `src/features/*`;
- `src/pages/*`;
- `src/components/*`;
- `src/hooks/*`;
- `src/utils/*`;
- `src/api/*`;
- `src/runtime/*`.

The existing feature directories are a useful start, but a module cannot yet be identified by one
closed source subtree plus declared dependencies.

#### 7. Large shared files are integration hotspots

Examples include the global router, AppShell, global stylesheet, large application-level test file,
and the playground runtime implementation. These should become composition or adapter code rather
than places where feature behavior accumulates.

## Target Structure

During migration, use an npm workspace inside `observable-web` before extracting repositories.

```text
observable-web/
  apps/
    shell/                    # production SPA composition root
    playground/               # optional distribution/runtime composition

  packages/
    ui-core/                  # design tokens, primitives, generic layout widgets
    app-contract/             # module manifest + host context types only
    runtime-contracts/        # generated/public backend capability types
    runtime-http/             # OpenAPI-backed production adapters
    runtime-playground/       # browser-local adapters
    test-harness/             # reusable module host + fake capabilities

  modules/
    home/
    services/
    traces/
    logs/
    metrics/
    infrastructure/
    dashboards/
    reliability/              # alerts + SLOs + incidents
    workbench/
    control/                  # setup/admin/fleet/config
    identity/                 # login/session/member/token UX
    onboarding/
```

This is a logical target. Do not perform a big-bang directory move.

## Module Contract

Every module exports one public manifest and nothing else is imported by the shell.

Conceptually:

```ts
export interface ObservableUiModule {
  id: string;
  routes: readonly RouteContribution[];
  navigation?: readonly NavigationContribution[];
  commands?: readonly CommandContribution[];
  requiredCapabilities: readonly CapabilityId[];
  createRuntime(runtime: RuntimeRegistry): ModuleRuntime;
}
```

The exact TypeScript shape may evolve, but the semantics are fixed:

- **routes** — path, lazy entry component, route-owned search/params validation;
- **navigation** — optional shell navigation contribution;
- **commands** — optional command-palette contribution;
- **requiredCapabilities** — explicit backend/runtime dependencies;
- **runtime** — capability lookup scoped to the module, not access to a global god interface.

The shell composes manifests:

```text
module manifests -> route tree
                 -> navigation tree
                 -> command registry
                 -> capability validation
```

A module must not edit shell internals to add a route or navigation item.

## Runtime Capability Contracts

Replace the single broad `RuntimeApi` with small capability interfaces.

Examples:

```text
capability.auth.v1
capability.tenant-context.v1
capability.traces.v1
capability.logs.v1
capability.metrics.v1
capability.services.v1
capability.infrastructure.v1
capability.dashboards.v1
capability.alerting.v1
capability.control.v1
capability.nlq.v1
```

A module receives only the capabilities it declares.

Example:

```text
traces module
  requires traces.v1
  optional logs.v1        # correlated logs
  optional deployments.v1 # deployment context
```

Rules:

- capability interfaces contain data/operation contracts, not React hooks;
- HTTP and playground are adapters implementing the same capability contracts;
- module code never imports from `runtime-http` or `runtime-playground`;
- generated OpenAPI DTO/client types live in `runtime-contracts`/`observable-contracts`, not inside
  feature code;
- capability versions change explicitly when the UI-facing semantic contract breaks.

## UI-Core Boundary

`ui-core` may contain only domain-agnostic assets:

- design tokens;
- Base UI-based primitives;
- generic table/list/layout components;
- loading/error/empty states;
- accessible overlays/dialogs/popovers;
- generic chart framing and formatting primitives where no domain semantics leak in.

It must not contain concepts such as `Trace`, `Incident`, `Service`, `Deployment`, or
`NotificationChannel`.

Domain-aware shared code belongs either to its owning module or to an explicit narrow domain
contract package if two independent modules genuinely require it.

## Cross-Module Interaction

Do not solve cross-module navigation with direct imports.

Preferred mechanisms:

1. URL/deep-link contract for navigation;
2. typed host context for tenant/environment/time range;
3. capability calls for backend operations;
4. small typed application events only when URL/capability calls do not fit.

Examples:

```text
trace -> log explorer     URL with trace/span filters
alert -> service          URL with service/time context
service -> traces         URL with service/time context
module -> global time     HostContext, read-only + explicit setter contract
```

Avoid a general-purpose frontend event bus. It would recreate implicit coupling.

## Standalone Runnable Harness

Every module gets a standard harness executable with Vite:

```text
npm run dev --workspace @observable/module-traces
```

The harness provides:

- minimal shell chrome;
- in-memory router;
- QueryClient;
- theme/design tokens;
- deterministic tenant/environment/time context;
- fake or playground-backed capability adapters;
- fixture/scenario selector;
- accessibility instrumentation in development.

The harness is part of the module's supported development contract, not a throwaway demo.

A module must be demonstrable without the production HTTP backend.

## Test Contract Per Module

Each module owns four test layers.

### 1. Unit

Pure state, transformations, parsing, formatting, query construction.

### 2. Component/integration

Render the module against fake capability adapters and verify user-visible behavior with
Vitest + Testing Library.

### 3. Module contract

Verify:

- manifest is valid;
- declared routes are unique and mountable;
- only declared capabilities are requested;
- public exports match the module contract;
- no forbidden imports cross module boundaries.

### 4. Standalone browser smoke

Playwright starts only that module's harness and exercises its primary flow.

Full-product E2E remains in the shell/distribution layer and tests module composition rather than
re-testing every internal behavior.

## Build and Dependency Rules

Each module must have its own `package.json`, `tsconfig`, build, and test command.

Allowed dependency direction:

```text
module -> app-contract
module -> runtime-contracts
module -> ui-core
module -> third-party libraries explicitly declared by that module

shell -> module public manifests
shell -> app-contract
shell -> runtime adapters
```

Forbidden:

```text
module A -> module B implementation
module -> shell internals
module -> runtime-http
module -> runtime-playground
module -> another module's private types/components/hooks
ui-core -> domain module
```

Enforce this mechanically with package `exports`, TypeScript project references/workspaces, and an
import-boundary lint/check script. Folder conventions alone are insufficient.

## Initial Module Boundaries

Do not create a package per page. Prefer cohesive operator capabilities.

| Module | Initial routes/scope | Primary backend capabilities |
| --- | --- | --- |
| `services` | service catalog, service detail/topology entry points | services, topology, query signals |
| `traces` | trace search/detail/compare | traces, optional logs/deployments |
| `logs` | log search/live/context | logs, optional traces |
| `metrics` | metric explorer | metrics |
| `infrastructure` | inventory/detail | infrastructure, services |
| `dashboards` | list/detail/editor | dashboards, query signals |
| `reliability` | alerts, SLOs, incidents, notification channels | alerting |
| `workbench` | NLQ/query workbench | NLQ/query |
| `control` | setup, platform config, fleet | control |
| `identity` | login/session, members, tokens | auth |
| `onboarding` | first-run workflow | auth, control, ingest/setup status |
| `home` | overview/entry page | small composition-specific read capabilities |

Service-detail tabs may render links or module-owned route outlets, but `services` must not import
trace/log/metric feature implementations.

## Migration Plan

### UI Phase 0 — Inventory and dependency graph

- map every page/component/hook/API/runtime file to an owning target module or shared package;
- generate an import graph and identify cross-feature cycles;
- classify every item currently in `components`, `hooks`, `utils`, and `api` as domain-owned or
  genuinely shared;
- record route ownership and runtime-capability usage.

**Exit evidence:** every frontend source file has one target owner; every cross-module dependency is
listed and either accepted as a public contract or scheduled for removal.

### UI Phase 1 — Establish workspace and contracts

- convert frontend to npm workspaces;
- create `app-contract`, `ui-core`, `runtime-contracts`, and `test-harness` packages;
- add package exports and import-boundary enforcement;
- keep the existing SPA behavior unchanged.

**Exit evidence:** an empty/sample module can build, test, run standalone, and mount in the shell
without private imports.

### UI Phase 2 — Split RuntimeApi into capabilities

- derive small interfaces from the existing `RuntimeApi`;
- make HTTP/playground adapters implement capability interfaces;
- keep a temporary compatibility facade for existing pages;
- migrate one module at a time off the facade.

**Exit evidence:** new modules cannot request the full application runtime; capability usage is
explicit and mechanically testable.

### UI Phase 3 — Extract vertical modules

Recommended order:

1. traces;
2. logs;
3. metrics;
4. infrastructure;
5. dashboards;
6. reliability;
7. workbench;
8. services;
9. identity;
10. control;
11. onboarding;
12. home.

Start with explorers because they already have clearer data contracts and relatively isolated
routes. Leave `services` until traces/logs/metrics have URL contracts so service detail can compose
rather than import them.

For each module:

1. move domain code behind one package boundary;
2. define required capabilities;
3. export a module manifest;
4. add standalone harness;
5. move feature tests into the module;
6. add module Playwright smoke;
7. switch shell route/nav registration to the manifest;
8. delete compatibility wrapper/imports only after full shell E2E passes.

### UI Phase 4 — Make the shell thin

Move feature knowledge out of `router.ts` and `AppShell.tsx`.

The shell should own only:

- composition/bootstrap;
- global chrome;
- auth/session bootstrap;
- tenant/environment/time host context;
- route/nav/command aggregation;
- runtime-capability registry;
- global error boundaries and observability.

**Exit evidence:** adding/removing a module requires changing only the composition manifest/version
set, not shell implementation code.

### UI Phase 5 — Independent package releases

- version modules independently only when there is operational value;
- publish immutable package artifacts;
- pin module versions in `observable-web` composition metadata;
- run shell compatibility tests against released module packages.

Repository extraction is optional. A module can satisfy the independence goal inside one repo as
long as build/test/runtime boundaries are real.

### UI Phase 6 — Optional repository extraction

Extract a UI module only if ownership/release cadence justifies it. Preserve the same module and
capability contracts so repository topology does not alter architecture.

## Definition of Done for a UI Module

A UI module is independent when all are true:

1. clean package build succeeds without sibling-module source;
2. unit/component tests run with only declared dependencies;
3. standalone Vite harness runs without the full shell/backend;
4. Playwright smoke passes against the harness;
5. shell imports only its public manifest;
6. module imports no other module implementation;
7. backend access uses declared capability contracts only;
8. route/nav/command contributions are explicit and tested;
9. module can be removed from shell composition without breaking unrelated module builds/tests;
10. its public contract has an explicit compatibility/versioning policy.

## Non-Goals

- micro-frontends or Module Federation as an architectural requirement;
- separate deployment for every UI module;
- iframe composition;
- a package per React component or page;
- duplicating design-system primitives in modules;
- introducing a global event bus to replace direct imports;
- forcing all modules into separate repositories before the boundaries are proven.
