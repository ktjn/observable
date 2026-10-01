# Frontend Module Architecture

## 21. Frontend Module Specification

This specification refines the feature-based layout in [05-frontend.md](05-frontend.md) into
mechanically enforceable module boundaries. [ADR-036](adr/ADR-036-frontend-module-boundaries.md)
records the decision; [the decomposition plan](../docs/frontend-module-decomposition.md) defines the
migration sequence.

## 21.1 Target

`observable-web` consists of:

- a thin application shell;
- domain-agnostic shared packages;
- independently buildable UI modules;
- runtime adapters that implement narrow capability contracts.

A feature directory is not considered a module until it can build, run, and test without importing
sibling-module implementation.

## 21.2 Required Module Contract

Every module MUST expose one public manifest that declares:

- stable module ID;
- route contributions;
- optional navigation contributions;
- optional command-palette contributions;
- required runtime capabilities.

The shell MUST consume the manifest rather than importing internal module pages/components.

## 21.3 Runtime Capability Contract

The current global `RuntimeApi` is a migration facade. The target is a registry of narrow versioned
capabilities such as traces, logs, metrics, services, dashboards, alerting, control, auth, and NLQ.

A module MUST receive only capabilities it declares.

Production HTTP and playground/local implementations MUST implement the same capability contract.
A module MUST NOT import transport implementations directly.

## 21.4 Shared Code

Shared `ui-core` code MAY contain:

- design tokens;
- accessible primitives;
- generic layout/table/list components;
- generic loading/error/empty states;
- domain-neutral visualization framing and formatting.

It MUST NOT contain domain concepts such as traces, services, incidents, deployments, alert rules,
or notification channels.

Cross-component wire/DTO types come from generated/released contract packages. HTTP adapter modules
must not become the shared type system.

## 21.5 Cross-Module Communication

Use, in preference order:

1. URLs/deep links;
2. typed host context;
3. runtime capability calls;
4. narrowly typed application events only where required.

A general-purpose frontend event bus is forbidden without a new ADR.

## 21.6 Standalone Runtime

Every module MUST provide a standard Vite development harness that supplies:

- in-memory routing;
- QueryClient;
- design-system/theme context;
- deterministic tenant/environment/time context;
- fake or playground-backed capability adapters.

The module MUST be runnable without the production backend and without the full application shell.

## 21.7 Test Requirements

Every module owns:

- unit tests for pure logic;
- component/integration tests against fake capability adapters;
- module-contract tests for manifest/capability/import rules;
- one standalone Playwright smoke path.

Full-product browser tests remain shell/distribution concerns and verify composition rather than
repeating module internals.

## 21.8 Dependency Direction

Allowed:

```text
module -> app-contract
module -> runtime-contracts
module -> ui-core
module -> explicitly declared third-party libraries
shell  -> module public manifest
shell  -> runtime adapters
```

Forbidden:

```text
module A -> module B private implementation
module -> shell internals
module -> HTTP/playground adapter implementation
ui-core -> domain module
```

These rules MUST be enforced with package exports plus automated import-boundary checks. Directory
conventions alone are insufficient.

## 21.9 Initial Module Set

The initial cohesive modules are:

- services;
- traces;
- logs;
- metrics;
- infrastructure;
- dashboards;
- reliability (alerts/SLOs/incidents/notification channels);
- workbench/NLQ;
- control/setup/fleet;
- identity/members/tokens;
- onboarding;
- home.

Do not create a package per page.

## 21.10 Composition Model

Static package composition is the default. Runtime Module Federation, iframes, or separately
deployed micro-frontends are not required for independence and require a separate ADR if introduced.

## 21.11 Definition of Independent

A UI module is independent only when:

1. a clean package build succeeds without sibling-module source;
2. unit/component/contract tests run with declared dependencies only;
3. the standalone Vite harness runs without the full shell/backend;
4. its standalone browser smoke test passes;
5. the shell imports only the public module manifest;
6. backend access uses declared capability contracts;
7. removing the module from shell composition does not break unrelated module builds/tests;
8. its public contract has an explicit compatibility/versioning policy.
