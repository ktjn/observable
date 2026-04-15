# ADR-006: React/Vite Frontend

**Date:** 2026-04-15  
**Status:** Accepted  
**Authors:** Gemini CLI  
**Deciders:** Project Stakeholders  
**Review date:** 2026-04-15  

## Context

The platform's frontend must support complex, data-heavy UIs for exploring traces, logs, and metrics. It needs a modern, performant, and well-supported framework that allows for efficient component-based development and fast iteration.

## Decision

The **frontend will be built using React (v19+), TypeScript, and Vite (v8+)**. We will use TanStack Query for server-state management.

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

**Constrained:** 
- The project is committed to the React ecosystem, making it more difficult to switch to other frameworks (e.g., Vue, Svelte) later.

## Alternatives Considered

### Option A: Next.js
Rejected to maintain a clean split between the frontend and backend, avoiding unnecessary server-side rendering complexity for a pure SPA dashboard.

### Option B: Angular
Rejected because React offers a more flexible and modern ecosystem, aligning with the project's performance and developer experience goals.

## Related

- `spec/05-frontend.md` (Frontend Architecture)
- `spec/13-risks-roadmap.md` (Final Recommendation)
