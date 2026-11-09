import { expect, test } from "@playwright/test";
import { checkout, mockCatalog, restrictedServiceId, review } from "./catalogFixtures";

test.beforeEach(async ({ page }) => {
  await mockCatalog(page);
});

test("ux-01-restricted-service gives restricted and missing links the same view", async ({ page }) => {
  const unavailable = page.getByRole("heading", { name: "Service not found" });
  const requested: string[] = [];
  page.on("request", (request) => {
    const path = new URL(request.url()).pathname;
    if (path.startsWith("/v1/catalog/") || path.startsWith("/v1/review/")) requested.push(path);
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

test("service review keeps a non-production failure out of production health and expectations", async ({ page }) => {
  await page.route(`**/v1/review/services/${checkout.id}?**`, (route) => {
    const environmentClass = new URL(route.request().url()).searchParams.get("environment_class") ?? "";
    route.fulfill({ json: environmentClass === "production"
      ? review(environmentClass, checkout)
      : review(environmentClass, checkout, { health: ["degraded", "current"], expectation: ["breached", "current"] }) });
  });
  await page.goto(`/catalog/services/${checkout.id}`);
  const panel = page.locator("section", { has: page.getByRole("heading", { name: "Health and expectations" }) });
  await expect(panel.getByRole("button", { name: "Production", exact: true })).toHaveAttribute("aria-pressed", "true");
  await expect(panel.getByText("Normal", { exact: true })).toBeVisible();
  await expect(panel.getByText("Compliant", { exact: true }).first()).toBeVisible();
  await panel.getByRole("button", { name: "Non-production" }).click();
  await expect(panel.getByText("Degraded", { exact: true })).toBeVisible();
  await expect(panel.getByText("Breached", { exact: true }).first()).toBeVisible();
  await panel.getByRole("button", { name: "Production", exact: true }).click();
  await expect(panel.getByText("Normal", { exact: true })).toBeVisible();
  await expect(panel.getByText("Breached", { exact: true })).toHaveCount(0);
});

test("service review report shows all four expectation states and never compliant without evidence", async ({ page }) => {
  const services = ["Compliant", "At risk", "Breached", "Stale"].map((name, i) => ({ id: `00000000-0000-0000-0000-00000000010${i}`, business_name: name }));
  await page.route("**/v1/review/services?**", (route) => {
    const url = new URL(route.request().url());
    expect(url.searchParams.get("context")).toBe("business_critical");
    const environmentClass = url.searchParams.get("environment_class") ?? "";
    route.fulfill({ json: { environment_class: environmentClass, total: 4, items: [
      review(environmentClass, services[0]),
      review(environmentClass, services[1], { expectation: ["at_risk", "current"] }),
      review(environmentClass, services[2], { expectation: ["breached", "current"] }),
      review(environmentClass, services[3], { health: ["unknown", "stale"], expectation: ["unknown", "stale"] }),
    ] } });
  });
  await page.goto("/catalog?view=review");
  const table = page.getByRole("table", { name: "Service review" });
  const row = (name: string) => table.getByRole("row", { name: new RegExp(`^${name}`) });
  await expect(row("Compliant")).toContainText("Compliant");
  await expect(row("At risk")).toContainText("At risk");
  await expect(row("Breached")).toContainText("Breached");
  await expect(row("Stale")).toContainText("Unknown");
  await expect(row("Stale")).toContainText("Stale evidence");
  await expect(row("Stale")).not.toContainText("Compliant");
  await expect(row("Stale")).not.toContainText("Normal");
});

test("service review report lists retired services as not live", async ({ page }) => {
  await page.route("**/v1/review/services?**", (route) => {
    const url = new URL(route.request().url());
    const environmentClass = url.searchParams.get("environment_class") ?? "";
    const items = url.searchParams.get("context") === "all"
      ? [review(environmentClass, checkout), review(environmentClass, { id: restrictedServiceId, business_name: "Legacy checkout" }, { live: false, lifecycle: "retired" })]
      : [review(environmentClass, checkout)];
    route.fulfill({ json: { environment_class: environmentClass, items, total: items.length } });
  });
  await page.goto("/catalog?view=review&env=non_production");
  await expect(page.getByRole("button", { name: "Non-production" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("link", { name: "Legacy checkout" })).toHaveCount(0);
  await page.getByLabel("Services").selectOption("all");
  const retired = page.getByRole("table", { name: "Service review" }).getByRole("row", { name: /^Legacy checkout/ });
  await expect(retired).toContainText("Retired");
  await expect(retired).toContainText("Not live");
  await expect(retired).not.toContainText("Normal");
  await expect(retired).not.toContainText("Compliant");
});

test("a retired service record shows no current health or expectation", async ({ page }) => {
  await page.route(`**/v1/catalog/services/${checkout.id}`, (route) => route.fulfill({ json: { ...checkout, lifecycle: "retired", live: false } }));
  await page.route(`**/v1/review/services/${checkout.id}?**`, (route) => {
    const environmentClass = new URL(route.request().url()).searchParams.get("environment_class") ?? "";
    route.fulfill({ json: review(environmentClass, checkout, { live: false, lifecycle: "retired" }) });
  });
  await page.goto(`/catalog/services/${checkout.id}`);
  await expect(page.getByText("Retired services are not live and have no current health or expectation.")).toBeVisible();
});
