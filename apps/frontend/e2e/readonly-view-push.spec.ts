import { expect, test, type Page } from "@playwright/test";
import { checkout, mockCatalog } from "./catalogFixtures";

const TENANT_ID = "00000000-0000-0000-0000-000000000001";

const FIXTURE_SUMMARY_RESPONSE = {
  items: [
    {
      service_name: "checkout",
      request_rate: 42.5,
      error_rate: 0.001,
      p95_latency_ms: 85,
      slo_breaching: false,
      active_alert_count: 0,
      latest_deployment: "v1.4.2",
    },
    {
      service_name: "payments",
      request_rate: 18.2,
      error_rate: 0.065,
      p95_latency_ms: 620,
      slo_breaching: false,
      active_alert_count: 3,
      latest_deployment: null,
    },
  ],
};

interface SseTestHarness {
  urls: string[];
  emit(eventName: string, payload: unknown): void;
  fail(fatal?: boolean): void;
}

declare global {
  interface Window {
    __sseTestHarness?: SseTestHarness;
  }
}

async function installSseHarness(page: Page) {
  await page.addInitScript(() => {
    const urls: string[] = [];
    let activeInstance: MockEventSource | null = null;

    class MockEventSource {
      static readonly CONNECTING = 0;
      static readonly OPEN = 1;
      static readonly CLOSED = 2;
      readonly CONNECTING = 0;
      readonly OPEN = 1;
      readonly CLOSED = 2;

      readyState = 0;
      url: string;
      withCredentials = false;
      onopen: ((ev: Event) => unknown) | null = null;
      onmessage: ((ev: MessageEvent) => unknown) | null = null;
      onerror: ((ev: Event) => unknown) | null = null;
      private listeners = new Map<string, Set<(ev: MessageEvent) => void>>();

      constructor(url: string | URL) {
        this.url = String(url);
        urls.push(this.url);
        activeInstance = this;
        queueMicrotask(() => {
          if (this.readyState === 2) return;
          this.readyState = 1;
          this.onopen?.(new Event("open"));
        });
      }

      addEventListener(type: string, listener: (ev: MessageEvent) => void) {
        const set = this.listeners.get(type) ?? new Set();
        set.add(listener);
        this.listeners.set(type, set);
      }

      removeEventListener(type: string, listener: (ev: MessageEvent) => void) {
        this.listeners.get(type)?.delete(listener);
      }

      dispatchSse(eventName: string, data: string) {
        if (this.readyState === 2) return;
        const event = new MessageEvent(eventName, { data });
        for (const listener of this.listeners.get(eventName) ?? []) {
          listener(event);
        }
      }

      triggerError(fatal: boolean) {
        if (this.readyState === 2) return;
        this.readyState = fatal ? 2 : 0;
        this.onerror?.(new Event("error"));
      }

      close() {
        this.readyState = 2;
        if (activeInstance === this) {
          activeInstance = null;
        }
      }
    }

    window.__sseTestHarness = {
      urls,
      emit(eventName: string, payload: unknown) {
        activeInstance?.dispatchSse(eventName, JSON.stringify(payload));
      },
      fail(fatal = false) {
        activeInstance?.triggerError(fatal);
      },
    };

    Object.defineProperty(window, "EventSource", {
      configurable: true,
      writable: true,
      value: MockEventSource,
    });
  });
}

