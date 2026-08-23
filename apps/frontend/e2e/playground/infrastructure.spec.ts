import { test, expect } from "@playwright/test";

// Proves the Infrastructure vertical renders through the runtime seam in the
// playground build (fixture-backed inventory via runtime.nlq.execute's
// inventory operation, plus runtime.infrastructure.get for drilldown) with
// no /v1/* backend calls.
test("infrastructure inventory and detail render from the runtime with no backend calls", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/infrastructure");

  await expect(page.getByRole("heading", { level: 1, name: "Infrastructure" })).toBeVisible({
    timeout: 15_000,
  });

  // Fixture entities from playgroundRuntime's seeded tree.
  await expect(page.getByText("demo-node-1").first()).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("payment-6c4d7a9b8-m8wqz").first()).toBeVisible();

  // Drill into a pod entity via its display-name link.
  await page.getByRole("link", { name: "checkout-7f9d8b6c5-x2p4k" }).click();
  await expect(page.getByRole("heading", { level: 1, name: /checkout-7f9d8b6c5-x2p4k/ })).toBeVisible({
    timeout: 15_000,
  });

  const blockedPrefixes = [
    "/v1/infrastructure",
    "/v1/nlq",
    "/v1/tenants",
    "/v1/logs",
    "/v1/traces",
    "/v1/metrics",
  ];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});
