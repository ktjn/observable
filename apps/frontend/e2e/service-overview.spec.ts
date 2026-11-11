import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { checkout, mockCatalog } from "./catalogFixtures";

const id = (n: number) => `00000000-0000-0000-0000-0000000002${String(n).padStart(2, "0")}`;
const states = ["normal", "degraded", "unavailable", "planned_maintenance", "unknown"] as const;
const names = ["Billing", "Checkout", "Payments", "Search", "Shipping"];

type Item = { state: string | null; evidence?: string; age?: number | null; issues?: { status: string; open: number | null }; fit?: boolean; reasons?: string[] };

function overview(items: Item[], confidence: { confidence: string; affected: string[] } = { confidence: "full", affected: [] }) {
  return {
    scope: { environment_class: "production", context: "business_critical" },
    period: { as_of: "2026-11-10T12:00:00Z", freshness_seconds: 300 },
    confidence: { ...confidence, functions: [], checked_at: "2026-11-10T12:00:00Z" },
    total: items.length,
    items: items.map((item, i) => ({
      service: { id: id(i), business_name: names[i], criticality: "critical", lifecycle: "live" },
      live: true,
      state: item.state,
      freshness: { status: item.evidence ?? "current", last_evidence_at: "2026-11-10T11:59:30Z", age_seconds: item.age ?? 30, bound_seconds: 300 },
      issues: item.issues ?? { status: "current", open: 0 },
      fit_for_decision: item.fit ?? true,
      unfit_reasons: item.reasons ?? [],
    })),
  };
}

const fiveStates = () => overview(states.map((state) => state === "unknown"
  ? { state, evidence: "stale", age: 1200, issues: { status: "evaluation_unavailable", open: null }, fit: false, reasons: ["evidence_stale"] }
  : { state }));

/** Serves `bodies[0]` until `next()` moves on, so a refresh is the only way to see a change. */
async function serve(page: Page, bodies: unknown[]) {
  let current = 0;
  await page.route("**/v1/overview/services?**", (route) => {
    const url = new URL(route.request().url());
    expect(url.searchParams.get("environment_class")).toBe("production");
    const body = bodies[current];
    return body === 503 ? route.fulfill({ status: 503, json: { error: "catalog_unavailable" } }) : route.fulfill({ json: body });
  });
  return { next: () => { current = Math.min(current + 1, bodies.length - 1); } };
}

test.beforeEach(async ({ page }) => {
  await mockCatalog(page);
});

test("ux-07 all five states read by text and shape, in grayscale and for screen readers", async ({ page }) => {
  await serve(page, [fiveStates()]);
  await page.goto("/catalog?view=overview");
  const table = page.getByRole("table", { name: "Service overview" });
  await expect(table).toBeVisible();
  // Grayscale: no state may rely on colour alone.
  await page.addStyleTag({ content: "html { filter: grayscale(1) !important; }" });
  const shapes = await table.locator("svg[data-shape]").evaluateAll((els) => els.map((el) => el.getAttribute("data-shape")));
  expect(new Set(shapes).size).toBe(5);
  for (const label of ["Normal", "Degraded", "Unavailable", "Planned maintenance", "Unknown"]) {
    await expect(table.getByRole("img", { name: new RegExp(`^Health: ${label},`) })).toBeVisible();
  }
  const results = await new AxeBuilder({ page }).include("main").analyze();
  expect(results.violations.filter((v) => v.impact === "critical" || v.impact === "serious")).toEqual([]);
});

test("ux-07 a failed source never shows zero or normal", async ({ page }) => {
  await serve(page, [fiveStates()]);
  await page.goto("/catalog?view=overview");
  const shipping = page.getByRole("row", { name: /^Shipping/ });
  await expect(shipping).toContainText("Stale");
  await expect(shipping).toContainText("Unavailable");
  await expect(shipping).toContainText("Not fit: evidence is stale");
  await expect(shipping).not.toContainText("None open");
  await expect(shipping).not.toContainText(/\b0\b/);
  await expect(shipping).not.toContainText("Normal");
});

