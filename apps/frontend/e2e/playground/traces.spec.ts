import { test, expect } from "@playwright/test";

// Proves the Traces vertical slice (histogram + NLQ execute + tenant/environment
// context) renders entirely from the in-memory playgroundRuntime stub, with no
// HTTP calls to /v1/traces, /v1/nlq, or /v1/tenants.
// See docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md.
test("traces page renders from the playground runtime with no backend calls", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/traces");

  await expect(page.getByRole("table", { name: "Trace results" })).toBeVisible();
  await expect(page.getByRole("cell", { name: "checkout", exact: true })).toBeVisible();
  await expect(page.getByRole("cell", { name: "payment", exact: true })).toBeVisible();

  const blockedPrefixes = ["/v1/traces", "/v1/nlq", "/v1/tenants"];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});
