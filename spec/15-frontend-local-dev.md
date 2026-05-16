# Frontend Local Development and Hosting

This document supplements `spec/05-frontend.md` (architecture) and `spec/12-deployment.md §19.6`
(quick-start commands). It specifies the directory layout, environment configuration, dev server
setup, local workflow commands, mock strategy, production build output, and hosting model.

---

## 22. Frontend Local Development

### 22.1 Directory Layout

The frontend lives inside the monorepo under `apps/frontend/`. Shared packages that are used by
both the frontend and other tooling live under `packages/`.

```
Observable/                         ← repo root
├── apps/
│   └── frontend/                   ← React SPA
│       ├── src/
│       │   ├── main.tsx            ← React entry point
│       │   ├── App.tsx             ← Root component
│       │   ├── router.ts           ← TanStack Router instance
│       │   ├── components/         ← shared components
│       │   │   ├── ui/             ← "owned" primitives (Base UI + Tailwind v4)
│       │   │   └── shared/         ← domain-agnostic UI components
│       │   ├── features/           ← domain-specific feature modules
│       │   │   ├── tracing/
│       │   │   ├── logs/
│       │   │   └── metrics/
│       │   ├── hooks/              ← shared custom hooks
│       │   ├── lib/                ← third-party SDK config (OTel, etc.)
│       │   ├── routes/             ← TanStack Router route definitions
│       │   ├── stores/             ← global state (Zustand)
│       │   ├── styles/             ← Tailwind v4 globals
│       │   └── mocks/              ← MSW handlers for offline development
│       ├── e2e/                    ← Playwright tests
│       ├── index.html              ← Vite HTML entry
│       ├── vite.config.ts
│       ├── tsconfig.json
│       └── package.json
```

### 22.2 Prerequisites

| Tool | Minimum version | Notes |
|------|----------------|-------|
| Node.js | 22 LTS | Use `nvm` or `fnm` to pin via `.nvmrc` at repo root |
| npm | 10 | Bundled with Node 22; do not substitute yarn or pnpm without an ADR |
| Docker | 24 | Required for the local Compose stack |
| Docker Compose | v2 plugin | `docker compose` (v2); not standalone `docker-compose` v1 |

A `.nvmrc` file at the repo root pins the Node version:

```
22
```

### 22.3 Environment Variables

Copy `.env.local.example` to `.env.local` at the repo root before running any service.

**Frontend-relevant variables in `.env.local`:**

```dotenv
# URL the Vite proxy forwards /v1/* requests to.
# Points to the platform control-plane HTTP API.
VITE_API_BASE_URL=http://localhost:8090

# Tenant ID used by default in local development.
VITE_DEFAULT_TENANT_ID=local-dev

# Feature flags (comma-separated list of enabled flags).
# Leave empty to use defaults defined in feature-flags.ts.
VITE_FEATURE_FLAGS=
```

**Rules:**

- All variables exposed to the browser **must** be prefixed `VITE_`. Variables without this prefix
  are never included in the bundle.
- Never place secrets (API keys, signing keys) in `VITE_*` variables; they are visible in the
  compiled JavaScript.
- Production values are injected at build time by CI; `.env.local` is never used outside
  local development.

### 22.4 Vite Dev Server Configuration

The dev server runs at `http://localhost:5173` by default.

- **Styling**: **Tailwind CSS v4** (Rust-based engine). Utility-first approach for high-density layouts.
- **Primitives**: **Base UI** (using the Shadcn/ownership pattern). Code for primitives is copied into `src/components/ui`.
- **API Proxy**: Vite's proxy rewrites `/v1/*` requests to the backend control-plane service, avoiding CORS issues and matching the production routing model.

`apps/frontend/vite.config.ts` (authoritative reference):

```typescript
import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  return {
    plugins: [react()],
    server: {
      port: 5173,
      proxy: {
        '/v1': {
          target: env.VITE_API_BASE_URL ?? 'http://localhost:8090',
          changeOrigin: true,
        },
      },
    },
    build: {
      sourcemap: true,
      rollupOptions: {
        output: {
          // Deterministic chunk names for cache-busting in production.
          entryFileNames: 'assets/[name]-[hash].js',
          chunkFileNames: 'assets/[name]-[hash].js',
          assetFileNames: 'assets/[name]-[hash][extname]',
        },
      },
    },
  }
})
```

**Port map for local development (no conflicts):**

| Service | Port(s) |
|---------|---------|
| ClickHouse HTTP | 8123 |
| ClickHouse native | 9000 |
| Redpanda Kafka | 9092 |
| Redpanda Admin | 9644 |
| Postgres | 5432 |
| OpenFGA | 8080 |
| Control-plane API | 8090 |
| Vite dev server | 5173 |

