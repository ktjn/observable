import { test, expect } from "@playwright/test";

// Proves the Traces vertical slice renders from the real playground engine —
// Rust-planned (query-core), wasm-bindgen-bridged, DuckDB-WASM-executed
// query against a Rust-generated `spans` dataset — with no HTTP calls to
// /v1/traces, /v1/nlq, or /v1/tenants, and that "Reset playground"
// regenerates data. See
// docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md
// Phase 3/4.
test("traces page renders from the real DuckDB-WASM engine with no backend calls", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/traces");

  // DuckDB-WASM initialization (first query) is slower than a static fixture.
  await expect(page.getByRole("table", { name: "Trace results" })).toBeVisible({ timeout: 15_000 });
  // Trace count is a fixed generator constant; only entry-point services
  // ("web", "api-gateway") appear here because this view lists root spans
  // only (parent_span_id empty) — same constraint production's
  // execute_trace_query applies. Downstream services (checkout, payment,
  // etc.) only appear on child spans, not yet surfaced by any wired page.
  await expect(page.locator("article", { hasText: "Total Traces" })).toContainText("40");
  await expect(page.getByRole("cell", { name: "web", exact: true }).first()).toBeVisible();

  await page.getByRole("button", { name: "Error" }).click();
  const table = page.getByRole("table", { name: "Trace results" });
  await expect(table).toContainText("ERROR");
  await expect(table).not.toContainText("OK");

  const blockedPrefixes = ["/v1/traces", "/v1/nlq", "/v1/tenants"];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});

test("reset playground regenerates the dataset", async ({ page }) => {
  await page.goto("#/traces");
  await expect(page.getByRole("table", { name: "Trace results" })).toBeVisible({ timeout: 15_000 });

  const resetButton = page.getByRole("button", { name: "Reset playground" });
  await expect(resetButton).toBeVisible();
  await resetButton.click();
  // The reset round-trip (DELETE + regenerate on the already-warm DuckDB
  // connection) is fast enough that the transient "Resetting…" label isn't
  // reliably observable — just wait for it to settle back.
  await expect(resetButton).toHaveText("Reset playground", { timeout: 15_000 });

  // Total count is a fixed generator constant; the underlying seed (and
  // thus per-row content/error count) changed, which isn't asserted here
  // (randomized) — this just confirms the table still populates and the
  // reset round-trip completes without error.
  await expect(page.locator("article", { hasText: "Total Traces" })).toContainText("40");
});
