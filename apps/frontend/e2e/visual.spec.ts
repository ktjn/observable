import { test } from "@playwright/test";

const MOCK_USER = {
  user_id: "00000000-0000-0000-0000-000000000001",
  email: "test@example.com",
  tenants: [{ tenant_id: "00000000-0000-0000-0000-000000000001", role: "admin" }],
};

async function mockAuth(page: import("@playwright/test").Page) {
  await page.route("**/v1/auth/me", (route) => route.fulfill({ json: MOCK_USER }));
  await page.route("**/v1/tenants", (route) =>
    route.fulfill({ json: { tenants: [{ id: "00000000-0000-0000-0000-000000000001", name: "observable" }] } })
  );
  await page.route("**/v1/tenants/**/environments", (route) =>
    route.fulfill({ json: { environments: ["prod"] } })
  );
}

const T_NS = 1_700_000_000_000_000_000;

// ── Traces ────────────────────────────────────────────────────────────────────

test("visual: traces page", async ({ page }) => {
  await mockAuth(page);
  await page.route("**/v1/nlq", (route) =>
    route.fulfill({
      json: {
        type: "frame",
        frame: {
          data: [
            { trace_id: "aaa", root_service: "checkout", root_operation: "GET /order/aaa", duration_ms: 150, status_code: "ERROR", environment: "prod", start_time_unix_nano: T_NS },
            { trace_id: "bbb", root_service: "payments", root_operation: "POST /pay", duration_ms: 50, status_code: "OK", environment: "prod", start_time_unix_nano: T_NS },
            { trace_id: "ccc", root_service: "notifications", root_operation: "GET /notify", duration_ms: 3000, status_code: "OK", environment: "prod", start_time_unix_nano: T_NS },
          ],
        },
      },
    })
  );
  await page.route("**/v1/tenants/**/traces/histogram**", (route) =>
    route.fulfill({ json: { buckets: [{ start_ms: 1_700_000_000_000, end_ms: 1_700_000_060_000, count: 3 }] } })
  );
  await page.goto("/traces");
  await page.waitForSelector('[aria-label="Trace results"]');
  await page.screenshot({ path: "e2e/screenshots/traces.png", fullPage: true });
});

// ── Logs ──────────────────────────────────────────────────────────────────────

test("visual: logs page", async ({ page }) => {
  await mockAuth(page);
  await page.route("**/v1/nlq", (route) =>
    route.fulfill({
      json: {
        type: "frame",
        frame: {
          data: [
            { log_id: "l1", timestamp_unix_nano: T_NS, observed_timestamp_unix_nano: T_NS, severity_number: 17, body: "checkout failed", service_name: "checkout", environment: "prod", host_id: "h1", trace_id: null, span_id: null, fingerprint: null, attributes: {}, resource_attributes: {} },
            { log_id: "l2", timestamp_unix_nano: T_NS + 1e9, observed_timestamp_unix_nano: T_NS + 1e9, severity_number: 13, body: "slow query detected", service_name: "checkout", environment: "prod", host_id: "h1", trace_id: null, span_id: null, fingerprint: null, attributes: {}, resource_attributes: {} },
            { log_id: "l3", timestamp_unix_nano: T_NS + 2e9, observed_timestamp_unix_nano: T_NS + 2e9, severity_number: 9, body: "order processed", service_name: "checkout", environment: "prod", host_id: "h1", trace_id: "abc", span_id: "def", fingerprint: null, attributes: {}, resource_attributes: {} },
          ],
        },
      },
    })
  );
  await page.route("**/v1/tenants/**/logs/histogram**", (route) =>
    route.fulfill({ json: { buckets: [{ start_ms: 1_700_000_000_000, end_ms: 1_700_000_060_000, counts: { "17": 1, "13": 1, "9": 1 } }] } })
  );
  await page.goto("/logs");
  await page.waitForSelector('[aria-label="Log results"]');
  await page.screenshot({ path: "e2e/screenshots/logs.png", fullPage: true });
});

// ── Services ──────────────────────────────────────────────────────────────────

test("visual: services page", async ({ page }) => {
  await mockAuth(page);
  await page.route("**/v1/services/summary**", (route) =>
    route.fulfill({
      json: {
        items: [
          { service_name: "checkout", request_rate: 42.5, error_rate: 0.001, p95_latency_ms: 85, health_state: "healthy", active_alert_count: 0, latest_deployment: "v1.4.2" },
          { service_name: "payments", request_rate: 18.2, error_rate: 0.065, p95_latency_ms: 620, health_state: "breach", active_alert_count: 3, latest_deployment: null },
          { service_name: "notifications", request_rate: 5.1, error_rate: 0.012, p95_latency_ms: 210, health_state: "watch", active_alert_count: 1, latest_deployment: "v2.0.1" },
        ],
      },
    })
  );
  await page.route("**/v1/services", (route) =>
    route.fulfill({ json: { items: ["checkout", "payments", "notifications"] } })
  );
  await page.route("**/v1/topology**", (route) =>
    route.fulfill({ json: { edges: [{ caller: "checkout", callee: "payments", request_count: 1000, error_rate: 0.02, p95_latency_ms: 45 }] } })
  );
  await page.goto("/services");
  await page.waitForSelector("table[aria-label='Service catalog']");
  await page.screenshot({ path: "e2e/screenshots/services.png", fullPage: true });
});

