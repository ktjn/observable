import { test, expect } from "@playwright/test";

// Proves Dashboards CRUD works entirely client-side against the playground's
// in-memory dashboard store (no DuckDB/wasm involved — dashboards are
// user-created content, not generated analytical data) with no HTTP calls
// to /v1/dashboards, /v1/nlq, or /v1/tenants. See
// docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md
// Phase 5.
test("create, open, and delete a dashboard entirely client-side", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/dashboards");
  await expect(page.getByText("No dashboards yet")).toBeVisible({ timeout: 15_000 });

  await page.getByRole("button", { name: "New dashboard" }).click();
  await page.getByRole("textbox", { name: "New dashboard name" }).fill("Playground test dashboard");
  await page.getByRole("button", { name: "Create" }).click();

  // Create navigates straight to the new dashboard's detail page.
  await expect(page.getByRole("heading", { name: "Playground test dashboard" })).toBeVisible();

  await page.goto("#/dashboards");
  await expect(page.getByText("Playground test dashboard")).toBeVisible();

  page.once("dialog", (dialog) => dialog.accept());
  await page.getByRole("button", { name: "Delete" }).click();
  await expect(page.getByText("No dashboards yet")).toBeVisible();

  const blockedPrefixes = ["/v1/dashboards", "/v1/nlq", "/v1/tenants"];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});
