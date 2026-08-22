import { test, expect } from "@playwright/test";

// Proves the Metrics vertical slice renders from the real playground engine
// — Rust-planned (query-core), wasm-bindgen-bridged, DuckDB-WASM catalog
// aggregation and group-points join over the Rust-generated
// `metric_series`/`metric_points` tables — with no HTTP calls to
// /v1/metrics, /v1/nlq, or /v1/tenants. See
// docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md
// Phase 5.
test("metrics page renders from the real DuckDB-WASM engine with no backend calls", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/metrics");

  // DuckDB-WASM initialization (first query) is slower than a static fixture.
  await expect(page.getByRole("table", { name: "Service metrics" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("http.server.duration").first()).toBeVisible();
  // The catalog auto-selects the highest series_count metric and renders its graph.
  await expect(page.locator('[aria-label^="Graph for"]')).toBeVisible();

  const blockedPrefixes = ["/v1/metrics", "/v1/nlq", "/v1/tenants"];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});
