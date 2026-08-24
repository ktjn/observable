import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig(({ mode }) => ({
  // Playground builds (`vite build --mode playground`) are served from
  // GitHub Pages under a repository sub-path; production keeps the default "/".
  base: mode === "playground" ? "/observable/" : "/",
  plugins: [react(), tailwindcss()],
  optimizeDeps: {
    // SQLite Wasm supplies its own ESM/Wasm loader and must not be pre-bundled
    // by Vite's dependency scanner.
    exclude: ["@sqlite.org/sqlite-wasm"],
  },
  server: {
    port: 5173,
    // The playground build must never depend on a live backend (it has none on
    // GitHub Pages). Proxying is dev-only tooling for the production runtime.
    proxy: mode === "playground" ? undefined : {
      "/v1/auth": { target: "http://localhost:4319", changeOrigin: true },
      // admin-service: members, tokens, platform config, tenant usage report.
      // Mirrors the ^~ prefix blocks in apps/frontend/nginx.conf (production) —
      // these must stay scoped exactly to admin-service's routes and precede the
      // generic "/v1" fallback below, or they get silently swallowed by it.
      "/v1/admin": { target: "http://localhost:4324", changeOrigin: true },
      "/v1/tokens": { target: "http://localhost:4324", changeOrigin: true },
      "/v1/config": { target: "http://localhost:4324", changeOrigin: true },
      "/v1/tenants/usage-report": { target: "http://localhost:4324", changeOrigin: true },
      "/v1": { target: "http://localhost:8090", changeOrigin: true },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
    exclude: ["**/node_modules/**", "**/dist/**", "e2e/**"],
  },
}));
