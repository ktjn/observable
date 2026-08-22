import { test, expect } from "@playwright/test";

// Proves the Services list vertical slice renders from the real playground
// engine — Rust-planned (query-core), wasm-bindgen-bridged, DuckDB-WASM
// per-service aggregation over the Rust-generated `spans` dataset — with no
// HTTP calls to /v1/services, /v1/nlq, or /v1/tenants. Topology view stays
// out of scope for this slice. See
// docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md
// Phase 5.
test("services page renders from the real DuckDB-WASM engine with no backend calls", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/services");

  // DuckDB-WASM initialization (first query) is slower than a static fixture.
  await expect(page.getByRole("table", { name: "Service catalog" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("cell", { name: "web", exact: true }).first()).toBeVisible();

  const blockedPrefixes = ["/v1/services", "/v1/nlq", "/v1/tenants"];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});
