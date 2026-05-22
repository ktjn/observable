import { render, screen, within } from "@testing-library/react";

let App: typeof import("../../App").default;

const TENANT_ID = "00000000-0000-0000-0000-000000000001";

function buildReportResponse(overrides?: Partial<Record<string, unknown>>) {
  return {
    service_name: "checkout",
    environment: "prod",
    from: "2026-05-22T08:00:00.000Z",
    to: "2026-05-22T14:00:00.000Z",
    incident_summary: {
      total: 2,
      open: 1,
      resolved: 1,
      mean_time_to_resolve_minutes: 60,
    },
    slo_summary: {
      total: 1,
      firing: 1,
    },
    deployment_summary: {
      total: 1,
    },
    incidents: [
      {
        incident_id: "incident-1",
        title: "Checkout prod resolved",
        severity: "critical",
        status: "resolved",
        triggered_at: "2026-05-22T09:00:00.000Z",
        resolved_at: "2026-05-22T10:00:00.000Z",
        triggered_by_rule_id: null,
      },
      {
        incident_id: "incident-2",
        title: "Checkout prod open",
        severity: "warning",
        status: "triggered",
        triggered_at: "2026-05-22T11:00:00.000Z",
        resolved_at: null,
        triggered_by_rule_id: null,
      },
    ],
    slos: [
      {
        slo_id: "slo-1",
        service_name: "checkout",
        environment: "prod",
        sli_type: "availability",
        target: 0.99,
        window_days: 30,
        burn_rate_fast_threshold: 14.4,
        burn_rate_slow_threshold: 1,
        description: "Checkout prod SLO",
        firing: true,
        last_fired_at: "2026-05-22T12:00:00.000Z",
        created_at: "2026-05-21T00:00:00.000Z",
        updated_at: "2026-05-22T12:00:00.000Z",
      },
    ],
    deployments: [
      {
        deployment_id: "deploy-1",
        tenant_id: TENANT_ID,
        project_id: null,
        service_name: "checkout",
        environment: "prod",
        service_version: "2026.05.22",
        status: "success",
        started_at: "2026-05-22T12:00:00.000Z",
        finished_at: "2026-05-22T13:00:00.000Z",
        deployed_by: "ci-bot",
        commit_sha: "abc12345",
        rollback_of: null,
        metadata: null,
      },
    ],
    ...overrides,
  };
}

beforeEach(async () => {
  window.localStorage.clear();
  window.history.pushState({}, "", "/services/checkout/reliability");
  vi.resetModules();
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/auth/me")) {
        return new Response(
          JSON.stringify({
            user_id: "user-1",
            email: "alice@example.com",
            tenants: [{ tenant_id: TENANT_ID, role: "member" }],
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/tenants/") && url.includes("/environments")) {
        return new Response(JSON.stringify({ items: [{ environment: "prod" }] }), {
          status: 200,
        });
      }

      if (url.includes("/v1/tenants")) {
        return new Response(
          JSON.stringify({
            tenants: [{ id: TENANT_ID, name: "observable" }],
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/services/checkout/summary")) {
        return new Response(
          JSON.stringify({
            service: {
              service_name: "checkout",
              request_rate: 12.5,
              error_rate: 0.025,
              p95_latency_ms: 245,
              health_state: "watch",
              active_alert_count: 2,
              latest_deployment: "checkout@2026.05.22",
            },
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/services/checkout/reliability-report")) {
        return new Response(JSON.stringify(buildReportResponse()), { status: 200 });
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  ({ default: App } = await import("../../App"));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

test("renders the reliability report tab with service-scoped summaries", async () => {
  render(<App />);

  expect(await screen.findByRole("heading", { name: "checkout" })).toBeInTheDocument();
  await screen.findByText("Checkout prod resolved");
  const summary = screen.getByRole("group", { name: "Reliability summary" });
  expect(within(summary).getByText("Incidents")).toBeInTheDocument();
  expect(within(summary).getByText("Open Incidents")).toBeInTheDocument();
  expect(within(summary).getByText("Firing SLOs")).toBeInTheDocument();
  expect(within(summary).getByText("Deployments")).toBeInTheDocument();
  expect(screen.getByText("Checkout prod resolved")).toBeInTheDocument();
  expect(screen.getByText("Checkout prod SLO")).toBeInTheDocument();
  expect(screen.getByText("2026.05.22")).toBeInTheDocument();

  const tabs = screen.getByRole("navigation", { name: "Service signals" });
  expect(within(tabs).getByRole("link", { name: "Reliability" })).toHaveAttribute(
    "href",
    "/services/checkout/reliability",
  );
});

test("renders an empty reliability state when the report has no data", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/auth/me")) {
        return new Response(
          JSON.stringify({
            user_id: "user-1",
            email: "alice@example.com",
            tenants: [{ tenant_id: TENANT_ID, role: "member" }],
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/tenants/") && url.includes("/environments")) {
        return new Response(JSON.stringify({ items: [{ environment: "prod" }] }), {
          status: 200,
        });
      }

      if (url.includes("/v1/tenants")) {
        return new Response(
          JSON.stringify({
            tenants: [{ id: TENANT_ID, name: "observable" }],
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/services/checkout/summary")) {
        return new Response(
          JSON.stringify({
            service: {
              service_name: "checkout",
              request_rate: 12.5,
              error_rate: 0.025,
              p95_latency_ms: 245,
              health_state: "watch",
              active_alert_count: 2,
              latest_deployment: "checkout@2026.05.22",
            },
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/services/checkout/reliability-report")) {
        return new Response(
          JSON.stringify(
            buildReportResponse({
              incident_summary: { total: 0, open: 0, resolved: 0, mean_time_to_resolve_minutes: null },
              slo_summary: { total: 0, firing: 0 },
              deployment_summary: { total: 0 },
              incidents: [],
              slos: [],
              deployments: [],
            }),
          ),
          { status: 200 },
        );
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  ({ default: App } = await import("../../App"));

  render(<App />);

  expect(await screen.findByText("No reliability data yet.")).toBeInTheDocument();
});
