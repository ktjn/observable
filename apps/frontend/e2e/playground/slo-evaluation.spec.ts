import { test, expect } from "@playwright/test";

// Proves service summaries reflect real local SLO burn-rate evaluation
// (ADR-047 §3 sub-slice B) — calculate_burn_rate/evaluate_burn_rate,
// shared with alert-evaluator — instead of the previously hardcoded
// `slo_breaching: false`. `payment` has the highest fixed error weight
// in the generator (0.15, vs 0.04 for most services — see
// generator.rs::error_weight), so it reliably burns through its seeded
// SLO's error budget; `web` (0.04) does not.
test("services reflect real SLO burn-rate evaluation, not a hardcoded default", async ({ page }) => {
  await page.goto("#/services/payment");
  const sloLink = page.getByText(/Breaching|Meeting commitments/);
  await expect(sloLink).toBeVisible({ timeout: 15_000 });
  await expect(sloLink).toContainText("Breaching");

  await page.goto("#/services/web");
  const webSloLink = page.getByText(/Breaching|Meeting commitments/);
  await expect(webSloLink).toBeVisible({ timeout: 15_000 });
  await expect(webSloLink).toContainText("Meeting commitments");
});