// ── Infrastructure ────────────────────────────────────────────────────────────

test("visual: infrastructure page", async ({ page }) => {
  await mockAuth(page);
  await page.route("**/v1/nlq", (route) =>
    route.fulfill({
      json: {
        type: "frame",
        frame: {
          data: [
            { entity_type: "host", entity_id: "prod-host-1", display_name: "prod-host-1", parent_id: null, parent_display_name: null, environment: "prod", health_state: "healthy", last_seen_unix_nano: T_NS, related_services: ["checkout"], log_rate_per_minute: 4.2, error_rate: 0.001, restart_count: 0, cpu_usage: 0.45, memory_usage: 0.52, disk_usage: 0.30, network_io: 1024 },
            { entity_type: "pod", entity_id: "payments-pod-1", display_name: "payments-pod-1", parent_id: "payments", parent_display_name: "payments", environment: "prod", health_state: "breach", last_seen_unix_nano: T_NS, related_services: ["payments"], log_rate_per_minute: 18.0, error_rate: 0.05, restart_count: 3, cpu_usage: 0.87, memory_usage: 0.92, disk_usage: 0.78, network_io: null },
          ],
        },
      },
    })
  );
  await page.goto("/infrastructure");
  await page.waitForSelector("text=prod-host-1");
  await page.screenshot({ path: "e2e/screenshots/infrastructure.png", fullPage: true });
});

// ── Alerts ────────────────────────────────────────────────────────────────────

test("visual: alerts page", async ({ page }) => {
  await mockAuth(page);
  await page.route("**/v1/alerts/rules**", (route) =>
    route.fulfill({
      json: {
        items: [
          { rule_id: "r1", name: "High Error Rate", metric_name: "error_rate", operator: "gt", threshold: 0.05, severity: "critical", silenced: false, state: "active", firing: true, last_fired_at: "2026-05-15T10:00:00Z", notification_channels: [], auto_trigger_incident: true },
          { rule_id: "r2", name: "Low Request Rate", metric_name: "request_rate", operator: "lt", threshold: 1.0, severity: "warning", silenced: false, state: "ok", firing: false, last_fired_at: null, notification_channels: [], auto_trigger_incident: false },
          { rule_id: "r3", name: "Memory Pressure", metric_name: "memory_usage", operator: "gt", threshold: 0.9, severity: "info", silenced: true, state: "ok", firing: false, last_fired_at: null, notification_channels: [], auto_trigger_incident: false },
        ],
      },
    })
  );
  await page.route("**/v1/slos**", (route) =>
    route.fulfill({
      json: {
        items: [
          { slo_id: "slo-1", service_name: "checkout", environment: "prod", sli_type: "availability", target: 0.999, window_days: 30, burn_rate_fast_threshold: 14.4, burn_rate_slow_threshold: 1.0, description: "Checkout availability SLO", firing: false, last_fired_at: null, created_at: "2026-01-01T00:00:00Z", updated_at: "2026-01-01T00:00:00Z" },
        ],
      },
    })
  );
  await page.route("**/v1/notification-channels**", (route) => route.fulfill({ json: [] }));
  await page.goto("/alerts");
  await page.waitForSelector("table[aria-label='Alert rules']");
  await page.screenshot({ path: "e2e/screenshots/alerts.png", fullPage: true });
});

// ── Dashboards ────────────────────────────────────────────────────────────────

test("visual: dashboards page", async ({ page }) => {
  await mockAuth(page);
  await page.route("**/v1/dashboards", (route) =>
    route.fulfill({
      json: {
        items: [
          { dashboard_id: "dash-1", name: "Checkout Overview", panels: [{ panel_id: "p1", title: "Errors", panel_kind: "query", query_kind: "logs", service: "checkout", preset: "1h", filters: {}, query_text: "", content: null, layout: { x: 0, y: 0, w: 6, h: 4 }, time_range: { mode: "preset", preset: "1h" } }, { panel_id: "p2", title: "Latency", panel_kind: "query", query_kind: "traces", service: "checkout", preset: "1h", filters: {}, query_text: "", content: null, layout: { x: 6, y: 0, w: 6, h: 4 }, time_range: { mode: "preset", preset: "1h" } }], created_at: "2026-05-05T00:00:00Z" },
          { dashboard_id: "dash-2", name: "Payments Health", panels: [{ panel_id: "p3", title: "Rate", panel_kind: "query", query_kind: "metrics", service: "payments", preset: "1h", filters: {}, query_text: "", content: null, layout: { x: 0, y: 0, w: 6, h: 4 }, time_range: { mode: "preset", preset: "1h" } }], created_at: "2026-05-06T00:00:00Z" },
        ],
      },
    })
  );
  await page.goto("/dashboards");
  await page.waitForSelector("[aria-label='Dashboard cards']");
  await page.screenshot({ path: "e2e/screenshots/dashboards.png", fullPage: true });
});
