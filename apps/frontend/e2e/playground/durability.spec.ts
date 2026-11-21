import { test, expect } from "@playwright/test";

// Proves the playground engine's DuckDB-WASM storage is OPFS-backed
// (ADR-047 §3): data survives a page reload instead of vanishing with the
// in-memory default. "Reset playground" reseeds with a fresh random seed;
// if a reload re-triggers seeding (no persistence), the table would revert
// to the engine's default seed=1 dataset instead of keeping the reset one.
test("trace data persists in OPFS-backed storage across a page reload", async ({ page, baseURL }) => {
  await page.goto("#/traces");
  const table = page.getByRole("table", { name: "Trace results" });
  await expect(table).toBeVisible({ timeout: 15_000 });

  const resetButton = page.getByRole("button", { name: "Reset playground", exact: true });
  await expect(resetButton).toBeVisible();
  await resetButton.click();
  await page.getByRole("button", { name: "Reset Playground", exact: true }).click();
  await expect(resetButton).toHaveText("Reset playground", { timeout: 15_000 });

  const firstRowBefore = await table.getByRole("row").nth(1).innerText();

  await page.reload();
  await expect(table).toBeVisible({ timeout: 15_000 });
  const firstRowAfter = await table.getByRole("row").nth(1).innerText();

  expect(firstRowAfter).toBe(firstRowBefore);

  // This test deliberately mutates OPFS-persisted state (the whole point is
  // to prove it survives a reload) — clean it up so it doesn't leak into
  // whatever test runs next in this worker. Navigating to a static asset
  // (not the SPA) avoids re-opening the engine's OPFS lock on this page.
  await page.goto(new URL("playground/wasm/package.json", baseURL!).toString());
  await page.evaluate(async () => {
    const root = await navigator.storage.getDirectory();
    for await (const name of (root as unknown as { keys(): AsyncIterable<string> }).keys()) {
      await root.removeEntry(name, { recursive: true }).catch(() => {});
    }
  });
});
