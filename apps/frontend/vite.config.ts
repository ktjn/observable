import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
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
});
