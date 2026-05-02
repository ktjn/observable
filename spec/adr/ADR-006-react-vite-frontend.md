# ADR-006: React/Vite Frontend

**Date:** 2026-04-15  
**Status:** Accepted  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

The platform's frontend must support complex, data-heavy UIs for exploring traces, logs, and metrics. It needs a modern, performant, and well-supported framework that allows for efficient component-based development and fast iteration.

## Decision

The **frontend will be built using React (v19+), TypeScript, and Vite (v8+)**.

| Layer | Choice | Rationale |
|---|---|---|
| Styling | **Tailwind CSS v4** | Rust-based engine for high-performance data density; zero runtime overhead |
| Primitives | **Base UI (MUI team)** | Modern, actively maintained, render-prop pattern replaces Radix UI |
| Workflow | **Shadcn Pattern** | Local component ownership; no library lock-in |

We will use **TanStack Query** for server-state management and **TanStack Router** for URL-driven navigation.

### Component Reusability and Minimal Duplication
To maintain a scalable and maintainable codebase, we mandate the use of **reusable components** and **minimal logic duplication**. 
- UI primitives are owned and styled locally in `src/components/ui/` (Shadcn pattern).
- Domain-agnostic layout and shared components live in `src/components/shared/`.
- Domain-specific components are kept within their respective `src/features/**/components/` directories but must be made reusable within that domain.
- Common business logic, data fetching patterns, and UI behaviors must be extracted into custom hooks and utility functions to avoid "copy-paste" development.

### Styling Choice: Tailwind CSS v4 over CSS Modules
Initially, CSS Modules were considered for their scoping and performance. However, **Tailwind CSS v4** provides a superior developer experience for building high-density observability UIs. Its new Rust-based compiler handles the thousands of utility classes required for complex dashboards with zero runtime overhead and significantly faster build times.

### Primitive Choice: Base UI over Radix UI
While Radix UI was the initial choice, the industry momentum in 2025–2026 has shifted toward **Base UI**. 
1. **Maintenance:** Base UI is backed by the MUI team and is more actively evolved than Radix.
2. **Architecture:** Base UI's **render prop** pattern is easier to debug and more flexible for complex customizations than Radix's `asChild` pattern.
3. **Engine:** Base UI utilizes **Floating UI** for popovers and tooltips, which is essential for stable positioning in data-heavy layouts.

## Consequences

**Easier:** 
- Modern, component-based architecture for complex UIs.
- Fast development cycle with Vite's HMR.
- Strong TypeScript integration for type safety.
- Efficient state management with TanStack Query.
- Broad ecosystem of libraries and tools.

**Harder:** 
- Managing complex client-side state for data-heavy visualizations.
- Ensuring high performance with large datasets in the browser.
- Maintaining a large dependency tree (npm) with regular security audits and updates.

**Constrained:** 
- The project is committed to the React ecosystem, making it more difficult to switch to other frameworks (e.g., Vue, Svelte) later.
- Production-grade requirements (Accessibility, Observability, Resilience) add implementation overhead to every new feature.

## Alternatives Considered

### Option A: Next.js
Rejected to maintain a clean split between the frontend and backend, avoiding unnecessary server-side rendering complexity for a pure SPA dashboard.

### Option B: Angular
Rejected because React offers a more flexible and modern ecosystem, aligning with the project's performance and developer experience goals.

## Related

- `spec/05-frontend.md` (Frontend Architecture)
- `spec/13-risks-roadmap.md` (Final Recommendation)
