import { render, screen, within } from "@testing-library/react";

let App: typeof import("../App").default;

const TENANT_ID = "00000000-0000-0000-0000-000000000002";

function buildUsageReportResponse(overrides?: Partial<Record<string, unknown>>) {
  return {
    tenant_id: TENANT_ID,
    from: "2026-05-22T08:00:00.000Z",
    to: "2026-05-22T14:00:00.000Z",
    telemetry_summary: {
      spans: 124,
      logs: 88,
      metric_points: 302,
      metric_series_created: 12,
    },
    control_plane_summary: {
      query_reads: 17,
      query_rows: 944,
      credential_checks: 9,
      credential_allows: 7,
      credential_denies: 2,
    },
    estimated_cost_index: 2434,
    ...overrides,
  };
}

beforeEach(async () => {
  window.localStorage.clear();
  window.history.pushState({}, "", "/admin");
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
            tenants: [{ tenant_id: TENANT_ID, role: "tenant_admin" }],
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/tenants/") && url.includes("/environments")) {
        return new Response(JSON.stringify({ environments: [{ environment: "prod" }] }), {
          status: 200,
        });
      }

      if (url.includes("/v1/tenants/usage-report")) {
        return new Response(JSON.stringify(buildUsageReportResponse()), { status: 200 });
      }

      if (url.includes("/v1/tenants")) {
        return new Response(
          JSON.stringify({
            tenants: [{ id: TENANT_ID, name: "observable" }],
          }),
          { status: 200 },
        );
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  ({ default: App } = await import("../App"));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

test("renders the tenant usage report for the admin overview", async () => {
  render(<App />);

  await screen.findByRole("heading", { name: "Admin Console" });
  expect(screen.getByText(/Selected environment:/)).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Identity settings" })).toBeInTheDocument();
  expect(screen.getByText(TENANT_ID)).toBeInTheDocument();
  expect(within(screen.getByRole("table")).getByText("Tenant admin")).toBeInTheDocument();

  const usageSummary = screen.getByRole("group", { name: "Usage summary" });
  expect(within(usageSummary).getByText("Cost index")).toBeInTheDocument();
  expect(within(usageSummary).getByText("2434")).toBeInTheDocument();

  const telemetry = screen.getByRole("group", { name: "Telemetry volume" });
  expect(within(telemetry).getByText("Spans")).toBeInTheDocument();
  expect(within(telemetry).getByText("124")).toBeInTheDocument();
  expect(within(telemetry).getByText("Logs")).toBeInTheDocument();
  expect(within(telemetry).getByText("88")).toBeInTheDocument();
  expect(within(telemetry).getByText("Metric points")).toBeInTheDocument();
  expect(within(telemetry).getByText("302")).toBeInTheDocument();

  const controlPlane = screen.getByRole("group", { name: "Control-plane activity" });
  expect(within(controlPlane).getByText("Query reads")).toBeInTheDocument();
  expect(within(controlPlane).getByText("17")).toBeInTheDocument();
  expect(within(controlPlane).getByText("Credential denials")).toBeInTheDocument();
  expect(within(controlPlane).getByText("2")).toBeInTheDocument();

  expect(screen.getByText(/1 environment available in the selected tenant/)).toBeInTheDocument();
});

test("renders zero usage without an empty state", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/auth/me")) {
        return new Response(
          JSON.stringify({
            user_id: "user-1",
            email: "alice@example.com",
            tenants: [{ tenant_id: TENANT_ID, role: "tenant_admin" }],
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/tenants/") && url.includes("/environments")) {
        return new Response(JSON.stringify({ environments: [{ environment: "prod" }] }), {
          status: 200,
        });
      }

      if (url.includes("/v1/tenants/usage-report")) {
        return new Response(
          JSON.stringify(
            buildUsageReportResponse({
              telemetry_summary: {
                spans: 0,
                logs: 0,
                metric_points: 0,
                metric_series_created: 0,
              },
              control_plane_summary: {
                query_reads: 0,
                query_rows: 0,
                credential_checks: 0,
                credential_allows: 0,
                credential_denies: 0,
              },
              estimated_cost_index: 0,
            }),
          ),
          { status: 200 },
        );
      }

      if (url.includes("/v1/tenants")) {
        return new Response(
          JSON.stringify({
            tenants: [{ id: TENANT_ID, name: "observable" }],
          }),
          { status: 200 },
        );
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  ({ default: App } = await import("../App"));

  render(<App />);

  await screen.findByRole("heading", { name: "Admin Console" });
  const usageSummary = screen.getByRole("group", { name: "Usage summary" });
  expect(within(usageSummary).getByText("Cost index")).toBeInTheDocument();
  expect(within(usageSummary).getAllByText("0")).toHaveLength(4);

  const telemetry = screen.getByRole("group", { name: "Telemetry volume" });
  expect(within(telemetry).getAllByText("0")).toHaveLength(4);

  const controlPlane = screen.getByRole("group", { name: "Control-plane activity" });
  expect(within(controlPlane).getAllByText("0")).toHaveLength(5);
});

test("renders the tenant configuration page at /admin/config", async () => {
  window.history.pushState({}, "", "/admin/config");

  render(<App />);

  await screen.findByRole("heading", { name: "Tenant configuration" });
  expect(screen.getByRole("link", { name: "Identity settings" })).toBeInTheDocument();
  expect(screen.getByText("Tenant admin", { selector: "span" })).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Quota posture" })).toBeInTheDocument();
  expect(screen.getByText(/Usage window:/)).toBeInTheDocument();
  expect(screen.getByText("prod", { selector: "span" })).toBeInTheDocument();
});

test("renders the fleet management contract page at /admin/fleet", async () => {
  window.history.pushState({}, "", "/admin/fleet");

  render(<App />);

  await screen.findByRole("heading", { name: "Fleet management" });
  expect(screen.getByText("Contract view")).toBeInTheDocument();
  expect(screen.getByText("agent.up", { selector: "td" })).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Remote configuration and upgrades" })).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Live agent inventory is not wired yet" })).toBeInTheDocument();
});
