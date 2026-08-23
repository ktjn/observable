import { test, expect } from "@playwright/test";

// Phase 5 parity: the trace-compare page must load traces through the
// runtime seam (runtime.traces.get -> engine worker DuckDB query), never a
// raw /v1/traces fetch, so it works identically in the playground build.
test("trace compare loads both traces from the real engine with no backend calls", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.goto("#/traces");
  const table = page.getByRole("table", { name: "Trace results" });
  await expect(table).toBeVisible({ timeout: 15_000 });

  // Drill into the first trace, then follow its "Compare trace" link so the
  // left input arrives prefilled with a real engine-generated trace id.
  await table.getByRole("link").first().click();
  await expect(page.getByText("Total Spans")).toBeVisible({ timeout: 15_000 });
  await page.getByRole("link", { name: "Compare trace" }).click();
  await expect(page.getByRole("heading", { name: "Trace comparison" })).toBeVisible();

  const leftInput = page.getByLabel("Left trace id");
  const leftId = await leftInput.inputValue();
  expect(leftId).not.toBe("");

  await page.getByLabel("Right trace id").fill(leftId);
  await page.getByRole("button", { name: "Compare" }).click();

  await expect(page.getByText("Comparison summary")).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("Shared path")).toBeVisible();

  const blockedPrefixes = ["/v1/traces", "/v1/nlq", "/v1/tenants"];
  const backendCalls = requests.filter((path) => blockedPrefixes.some((prefix) => path.startsWith(prefix)));
  expect(backendCalls).toEqual([]);
});