test("ux-07 a degraded platform function shows a loss-of-confidence warning", async ({ page }) => {
  await serve(page, [overview([{ state: "normal", fit: false, reasons: ["confidence_degraded"] }], { confidence: "degraded", affected: ["detect"] })]);
  await page.goto("/catalog?view=overview");
  const notice = page.getByRole("status", { name: "Platform confidence" });
  await expect(notice).toContainText("Loss of confidence");
  await expect(notice).toContainText("alert detection");
  await expect(page.getByRole("row", { name: /^Billing/ })).toContainText("Not fit: platform confidence is degraded");
});

test("ux-07 an overview that cannot be read is an error, not an empty or healthy list", async ({ page }) => {
  await serve(page, [503]);
  await page.goto("/catalog?view=overview");
  await expect(page.getByRole("alert")).toContainText("Could not load service overview");
  await expect(page.getByRole("table", { name: "Service overview" })).toHaveCount(0);
  await expect(page.getByRole("img", { name: /^Health:/ })).toHaveCount(0);
});

test("ux-07 refresh keeps the inspection, sort and filter and says what changed", async ({ page }) => {
  const before = overview([{ state: "normal" }, { state: "degraded" }, { state: "normal" }]);
  const after = overview([{ state: "degraded" }, { state: "degraded" }, { state: "normal" }]);
  const served = await serve(page, [before, after]);
  await page.goto("/catalog?view=overview");
  await page.getByRole("combobox", { name: "Sort by" }).selectOption("name");
  await page.getByRole("combobox", { name: "State", exact: true }).selectOption("degraded");
  await page.getByRole("button", { name: "Inspect Checkout" }).click();
  await expect(page.locator("section", { has: page.getByRole("heading", { name: "Inspection" }) })).toContainText("Checkout");
  served.next();
  await page.getByRole("button", { name: "Refresh now" }).click();
  await expect(page.getByRole("status").filter({ hasText: "changed state" })).toContainText("Billing changed state");
  const url = new URL(page.url());
  expect(Object.fromEntries(url.searchParams)).toMatchObject({ view: "overview", sort: "name", state: "degraded", selected: id(1) });
  const rows = page.getByRole("table", { name: "Service overview" }).getByRole("row");
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(1)).toContainText("Billing");
  await expect(rows.nth(2)).toContainText("Checkout");
  await expect(page.getByRole("button", { name: "Inspect Checkout" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator("section", { has: page.getByRole("heading", { name: "Inspection" }) })).toContainText("Checkout");
});

test("ux-07 the service detail health panel shows freshness, issues and fitness", async ({ page }) => {
  await page.route(`**/v1/review/services/${checkout.id}?**`, (route) => route.fulfill({ json: {
    service: { id: checkout.id, business_name: "Checkout", criticality: "critical", lifecycle: "live" },
    environment_class: "production", live: true,
    health: { state: "unknown", evidence_status: "stale", evaluated_at: "2026-11-10T12:00:00Z", last_evidence_at: "2026-11-10T11:40:00Z", freshness_seconds: 300 },
    expectation: { status: "unknown", reason: "stale", objectives: [] },
    freshness: { status: "stale", last_evidence_at: "2026-11-10T11:40:00Z", age_seconds: 1200, bound_seconds: 300 },
    issues: { status: "evaluation_unavailable", open: null },
    fit_for_decision: false, unfit_reasons: ["evidence_stale", "confidence_unknown"],
    confidence: { confidence: "unknown", affected: ["assess"], functions: [], checked_at: "2026-11-10T12:00:00Z" },
    period: { as_of: "2026-11-10T12:00:00Z", freshness_seconds: 300 },
  } }));
  await page.goto(`/catalog/services/${checkout.id}`);
  const panel = page.locator("section", { has: page.getByRole("heading", { name: "Health and expectations" }) });
  await expect(panel.getByRole("img", { name: "Health: Unknown, Stale evidence" })).toBeVisible();
  await expect(panel.getByLabel("View freshness")).toContainText("Evidence older than 5 min is stale");
  await expect(panel.getByRole("status", { name: "Platform confidence" })).toContainText("health assessment");
  await expect(panel).toContainText("Not fit: evidence is stale; platform confidence is unknown");
  await expect(panel).not.toContainText("None open");
});
