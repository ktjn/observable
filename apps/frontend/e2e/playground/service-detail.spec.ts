import { test, expect } from "@playwright/test";

// Proves clicking into a service from the Services list renders a real
// service detail page — Rust-planned (query-core), wasm-bindgen-bridged,
// DuckDB-WASM per-service summary and response-time histogram over the
// Rust-generated `spans` dataset — instead of "Service not found" (the
// summary/history queries previously hit unwired HTTP endpoints that
// 404 with no backend). No HTTP calls to /v1/services, /v1/nlq, or
// /v1/tenants. See
// docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md
// Phase 5.
test("service detail page renders from the real DuckDB-WASM engine with no backend calls", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/services/web");

  // DuckDB-WASM initialization (first query) is slower than a static fixture.
  await expect(page.getByRole("heading", { name: "web", exact: true })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("Service not found")).not.toBeVisible();
  await expect(page.getByLabel("Service performance summary")).toBeVisible();

  const blockedPrefixes = ["/v1/services", "/v1/nlq", "/v1/tenants"];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});

// The Logs/Traces tabs lock the view to this service via a JSON-encoded raw
// NlqIr passed as the NLQ `question` (ADR-034 "Simple IR Shorthand"), not by
// leaving `question` unset. This regression-tests that playgroundRuntime
// recognizes that shorthand and still routes to the real DuckDB engine —
// earlier it fell through to signal-mismatched fixture data (trace rows
// rendered in the logs table) and, once that was fixed, to a ClickHouse-only
// SQL expression DuckDB rejected (a still-relative "now-1h" time_range).
test("locked-service Logs and Traces tabs render real, service-scoped data", async ({ page }) => {
  await page.goto("#/services/web/logs");

  await expect(page.getByRole("table", { name: "Service logs" })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("Failed to load logs")).not.toBeVisible();
  const logsTotal = await page.locator("article", { hasText: "Total Logs" }).locator("div").nth(1).innerText();
  expect(Number(logsTotal)).toBeGreaterThan(0);
  await expect(page.getByRole("cell", { name: /web request/ }).first()).toBeVisible();

  await page.goto("#/services/web/traces");
  await expect(page.getByRole("table", { name: "Service traces" })).toBeVisible({ timeout: 15_000 });
  const tracesTotal = await page.locator("article", { hasText: "Total Traces" }).locator("div").nth(1).innerText();
  expect(Number(tracesTotal)).toBe(Number(logsTotal));
});
