import { test, expect } from "@playwright/test";

// Proves the Home page (the playground's landing route) renders real
// service-health data instead of hanging forever on "Checking services…" —
// it previously called api/services.ts's listServiceSummaries directly,
// bypassing the runtime seam entirely, so the query always 404'd with no
// backend. No HTTP calls to /v1/services, /v1/nlq, or /v1/tenants. See
// docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md
// Phase 5.
test("home page renders real service health from the DuckDB-WASM engine with no backend calls", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/");

  await expect(page.getByText("Checking services…")).not.toBeVisible({ timeout: 15_000 });
  await expect(page.getByText(/healthy · \d+ watch · \d+ breach across \d+ services/)).toBeVisible();
  const serviceCount = await page
    .locator("article", { hasText: "Services" })
    .locator("div")
    .nth(1)
    .innerText();
  expect(Number(serviceCount)).toBeGreaterThan(0);

  const blockedPrefixes = ["/v1/services", "/v1/nlq", "/v1/tenants"];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});
