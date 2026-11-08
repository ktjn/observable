import { expect, test } from "@playwright/test";
import { checkout, mockCatalog, restrictedServiceId } from "./catalogFixtures";

test.beforeEach(async ({ page }) => {
  await mockCatalog(page);
});

test("ux-01-restricted-service gives restricted and missing links the same view", async ({ page }) => {
  const unavailable = page.getByRole("heading", { name: "Service not found" });
  const requested: string[] = [];
  page.on("request", (request) => {
    const path = new URL(request.url()).pathname;
    if (path.startsWith("/v1/catalog/")) requested.push(path);
  });
  await page.goto(`/catalog/services/${checkout.id}`);
  await expect(page.getByText("Payments team", { exact: false })).toBeVisible();
  for (const id of [restrictedServiceId, "00000000-0000-0000-0000-000000000013"]) {
    await page.goto(`/catalog/services/${id}`);
    await expect(unavailable).toBeVisible();
    await expect(page.getByText("Payments team", { exact: false })).toHaveCount(0);
    await expect(page.getByRole("heading", { name: "Monitoring coverage" })).toHaveCount(0);
    expect(new Set(requested.filter((path) => path.includes(id)))).toEqual(new Set([`/v1/catalog/services/${id}`]));
  }
});

test("ux-01-search-service-restricted shows the same empty state as a nonmatch", async ({ page }) => {
  await page.goto("/catalog");
  const search = page.getByRole("searchbox", { name: "Search services" });
  for (const term of ["Checkout", "checkout-api", "Commerce", "Payments team"]) {
    const response = page.waitForResponse((res) => new URL(res.url()).searchParams.get("q") === term);
    await search.fill(term);
    await search.press("Enter");
    await response;
    await expect(page.getByRole("link", { name: "Checkout", exact: true })).toBeVisible();
    await expect(page.getByRole("link", { name: "Restricted Payments" })).toHaveCount(0);
  }
  for (const term of ["Restricted Payments", "Unregistered Example", "old-checkout"]) {
    const response = page.waitForResponse((res) => new URL(res.url()).searchParams.get("q") === term);
    await search.fill(term);
    await search.press("Enter");
    await response;
    await expect(page.getByRole("heading", { name: "No matching services" })).toBeVisible();
    await expect(page.getByRole("table")).toHaveCount(0);
    await expect(page.getByRole("link", { name: term })).toHaveCount(0);
  }
});

test("a retired service retains its lifecycle and flags departed ownership", async ({ page }) => {
  await page.route(`**/v1/catalog/services/${checkout.id}`, (route) => route.fulfill({ json: {
    ...checkout, lifecycle: "retired", live: false,
    owners: [{ ...checkout.owners[0], state: "departed" }, checkout.owners[1]],
    exceptions: ["business_owner_departed"],
  } }));
  await page.goto(`/catalog/services/${checkout.id}`);
  await expect(page.getByText("Retired", { exact: true })).toBeVisible();
  await expect(page.getByText("Commerce team (Departed)")).toBeVisible();
  await expect(page.getByText("Business owner departed")).toBeVisible();
  await expect(page.getByText("Coverage assessments are unavailable for retired services.")).toBeVisible();
});

test("authorization outage shows a retryable error and no service details", async ({ page }) => {
  await page.route(`**/v1/catalog/services/${checkout.id}`, (route) =>
    route.fulfill({ status: 503, json: { error: "access_decision_unavailable" } }));
  await page.goto(`/catalog/services/${checkout.id}`);
  await expect(page.getByRole("alert")).toContainText("Could not load service");
  await expect(page.getByRole("heading", { name: "Service not found" })).toHaveCount(0);
  await expect(page.getByText("Payments team")).toHaveCount(0);
  await page.unroute(`**/v1/catalog/services/${checkout.id}`);
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(page.getByRole("heading", { name: "Checkout", exact: true })).toBeVisible();
});
