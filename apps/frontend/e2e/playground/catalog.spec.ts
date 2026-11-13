import { expect, test } from "@playwright/test";

test("catalog works through the offline playground runtime", async ({ page }) => {
  const catalogRequests: string[] = [];
  page.on("request", (request) => {
    if (new URL(request.url()).pathname.startsWith("/v1/catalog/")) catalogRequests.push(request.url());
  });
  await page.goto("./#/catalog");
  const search = page.getByRole("searchbox", { name: "Search services" });
  await search.fill("orders");
  await search.press("Enter");
  await page.getByRole("link", { name: "Checkout", exact: true }).click();
  await expect(page.getByText("Checkout operations (Active)")).toBeVisible();
  await expect(page.getByText("No active objective")).toBeVisible();
  expect(catalogRequests).toEqual([]);
});
