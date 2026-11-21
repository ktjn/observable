import { test, expect } from "@playwright/test";

// Proves DuckDB-WASM is vendored into the build, not fetched from jsDelivr
// at runtime (ADR-047 §5, embeddable delivery): a third-party site
// embedding this engine may have a CSP that blocks cross-origin script/
// wasm fetches Observable has no control over. Every engine asset must
// resolve same-origin.
test("DuckDB-WASM assets load same-origin, with no jsDelivr/CDN fetch", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(req.url()));

  await page.goto("#/traces");
  await expect(page.getByRole("table", { name: "Trace results" })).toBeVisible({ timeout: 15_000 });

  const jsDelivrRequests = requests.filter((url) => url.includes("jsdelivr"));
  expect(jsDelivrRequests).toEqual([]);

  const duckdbRequests = requests.filter((url) => url.includes("duckdb"));
  expect(duckdbRequests.length).toBeGreaterThan(0);
  for (const url of duckdbRequests) {
    expect(url.startsWith("http://localhost")).toBe(true);
  }
});
