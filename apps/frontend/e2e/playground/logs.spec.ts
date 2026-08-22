import { test, expect } from "@playwright/test";

// Proves the Logs vertical slice renders from the real playground engine —
// Rust-planned (query-core), wasm-bindgen-bridged, DuckDB-WASM-executed
// query against a Rust-generated `logs` dataset correlated with the Traces
// slice's `spans` (see generator.rs::generate_logs) — with no HTTP calls to
// /v1/logs, /v1/nlq, or /v1/tenants. See
// docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md
// Phase 3/4 follow-up.
test("logs page renders from the real DuckDB-WASM engine with no backend calls", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/logs");

  // DuckDB-WASM initialization (first query) is slower than a static fixture.
  await expect(page.getByRole("table", { name: "Log results" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByRole("cell", { name: /request (completed|failed)/ }).first()).toBeVisible();

  const blockedPrefixes = ["/v1/logs", "/v1/nlq", "/v1/tenants"];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});
