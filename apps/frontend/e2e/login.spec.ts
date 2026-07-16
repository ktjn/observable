import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test.describe("login page", () => {
  test("auto-redirects to OIDC when no error param", async ({ page }) => {
    let loginRedirect = false;
    await page.route("**/v1/auth/login**", (route) => {
      loginRedirect = true;
      route.fulfill({ status: 302, headers: { Location: "/login?error=auth_failed" } });
    });
    await page.goto("/login");
    expect(loginRedirect).toBe(true);
  });

  test("shows session_expired error message", async ({ page }) => {
    await page.route("**/v1/auth/login**", (route) => route.abort());
    await page.goto("/login?error=session_expired");
    await expect(page.locator("[role='alert']")).toContainText(
      "Your sign-in session expired"
    );
  });

  test("shows auth_failed error message", async ({ page }) => {
    await page.route("**/v1/auth/login**", (route) => route.abort());
    await page.goto("/login?error=auth_failed");
    await expect(page.locator("[role='alert']")).toContainText("Sign-in failed");
  });

  test("shows no_access error message", async ({ page }) => {
    await page.route("**/v1/auth/login**", (route) => route.abort());
    await page.goto("/login?error=no_access");
    await expect(page.locator("[role='alert']")).toContainText("no access");
  });

  test("shows provider_error error message", async ({ page }) => {
    await page.route("**/v1/auth/login**", (route) => route.abort());
    await page.goto("/login?error=provider_error");
    await expect(page.locator("[role='alert']")).toContainText(
      "identity provider is unavailable"
    );
  });

  test("shows server_error error message", async ({ page }) => {
    await page.route("**/v1/auth/login**", (route) => route.abort());
    await page.goto("/login?error=server_error");
    await expect(page.locator("[role='alert']")).toContainText("internal error");
  });

  test("shows fallback error for unknown error code", async ({ page }) => {
    await page.route("**/v1/auth/login**", (route) => route.abort());
    await page.goto("/login?error=some_unknown_code");
    await expect(page.locator("[role='alert']")).toContainText("Sign-in failed");
  });

  test("Sign in button is visible when error is shown", async ({ page }) => {
    await page.route("**/v1/auth/login**", (route) => route.abort());
    await page.goto("/login?error=auth_failed");
    await expect(page.getByRole("button", { name: "Sign in" })).toBeVisible();
  });

  test("passes accessibility audit", async ({ page }) => {
    await page.route("**/v1/auth/login**", (route) => route.abort());
    await page.goto("/login?error=auth_failed");
    await page.waitForSelector("[role='alert']");
    const results = await new AxeBuilder({ page }).analyze();
    expect(results.violations).toEqual([]);
  });
});
