import { test, expect } from "@playwright/test";
import type { Page } from "@playwright/test";

/**
 * Browser end-to-end proof of read-only view subscriptions (PUSH-004/005/009/
 * 010/014/019) against the built playground artifact.
 *
 * The stream source is the programmable playground fake, exposed on
 * `window.__observableViewSubscriptions` by the harness page — no backend, no
 * broker, no real `EventSource`. What is proved here is the browser half of the
 * contract: snapshot and delta reach the rendered view without a second GET,
 * status transitions are visible, UI state survives a reconnect, and a tenant
 * switch removes the previous tenant's results.
 */

/** The slice of the playground fake's control surface this spec drives. */
interface ViewSubscriptionControl {
  delta(viewId: string, changes: { op: string; key: string; value?: unknown }[]): void;
  replace(viewId: string, data: unknown): void;
  resendLast(): void;
  resendStale(data?: unknown): void;
  setSnapshotData(viewId: string, data: unknown): void;
  setAutoConnect(auto: boolean): void;
  isConnected(): boolean;
  connect(): void;
  disconnect(kind?: "transient" | "fatal"): void;
  reset(reason?: string): void;
  switchTenant(tenantId: string): void;
}

declare global {
  interface Window {
    __observableViewSubscriptions?: ViewSubscriptionControl;
  }
}

const HARNESS = "#/playground-view-subscriptions";
const SUBSCRIPTION_PATH = "/v1/ui/subscriptions/events";
const VIEW_ID = "service-status";
const OTHER_TENANT = "11111111-1111-4111-8111-111111111111";

async function openHarness(page: Page) {
  await page.goto(HARNESS);
  await expect(page.getByTestId("subscription-status")).toHaveText("live", { timeout: 20_000 });
}

test("renders the initial authorized snapshot and goes live", async ({ page }) => {
  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await openHarness(page);

  await expect(page.getByTestId("row-count")).toHaveText("2");
  await expect(page.getByTestId("health-checkout")).toHaveText("normal");
  await expect(page.getByTestId("health-payments")).toHaveText("normal");

  // The fake streams in-browser: nothing was requested from a backend.
  expect(requests.filter((path) => path.startsWith(SUBSCRIPTION_PATH))).toEqual([]);
});

test("a pushed delta updates the view without refetching it", async ({ page }) => {
  await openHarness(page);

  const requests: string[] = [];
  page.on("request", (req) => requests.push(new URL(req.url()).pathname));

  await page.evaluate((viewId) => {
    window.__observableViewSubscriptions!.delta(viewId, [
      { op: "upsert", key: "payments", value: { health: "degraded" } },
    ]);
  }, VIEW_ID);

  await expect(page.getByTestId("health-payments")).toHaveText("degraded");
  // The unrelated row is left exactly as it was.
  await expect(page.getByTestId("health-checkout")).toHaveText("normal");
  expect(requests.filter((path) => path.startsWith("/v1/"))).toEqual([]);
});

test("a duplicate delivery never regresses the displayed result", async ({ page }) => {
  await openHarness(page);

  await page.evaluate((viewId) => {
    window.__observableViewSubscriptions!.replace(viewId, { payments: { health: "degraded" } });
  }, VIEW_ID);
  await expect(page.getByTestId("health-payments")).toHaveText("degraded");

  await page.evaluate((viewId) => {
    const control = window.__observableViewSubscriptions!;
    control.resendLast();
    control.resendStale({ payments: { health: "from-the-past" } });
    control.delta(viewId, [{ op: "upsert", key: "checkout", value: { health: "normal" } }]);
  }, VIEW_ID);

  // The stale re-delivery is discarded; the later legitimate event still applies.
  await expect(page.getByTestId("health-checkout")).toHaveText("normal");
  await expect(page.getByTestId("health-payments")).toHaveText("degraded");
});

test("a dropped stream shows reconnecting and returns to live, preserving UI state", async ({
  page,
}) => {
  await openHarness(page);

  await page.getByRole("button", { name: "payments" }).click();
  await page.getByTestId("unsaved-note").fill("investigating");
  await expect(page.getByTestId("selected-row")).toHaveText("payments");

  // Hold the reconnect open so the degraded state is observable, not a race.
  await page.evaluate(() => {
    const control = window.__observableViewSubscriptions!;
    control.setAutoConnect(false);
    control.disconnect();
  });
  await expect(page.getByTestId("subscription-status")).toHaveText("reconnecting");

  // Selection and unsaved input survive the reconnect (PUSH-005).
  await expect(page.getByTestId("selected-row")).toHaveText("payments");
  await expect(page.getByTestId("unsaved-note")).toHaveValue("investigating");
  await expect(page.getByTestId("health-checkout")).toHaveText("normal");

  // The manager reconnects on its own backoff; wait for that attempt before
  // letting it complete the handshake.
  await page.waitForFunction(() => window.__observableViewSubscriptions!.isConnected());
  await page.evaluate(() => {
    const control = window.__observableViewSubscriptions!;
    control.setAutoConnect(true);
    control.connect();
  });
  await expect(page.getByTestId("subscription-status")).toHaveText("live", { timeout: 20_000 });
  await expect(page.getByTestId("selected-row")).toHaveText("payments");
  await expect(page.getByTestId("unsaved-note")).toHaveValue("investigating");
});

test("a reset re-registers the views and renders a fresh snapshot", async ({ page }) => {
  await openHarness(page);

  await page.evaluate((viewId) => {
    const control = window.__observableViewSubscriptions!;
    control.setSnapshotData(viewId, { billing: { health: "normal" } });
    control.reset("cursor_unavailable");
  }, VIEW_ID);

  await expect(page.getByTestId("subscription-reset-reason")).toHaveText("cursor_unavailable");
  await expect(page.getByTestId("health-billing")).toHaveText("normal", { timeout: 20_000 });
  await expect(page.getByTestId("row-count")).toHaveText("1");
});

test("a tenant switch clears the previous tenant's view", async ({ page }) => {
  await openHarness(page);
  await expect(page.getByTestId("row-count")).toHaveText("2");

  await page.evaluate(
    ([viewId, tenantId]) => {
      const control = window.__observableViewSubscriptions!;
      control.setSnapshotData(viewId, { ledger: { health: "normal" } });
      control.switchTenant(tenantId);
    },
    [VIEW_ID, OTHER_TENANT] as const,
  );

  await expect(page.getByTestId("subscription-tenant")).toHaveText(OTHER_TENANT);
  // The previous tenant's rows are gone, then the new tenant's snapshot lands.
  await expect(page.getByTestId("row-checkout")).toHaveCount(0);
  await expect(page.getByTestId("row-payments")).toHaveCount(0);
  await expect(page.getByTestId("health-ledger")).toHaveText("normal", { timeout: 20_000 });
  await expect(page.getByTestId("row-count")).toHaveText("1");
});
