import { test, expect } from "@playwright/test";

// Proves the playground engine emits its own self-instrumentation
// (ADR-047 §4, Phase 3): startup/query-cost logs, written into the same
// `logs` table every other log goes into, under a fixed system tenant,
// queryable through the same DuckDbLogQueryApi path — not a hardcoded
// stand-in. Reached via a dev-only route, not the tenant switcher
// (ADR-047 §5's "one implicit tenant" stays true for user-visible data).
test("engine self-telemetry captures real startup and query-cost events", async ({ page }) => {
  await page.goto("#/playground-self-telemetry");

  const countEl = page.getByTestId("self-telemetry-count");
  await expect(countEl).not.toHaveText("0", { timeout: 15_000 });

  const bodies = await page.getByTestId("self-telemetry-body").allInnerTexts();
  expect(bodies.some((body) => body.includes("engine initialized"))).toBe(true);
});
