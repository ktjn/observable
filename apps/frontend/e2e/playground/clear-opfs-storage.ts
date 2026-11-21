import { chromium, type FullConfig } from "@playwright/test";

/**
 * Global setup for the playground e2e suite: clears any OPFS-backed
 * DuckDB-WASM storage left over from a previous run before tests start.
 * Without this, a completed "Reset playground" (or live-tick-appended
 * data) from one run persists into the next (ADR-047 §3's OPFS durability
 * is origin-scoped, not test-run-scoped), breaking tests that assume a
 * pristine, deterministic dataset.
 */
export default async function globalSetup(config: FullConfig): Promise<void> {
  const baseURL = config.projects[0]?.use?.baseURL;
  if (!baseURL) return;

  const browser = await chromium.launch();
  try {
    const page = await browser.newPage();
    // Navigate to a static asset, not the SPA entrypoint: loading the app
    // itself would start the DuckDB-WASM engine, which opens the OPFS file
    // and holds a lock that `removeEntry` can't clear (NoModificationAllowedError).
    await page.goto(new URL("playground/wasm/package.json", baseURL).toString());
    await page.evaluate(async () => {
      const root = await navigator.storage.getDirectory();
      for await (const name of (root as unknown as { keys(): AsyncIterable<string> }).keys()) {
        await root.removeEntry(name, { recursive: true }).catch(() => {});
      }
    });
  } finally {
    await browser.close();
  }
}
