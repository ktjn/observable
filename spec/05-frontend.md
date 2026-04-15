# Frontend Architecture

## 9. Frontend Architecture

### 9.1 Stack

- React 19.x
- TypeScript
- Vite 8
- TanStack Query
- routing of choice with typed routes
- design system + charting layer
- Monaco-based query editor (optional)

React is the default choice for complex componentized UIs. Vite 8 is current and production-focused. TanStack Query is a strong fit for server-state heavy applications.

### 9.2 Frontend Modules

- onboarding/setup
- fleet/agent management
- trace explorer
- log explorer
- metrics explorer
- profiling explorer
- service map
- dashboards
- alerts/incidents
- admin/security/billing
- query notebook/workbench

### 9.3 UX Requirements

- sub-2s common interactions on hot data
- saved views
- deep links everywhere
- compare mode
- time travel
- side-by-side correlated panes
- keyboard-driven query UX
- export APIs
- dark mode
- accessibility baseline

### 9.4 Frontend Anti-Patterns

Avoid:
- direct storage coupling in UI components
- ad hoc query syntax divergence per page
- duplicated state machines
- chart library lock-in without an adapter layer
