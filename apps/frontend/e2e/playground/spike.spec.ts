import { test, expect } from "@playwright/test";

// Phase 0 viability spike (see docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md).
// Proves, against the built static artifact under the /observable/ base path:
// hash routing, worker + wasm-bindgen loading, and a DuckDB-WASM round trip —
// with no HTTP calls to an Observable backend.
test("playground spike loads WASM + DuckDB-WASM under hash routing", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(req.url()));

  await page.goto("#/playground-spike");

  await expect(page.getByTestId("spike-ready")).toHaveText(
    'ok: ping="observable-playground-wasm:42" rowCount=1',
    { timeout: 30_000 }
  );

  const backendCalls = requests.filter((url) => new URL(url).pathname.startsWith("/v1/auth"));
  expect(backendCalls).toHaveLength(0);
});
