import { test, expect } from "@playwright/test";

// Proves the Change Events vertical slice renders from the real playground
// engine — Rust-planned (query-core), wasm-bindgen-bridged, DuckDB-WASM
// filtered query over the Rust-generated `change_events` table — with no
// HTTP calls to /v1/events/changes, /v1/nlq, or /v1/tenants. See
// docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md
// Phase 5.
test("change events page renders from the real DuckDB-WASM engine with no backend calls", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/change-events");

  // DuckDB-WASM initialization (first query) is slower than a static fixture.
  await expect(page.getByRole("table", { name: "Change events" })).toBeVisible({ timeout: 15_000 });
  // One deterministic event per topology service (generator.rs::generate_change_events);
  // "web" (index 0) always rotates to the config_change template.
  await expect(page.getByText("Updated rollout config for web")).toBeVisible();

  const blockedPrefixes = ["/v1/events/changes", "/v1/nlq", "/v1/tenants"];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});
