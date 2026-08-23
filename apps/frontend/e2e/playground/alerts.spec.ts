import { test, expect } from "@playwright/test";

// Proves the Alerts vertical renders through the runtime seam in the
// playground build (fixture-backed alert rules, SLOs, and notification
// channels) with no /v1/* backend calls.
test("alerts page renders rules and slos from the runtime with no backend calls", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/alerts");

  await expect(page.getByRole("heading", { level: 1, name: "Alerts" })).toBeVisible({
    timeout: 15_000,
  });

  // Fixture rule from playgroundRuntime's seeded store.
  await expect(page.getByText("payment error rate > 5%")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("checkout p95 latency > 500ms")).toBeVisible();

  // The SLO tab content is fixture-backed too.
  await page.getByRole("tab", { name: "SLOs" }).click();
  await expect(page.getByText("checkout availability SLO").first()).toBeVisible({
    timeout: 15_000,
  });

  const blockedPrefixes = ["/v1/alerts", "/v1/slos", "/v1/notifications", "/v1/nlq", "/v1/tenants"];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});