### 22.5 Quick Start (Step by Step)

```bash
# Start the Compose stack, including the nginx-hosted frontend.
make dev
```

The containerized app is now available at `http://localhost:5173`.

For frontend development with HMR, run the Vite dev server instead:

```bash
# 1. Install Node dependencies.
npm ci

# 2. Start Vite.
npm run dev --workspace=apps/frontend
```

Hot module replacement (HMR) is active in Vite mode; saving any source file updates the browser
without a full page reload.

To run the frontend **without a running backend** (UI development only), start Vite with the
mock service worker enabled:

```bash
npm run dev:mock
```

See §22.6 for the MSW mock strategy.

### 22.6 Mock / Stub Strategy (Backend-less Development)

When the backend is unavailable or not yet implemented for a feature under development, use
[Mock Service Worker (MSW)](https://mswjs.io/) to intercept `fetch` calls at the network layer.

**Structure:**

```
src/mocks/
├── browser.ts          ← MSW browser setup (service worker registration)
├── handlers/
│   ├── traces.ts       ← handlers for /api/v1/traces/*
│   ├── logs.ts
│   ├── metrics.ts
│   └── services.ts
└── fixtures/           ← typed JSON fixtures for response bodies
    ├── trace-list.json
    └── service-catalog.json
```

**Rules:**

- MSW handlers **must** mirror the exact OpenAPI contract paths and response shapes from
  `spec/09-api.md`. Diverging mock shapes invalidate the point of contract-aligned testing.
- Fixtures are shared between MSW handlers and Vitest unit tests. Do not duplicate fixture data.
- MSW is **never** active in the production build. Guard activation behind
  `import.meta.env.DEV && import.meta.env.VITE_USE_MOCKS === 'true'`.
- `npm run dev:mock` sets `VITE_USE_MOCKS=true` via the `--mode mock` flag and a
  `vite.config.mock.ts` env override.

### 22.7 Local Workflow Commands

Run from `apps/frontend/`:

| Command | Purpose | CI equivalent |
|---------|---------|--------------|
| `npm run dev` | Start dev server with HMR | — |
| `npm run dev:mock` | Start dev server with MSW mocks active | — |
| `npm run typecheck` | Run `tsc --noEmit` | ✅ PR check |
| `npm run lint` | Run ESLint | ✅ PR check |
| `npm run test` | Run Vitest unit/component tests | ✅ PR check |
| `npm run test:watch` | Vitest in watch mode | — |
| `npm run build` | Production build to `dist/` | ✅ PR check |
| `npm run build:analyze` | Production build + bundle size report | on demand |
| `npm run e2e` | Run Playwright E2E suite (requires running app) | nightly |
| `npm run e2e:ui` | Playwright UI mode for debugging | — |

**Run all required PR checks locally before pushing:**

```bash
npm run typecheck && npm run lint && npm run test && npm run build
```

---

## 23. Frontend Production Build

### 23.1 Build Output

`npm run build` (i.e., `vite build`) writes to `apps/frontend/dist/`:

```
dist/
├── index.html                  ← SPA shell; all routes serve this file
└── assets/
    ├── main-[hash].js          ← application entry chunk
    ├── vendor-[hash].js        ← node_modules split (React, TanStack, etc.)
    ├── grafana-[hash].js       ← @grafana/ui + @grafana/scenes split
    └── *.css, *.woff2, *.svg   ← hashed static assets
```

**Chunk split strategy:**

Separate `vendor` and `grafana` chunks are split by manual Rollup `manualChunks` config.
This maximises cache reuse: a product feature change does not invalidate the large Grafana chunk.

### 23.2 Bundle Size Targets

ADR-016 requires monitoring bundle size due to `@grafana/ui` and `@grafana/scenes` being
large packages. CI enforces the following gzip limits via `vite-bundle-visualizer` or
`bundlesize` in the PR check pipeline:

| Chunk | Gzip limit |
|-------|-----------|
| `main-*.js` | 150 KB |
| `vendor-*.js` | 200 KB |
| `grafana-*.js` | 400 KB |
| Total initial JS | 800 KB |

Exceeding a limit fails the `frontend checks` PR gate. Adjust limits via
`apps/frontend/bundlesize.config.json` with a PR explaining the increase.

### 23.3 Environment Variables at Build Time

CI injects production environment variables at build time using GitHub Actions secrets
(or the equivalent CI secret store). The frontend build does **not** embed secrets; it only
embeds public configuration:

```dotenv
VITE_API_BASE_URL=https://api.observable.example.com
VITE_DEFAULT_TENANT_ID=              # empty; derived from auth token at runtime
VITE_FEATURE_FLAGS=                  # controlled by server-side feature flag endpoint
```

---

## 24. Frontend Hosting in Production

### 24.1 Hosting Model

The frontend is deployed as a **container image** that runs an nginx server serving the
`dist/` directory. This approach integrates cleanly with the Kubernetes-first deployment model
(see `spec/12-deployment.md §19.1`) without requiring a separate CDN or object storage bucket
as a prerequisite for Phase 1.

**Why nginx-in-container over CDN/object storage:**

- Single Kubernetes deployment mechanism for all artifacts in Phase 1.
- No CDN or bucket provisioning required before Phase 1 exits.
- Consistent promotion model: the same signed container image promotes from integration →
  staging → production.
- CDN can be layered in front of the ingress in a later phase without changing the
  container or deployment manifests.

### 24.2 Frontend Dockerfile

`apps/frontend/Dockerfile`:

```dockerfile
# syntax=docker/dockerfile:1.7

FROM node:22-alpine AS deps
WORKDIR /app

COPY package.json package-lock.json ./
COPY apps/frontend/package.json apps/frontend/package.json
RUN --mount=type=cache,id=observable-npm,target=/root/.npm,sharing=locked \
    npm ci --workspace=apps/frontend --ignore-scripts

FROM deps AS build
COPY apps/frontend apps/frontend
WORKDIR /app/apps/frontend
RUN npm run build

FROM nginx:1.27-alpine AS runtime

RUN rm -rf /usr/share/nginx/html/*
COPY --from=build /app/apps/frontend/dist /usr/share/nginx/html
COPY apps/frontend/nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
```

`apps/frontend/nginx.conf`:

```nginx
server {
    listen 80;
    root /usr/share/nginx/html;
    index index.html;
    resolver 127.0.0.11 valid=10s ipv6=off;

    # Serve static assets with long cache TTL (filenames include content hash).
    location /assets/ {
        expires 1y;
        add_header Cache-Control "public, immutable";
    }

    # Proxy /v1/* to the control-plane backend service.
    # In production, the Kubernetes ingress handles this routing;
    # this nginx rule is only active when running the container standalone.
    location /v1/ {
        set $query_api http://query-api:8090;
        proxy_pass $query_api;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    location = /index.html {
        add_header Cache-Control "no-cache";
    }

    # SPA fallback: serve index.html for all unmatched paths.
    location / {
        try_files $uri $uri/ /index.html;
        add_header Cache-Control "no-cache";
    }
}
```

**Rules:**

- The nginx `location /v1/` block is a local-container convenience only. In Kubernetes,
  path-based routing is handled by the ingress controller, not by nginx inside the frontend
  container.
- `index.html` must **not** be cached. The nginx default `Cache-Control: no-cache` for
  non-asset routes achieves this; do not add `expires` directives to the `/` or `/api/`
  location blocks.
- The container must expose port 80 only. TLS termination is handled by the ingress layer.

### 24.3 Kubernetes Deployment

The frontend container is deployed as a `Deployment` in the control-plane namespace alongside
the backend services. It is exposed via the shared platform ingress, which routes:

```
/         → frontend container (nginx, port 80)
/oauth/*  → Zitadel auth service
/oidc/*   → Zitadel auth service
/.well-known/* → Zitadel auth service
/ui/*     → Zitadel login UI
/api/*    → control-plane service (port 8090)
/grpc/*   → ingest service (port 4317)
```

This path-based split means the SPA and the API share a single origin in production,
eliminating CORS and matching the local Vite proxy setup exactly.

---

## 25. E2E Testing with Playwright

### 25.1 Setup

Playwright is installed as a dev dependency in `apps/frontend/`:

```bash
npm install --save-dev @playwright/test
npx playwright install --with-deps chromium
```

Tests live in `apps/frontend/e2e/`. The `playwright.config.ts` at the same level points at
`http://localhost:5173` for local runs and `http://localhost:4173` for preview mode.

### 25.2 Running E2E Tests

```bash
# Start the app (with mocks) then run E2E.
npm run build && npm run preview &
npm run e2e

# Or run against the live dev server.
npm run dev &
npm run e2e
```

In CI, E2E runs nightly against the ephemeral environment (not on every PR). See
`spec/10-process.md §16.6` pipeline triggers.

### 25.3 Test Scope

| Suite | Coverage target |
|-------|----------------|
| Smoke | App loads, service catalog renders, no console errors |
| Trace explorer | Search, waterfall render, span detail panel |
| Log explorer | Search, timeline, log detail with trace link |
| Metric explorer | Series query, cardinality inspector |
| Navigation | Deep links survive reload; breadcrumb trail works |
| Time range | URL reflects time; compare mode overlay renders |

---

## 27. Dependency Maintenance & Security

Managing npm dependencies is a critical security and stability concern for a production-grade UI.

### 27.1 Vulnerability Scanning
- **`npm audit`**: Run automatically in CI on every PR. Any "High" or "Critical" vulnerability blocks the PR.
- **Snyk/GitHub Advanced Security**: Used for continuous monitoring of vulnerabilities in the dependency tree.

### 27.2 Automated Updates
- **Renovate/Dependabot**: Configured to open PRs for dependency updates.
- **Grouping**: Patch and minor updates are grouped to reduce PR noise. Major updates are opened individually.
- **Human Review Mandate**: Every dependency update PR requires manual review and approval by a human maintainer. Auto-merge is disabled to ensure project security and stability, complying with the core mandates in `GEMINI.md`.

### 27.3 License Compliance
- **Allowed Licenses**: MIT, Apache-2.0, BSD-3-Clause, ISC.
- **Restricted Licenses**: AGPL, GPL, LGPL (requires manual review).
- **Tooling**: `license-checker` runs in CI to ensure no restricted licenses are introduced via new dependencies.

### 27.4 Package Pinning
- Always use `npm ci` in CI and Docker builds to ensure exact versions from `package-lock.json` are used.
- Commit `package-lock.json` after every dependency change.

---

## 28. Accessibility Verification

Accessibility is not a one-time task but a continuous requirement.

### 28.1 Automated Testing
- **`playwright-axe`**: Integrated into the Playwright E2E suite.
- Every major page (Service Catalog, Explorers, Dashboards) must have an accessibility test case:
  ```typescript
  test('should have no automatically detectable accessibility violations', async ({ page }) => {
    await page.goto('/services');
    const accessibilityScanResults = await new AxeBuilder({ page }).analyze();
    expect(accessibilityScanResults.violations).toEqual([]);
  });
  ```

### 28.2 Manual Review Checklist
- **Keyboard Navigation**: Can all interactive elements be reached and activated using only the keyboard?
- **Focus Indicators**: Are focus rings clearly visible?
- **Color Contrast**: Do all text and icons meet WCAG AA contrast ratios? (Verified via browser devtools).
- **Screen Readers**: Do complex components (charts, Monaco) have appropriate ARIA labels and descriptions?

---

## 29. Spec and ADR Cross-References

| Topic | Reference |
|-------|-----------|
| Stack selection (React, Vite, TanStack) | `spec/05-frontend.md §9.1`, `spec/adr/ADR-006-react-vite-frontend.md` |
| Visualization library (Grafana npm packages) | `spec/05-frontend.md §9.1`, `spec/adr/ADR-016-grafana-visualization-strategy.md` |
| State management boundaries | `spec/05-frontend.md §9.13` |
| Query API contracts | `spec/09-api.md` |
| Local dep stack (Docker Compose) | `spec/12-deployment.md §19.6` |
| Kubernetes deployment model | `spec/12-deployment.md §19.1–19.3` |
| CI frontend checks | `spec/10-process.md §16.6` |
| Phase 1 UI scope | `spec/10-process.md §17`, `spec/05-frontend.md §9.3` |

---

## 30. Frontend Styling Migration Rule

All frontend slices after UI-R3 must follow this rule:

1. **Primitives first:** Reuse `src/components/ui/` primitives for interactive elements, loading states, table wrappers, status indicators, metric cards, panels, and buttons. Do not duplicate their styling in product pages.
2. **Add missing primitives in the same slice:** If a needed primitive does not exist, add it to `src/components/ui/` with a focused RTL test in the same PR. Do not defer primitive additions to a follow-up.
3. **Token classes over hardcoded values:** Use Tailwind token classes (`text-[var(--muted)]`, `bg-[var(--surface)]`, `border-[var(--border)]`, etc.) instead of hardcoded hex values or non-standard CSS custom properties.
4. **Avoid page-local interactive styling:** Do not define new interactive CSS rules (hover, focus, active states) in `styles.css` or in `<style>` blocks. Express them via Tailwind utilities.
5. **Approved rendering exceptions** — inline `style` props are permitted only for:
   - **Span bar position/width in `TraceDetail.tsx`:** percentage-based `left`/`width` for canvas-accurate waterfall rendering.
   - **Topology SVG/canvas in `ServiceOverview.tsx`:** dynamic `x`/`y`/`fill`/`stroke` values for force-directed graph node and edge positioning.
   - **Severity colors in `LogContextView.tsx`, `LogCorrelatedList.tsx`, `LogLiveTail.tsx`:** runtime-computed numeric severity to color mapping.
   - **`minmax()` grid columns:** `style={{ gridTemplateColumns: "repeat(4, minmax(140px, 1fr))" }}` where Tailwind has no direct equivalent.
   - Any new exception must be noted with a comment in the file and approved by the reviewer.
