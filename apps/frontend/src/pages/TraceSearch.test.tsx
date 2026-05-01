import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, expect, test, vi } from "vitest";
import TraceSearch from "./TraceSearch";

const traceResponse = {
  traces: [
    {
      trace_id: "trace-abc-1234567890",
      events: [],
      spans: [
        {
          tenant_id: "00000000-0000-0000-0000-000000000001",
          trace_id: "trace-abc-1234567890",
          span_id: "span-root",
          service_name: "checkout",
          service_namespace: "shop",
          service_version: "2026.04.30",
          operation_name: "GET /checkout",
          span_kind: "SERVER",
          start_time_unix_nano: 1,
          end_time_unix_nano: 5000001,
          duration_ns: 5000000,
          status_code: "OK",
          status_message: "",
          attributes: {},
          resource_attributes: {},
          environment: "prod",
          host_id: "host-1",
          workload: "checkout-api",
          deployment_id: "deploy-1",
        },
      ],
    },
  ],
  total: 1,
  facets: {
    service_name: [{ value: "checkout", count: 1 }],
    status_code: [{ value: "OK", count: 1 }],
  },
};

vi.mock("../api/traces", () => ({
  searchTraces: vi.fn(async () => traceResponse),
}));

vi.mock("../api/dashboards", () => ({
  createDashboard: vi.fn(async () => ({
    dashboard_id: "dash-1",
    name: "Promoted trace query",
    panels: [],
    created_at: "2026-04-30T00:00:00Z",
  })),
}));

vi.mock("@tanstack/react-router", () => ({
  Link: ({
    to,
    params,
    children,
  }: {
    to: string;
    params?: Record<string, string>;
    children: ReactNode;
  }) => {
    const href = params?.traceId
      ? to.replace("$traceId", params.traceId)
      : to;
    return <a href={href}>{children}</a>;
  },
}));

const { searchTraces } = await import("../api/traces");

beforeEach(() => {
  vi.clearAllMocks();
  window.history.pushState({}, "", "/traces?service=checkout");
});

function renderTraceSearch() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return render(
    <QueryClientProvider client={client}>
      <TraceSearch />
    </QueryClientProvider>,
  );
}

test("renders the trace explorer shell with facets and named results table", async () => {
  const { container } = renderTraceSearch();

  await waitFor(() =>
    expect(searchTraces).toHaveBeenCalledWith(
      expect.objectContaining({
        service: "checkout",
        lookback_minutes: 60,
        facets: ["service_name", "status_code", "span_kind"],
      }),
    ),
  );

  expect(container.querySelector(".trace-explorer-page")).toBeInTheDocument();

  const facets = await screen.findByRole("complementary", {
    name: "Trace facets",
  });
  expect(within(facets).getByText("service name")).toBeInTheDocument();
  expect(within(facets).getByRole("button", { name: "checkout 1" })).toBeInTheDocument();

  const table = screen.getByRole("table", { name: "Trace results" });
  expect(within(table).getByRole("columnheader", { name: "Trace ID" })).toBeInTheDocument();
  expect(within(table).getByRole("columnheader", { name: "Duration" })).toBeInTheDocument();
  expect(within(table).getByText("GET /checkout")).toBeInTheDocument();
  expect(within(table).getByRole("link", { name: "trace-abc-123456…" })).toHaveAttribute(
    "href",
    "/traces/trace-abc-1234567890",
  );
});