test.describe("read-only view SSE push subscriptions (Wave 5 join)", () => {
  test("services_view_receives_sse_snapshot_delta_and_replace_without_polling_or_losing_filter_state", async ({
    page,
  }) => {
    await installSseHarness(page);

    let summaryFetchCount = 0;
    await page.route("**/v1/auth/me", (route) =>
      route.fulfill({
        json: {
          user_id: TENANT_ID,
          email: "test@example.com",
          active_tenant_id: TENANT_ID,
          tenants: [{ tenant_id: TENANT_ID, role: "admin" }],
        },
      }),
    );
    await page.route("**/v1/tenants", (route) =>
      route.fulfill({ json: { tenants: [{ id: TENANT_ID, name: "observable" }] } }),
    );
    await page.route("**/v1/tenants/**/environments", (route) =>
      route.fulfill({ json: { environments: [{ environment: "prod" }] } }),
    );
    await page.route("**/v1/services/summary**", (route) => {
      summaryFetchCount += 1;
      return route.fulfill({ json: FIXTURE_SUMMARY_RESPONSE });
    });

    await page.goto("/services");
    const table = page.getByRole("table", { name: "Service catalog" });
    await expect(table).toBeVisible();
    await expect(page.getByTestId("subscription-status")).toHaveText("Live");
    expect(summaryFetchCount).toBe(1);

    const streamUrls = await page.evaluate(() => window.__sseTestHarness?.urls ?? []);
    expect(streamUrls.length).toBe(1);
    expect(streamUrls[0]).toContain("/v1/ui/subscriptions/events?views=");

    // Change sort order to Active Alerts/RPS so we verify live SSE updates preserve UI state (PUSH-005).
    await table.locator("th", { hasText: "RPS" }).click();
    await expect(table.locator("th", { hasText: "RPS" })).toContainText("▲");

    // Emit an initial snapshot followed by a deployment delta for `payments`.
    await page.evaluate(() => {
      window.__sseTestHarness!.emit("snapshot", {
        v: 1,
        viewId: "service-status",
        queryRevision: "r1",
        revision: "1",
        data: {
          checkout: { active_alert_count: 0, slo_breaching: false, latest_deployment: "v1.4.2" },
          payments: { active_alert_count: 3, slo_breaching: false, latest_deployment: null },
        },
      });
      window.__sseTestHarness!.emit("delta", {
        v: 1,
        viewId: "service-status",
        queryRevision: "r1",
        revision: "2",
        changes: [{ op: "upsert", key: "payments", value: { latest_deployment: "v2.5.0" } }],
      });
    });

    const paymentsRow = table.locator("tr", { hasText: "payments" });
    await expect(paymentsRow).toContainText("v2.5.0");
    await expect(table.locator("tr", { hasText: "checkout" })).toContainText("v1.4.2");
    await expect(table.locator("th", { hasText: "RPS" })).toContainText("▲");
    expect(summaryFetchCount).toBe(1);

    // Emit an authoritative replace updating active_alert_count for `payments` from 3 to 7.
    await page.evaluate(() => {
      window.__sseTestHarness!.emit("replace", {
        v: 1,
        viewId: "service-status",
        queryRevision: "r1",
        revision: "3",
        data: {
          checkout: { active_alert_count: 0, slo_breaching: false, latest_deployment: "v1.4.2" },
          payments: { active_alert_count: 7, slo_breaching: true, latest_deployment: "v2.5.0" },
        },
      });
    });

    await expect(paymentsRow).toContainText("7");
    await expect(page.locator("[aria-label='Service summary']")).toContainText("7");
    expect(summaryFetchCount).toBe(1);
  });

  test("catalog_service_health_panel_updates_directly_from_sse_replace_without_refetch", async ({
    page,
  }) => {
    await installSseHarness(page);
    await mockCatalog(page);

    let reviewFetchCount = 0;
    await page.route(`**/v1/review/services/${checkout.id}?**`, (route) => {
      reviewFetchCount += 1;
      return route.fulfill({
        json: {
          service: {
            id: checkout.id,
            business_name: "Checkout",
            criticality: "critical",
            lifecycle: "live",
          },
          environment_class: "production",
          live: true,
          health: {
            state: "normal",
            evidence_status: "current",
            evaluated_at: "2026-11-18T12:00:00Z",
            last_evidence_at: "2026-11-18T11:59:30Z",
            freshness_seconds: 300,
          },
          expectation: { status: "compliant", reason: "current", objectives: [] },
          freshness: {
            status: "current",
            last_evidence_at: "2026-11-18T11:59:30Z",
            age_seconds: 30,
            bound_seconds: 300,
          },
          issues: { status: "current", open: 0 },
          fit_for_decision: true,
          unfit_reasons: [],
          confidence: {
            confidence: "full",
            affected: [],
            functions: [],
            checked_at: "2026-11-18T12:00:00Z",
          },
          period: { as_of: "2026-11-18T12:00:00Z", freshness_seconds: 300 },
        },
      });
    });

    await page.goto(`/catalog/services/${checkout.id}`);
    const panel = page.locator("section", {
      has: page.getByRole("heading", { name: "Health and expectations" }),
    });
    await expect(panel.getByRole("img", { name: "Health: Normal, Current evidence" })).toBeVisible();
    await expect(panel.getByTestId("subscription-status")).toHaveText("live");
    const baselineFetches = reviewFetchCount;
    expect(baselineFetches).toBeGreaterThanOrEqual(1);

    await page.evaluate(() => {
      window.__sseTestHarness!.emit("replace", {
        v: 1,
        viewId: "service-health",
        queryRevision: "r1",
        revision: "2",
        data: {
          state: "degraded",
          evidence_status: "current",
          evaluated_at: "2026-11-18T12:01:00Z",
          last_evidence_at: "2026-11-18T12:00:50Z",
          freshness_seconds: 300,
        },
      });
    });

    await expect(
      panel.getByRole("img", { name: "Health: Degraded, Current evidence" }),
    ).toBeVisible();
    expect(reviewFetchCount).toBe(baselineFetches);
  });

  test("pinned_historical_range_pauses_sse_subscription_on_services_view", async ({ page }) => {
    await installSseHarness(page);

    await page.route("**/v1/auth/me", (route) =>
      route.fulfill({
        json: {
          user_id: TENANT_ID,
          email: "test@example.com",
          active_tenant_id: TENANT_ID,
          tenants: [{ tenant_id: TENANT_ID, role: "admin" }],
        },
      }),
    );
    await page.route("**/v1/tenants", (route) =>
      route.fulfill({ json: { tenants: [{ id: TENANT_ID, name: "observable" }] } }),
    );
    await page.route("**/v1/tenants/**/environments", (route) =>
      route.fulfill({ json: { environments: [{ environment: "prod" }] } }),
    );
    await page.route("**/v1/services/summary**", (route) =>
      route.fulfill({ json: FIXTURE_SUMMARY_RESPONSE }),
    );

    await page.goto("/services?from=1700000000000&to=1700003600000");
    await expect(page.getByRole("table", { name: "Service catalog" })).toBeVisible();
    await expect(page.getByTestId("subscription-status")).toHaveText("Paused");
    await expect(page.getByTestId("view-freshness-mode")).toHaveText("Historical analysis");

    const streamUrls = await page.evaluate(() => window.__sseTestHarness?.urls ?? []);
    expect(streamUrls).toEqual([]);
  });

  test("traces_view_subscribes_to_telemetry_watermark_and_refreshes_only_when_watermark_advances", async ({
    page,
  }) => {
    await installSseHarness(page);

    let nlqFetchCount = 0;
    await page.route("**/v1/auth/me", (route) =>
      route.fulfill({
        json: {
          user_id: TENANT_ID,
          email: "test@example.com",
          active_tenant_id: TENANT_ID,
          tenants: [{ tenant_id: TENANT_ID, role: "admin" }],
        },
      }),
    );
    await page.route("**/v1/tenants", (route) =>
      route.fulfill({ json: { tenants: [{ id: TENANT_ID, name: "observable" }] } }),
    );
    await page.route("**/v1/tenants/**/environments", (route) =>
      route.fulfill({ json: { environments: [{ environment: "prod" }] } }),
    );
    await page.route("**/v1/saved-views**", (route) =>
      route.fulfill({ json: { items: [] } }),
    );
    await page.route("**/v1/traces/histogram**", (route) =>
      route.fulfill({ json: { buckets: [] } }),
    );
    await page.route("**/v1/nlq", (route) => {
      nlqFetchCount += 1;
      return route.fulfill({
        json: {
          type: "frame",
          frame: {
            frame_type: "table",
            x_field: null,
            y_field: null,
            series_field: null,
            unit: null,
            suggested_visualization: "table",
            field_roles: [],
            data: [
              {
                trace_id: `trace-${nlqFetchCount}`,
                root_service: "checkout",
                root_operation: `GET /checkout-v${nlqFetchCount}`,
                duration_ms: 12,
                status_code: "OK",
                environment: "prod",
                start_time_unix_nano: 1_700_000_000_000_000_000,
              },
            ],
            nlq_ir: {},
            source_sql: "",
            time_range: { from: "now-1h", to: "now" },
            signal_types: ["traces"],
            sample_rate: null,
            approximation_statement: "",
          },
        },
      });
    });

    await page.goto("/traces");
    await expect(page.getByText("GET /checkout-v1")).toBeVisible();
    await expect(page.getByTestId("subscription-status")).toHaveText("Live");
    expect(nlqFetchCount).toBe(1);

    const streamUrls = await page.evaluate(() => window.__sseTestHarness?.urls ?? []);
    expect(streamUrls.length).toBe(1);
    expect(decodeURIComponent(streamUrls[0]!)).toContain('"kind":"telemetry-watermark"');
    expect(decodeURIComponent(streamUrls[0]!)).toContain('"signal":"spans"');

    // Emit initial watermark snapshot (0), then a coalesced replace advancing watermark_unix_nano (PUSH-016).
    await page.evaluate(() => {
      window.__sseTestHarness!.emit("snapshot", {
        v: 1,
        viewId: "traces-watermark",
        queryRevision: "r1",
        revision: "1",
        data: {
          signal: "spans",
          service: null,
          watermark_unix_nano: 0,
          coalesced_records: 0,
        },
      });
      window.__sseTestHarness!.emit("replace", {
        v: 1,
        viewId: "traces-watermark",
        queryRevision: "r1",
        revision: "2",
        data: {
          signal: "spans",
          service: null,
          watermark_unix_nano: 1_700_000_000_005_000_000,
          coalesced_records: 250,
        },
      });
    });

    await expect(page.getByText("GET /checkout-v2")).toBeVisible();
    expect(nlqFetchCount).toBe(2);
  });
});
