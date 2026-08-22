import { test, expect } from "@playwright/test";

// Proves the Traces vertical slice renders from the real playground engine —
// Rust-planned (query-core), wasm-bindgen-bridged, DuckDB-WASM-executed
// query against a seeded `spans` table — with no HTTP calls to /v1/traces,
// /v1/nlq, or /v1/tenants. See
// docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md Phase 3.
test("traces page renders from the real DuckDB-WASM engine with no backend calls", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/traces");

  // DuckDB-WASM initialization (first query) is slower than the previous
  // static-fixture stub.
  await expect(page.getByRole("table", { name: "Trace results" })).toBeVisible({ timeout: 15_000 });
  await expect(page.locator("article", { hasText: "Total Traces" })).toContainText("3");
  await expect(page.getByRole("cell", { name: "checkout", exact: true }).first()).toBeVisible();
  await expect(page.getByRole("cell", { name: "payment", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Error" }).click();
  const rows = page.getByRole("table", { name: "Trace results" }).getByRole("row");
  await expect(rows).toHaveCount(2); // header + one ERROR row
  await expect(rows.nth(1)).toContainText("ERROR");
  await expect(rows.nth(1)).toContainText("payment");

  const blockedPrefixes = ["/v1/traces", "/v1/nlq", "/v1/tenants"];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});
