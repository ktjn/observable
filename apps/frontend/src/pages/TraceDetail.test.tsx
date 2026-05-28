import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi } from "vitest";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import { TenantContextProvider } from "../hooks/useTenantContext";
import { TraceDetail } from "./TraceDetail";
import * as logsApi from "../api/logs";

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    Link: ({
      children,
      to,
      ...props
    }: React.AnchorHTMLAttributes<HTMLAnchorElement> & { to?: string; children?: React.ReactNode }) => (
      <a href={to} {...props}>{children}</a>
    ),
  };
});

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false } },
});

function wrapper({ children }: { children: React.ReactNode }) {
  return (
    <QueryClientProvider client={queryClient}>
      <TenantContextProvider>
        <TimeDisplayProvider>{children}</TimeDisplayProvider>
      </TenantContextProvider>
    </QueryClientProvider>
  );
}

const baseSpan = {
  trace_id: "abc",
  tenant_id: "t1",
  span_id: "111",
  service_name: "checkout",
  service_namespace: "production",
  service_version: "1.0.0",
  operation_name: "POST /order",
  span_kind: "INTERNAL",
  start_time_unix_nano: 0,
  end_time_unix_nano: 5000000,
  duration_ns: 5_000_000,
  status_code: "OK",
  status_message: "",
  environment: "prod",
  host_id: "host-1",
  workload: "checkout-service",
  deployment_id: "deploy-123",
};

beforeEach(() => {
  vi.spyOn(logsApi, "searchLogs").mockResolvedValue({ logs: [], total: 0, facets: {} });
});

test("renders waterfall with spans", () => {
  render(
    <QueryClientProvider client={queryClient}>
      <TenantContextProvider>
        <TimeDisplayProvider>
          <TraceDetail traceId="abc" spans={[baseSpan]} />
        </TimeDisplayProvider>
      </TenantContextProvider>
    </QueryClientProvider>
  );
  expect(screen.getByText(/POST \/order/)).toBeInTheDocument();
  expect(screen.getAllByText("5.00ms").length).toBeGreaterThanOrEqual(1);
});

test("renders infra pill links when spans have resource_attributes", () => {
  const spans = [
    {
      ...baseSpan,
      resource_attributes: {
        "k8s.pod.name": "checkout-pod-1",
        "host.name": "node-3",
      },
    },
    {
      ...baseSpan,
      span_id: "222",
      resource_attributes: {
        "k8s.pod.name": "checkout-pod-1", // duplicate — should deduplicate
      },
    },
  ];

  render(<TraceDetail traceId="abc" spans={spans} />, { wrapper });

  const podLink = screen.getByRole("link", { name: "pod: checkout-pod-1" });
  expect(podLink).toBeInTheDocument();
  expect(podLink).toHaveAttribute("href", "/infrastructure/pod/checkout-pod-1");

  const hostLink = screen.getByRole("link", { name: "host: node-3" });
  expect(hostLink).toBeInTheDocument();
  expect(hostLink).toHaveAttribute("href", "/infrastructure/host/node-3");

  // Deduplicated — only one pod link
  expect(screen.getAllByRole("link", { name: "pod: checkout-pod-1" })).toHaveLength(1);
});

test("omits infra section entirely when no span has resource_attributes", () => {
  render(
    <QueryClientProvider client={queryClient}>
      <TenantContextProvider>
        <TimeDisplayProvider>
          <TraceDetail traceId="abc" spans={[baseSpan]} />
        </TimeDisplayProvider>
      </TenantContextProvider>
    </QueryClientProvider>
  );
  expect(screen.queryByText(/Infrastructure/)).not.toBeInTheDocument();
});

test("omits infra section when resource_attributes has no recognised infra keys", () => {
  const spans = [
    {
      ...baseSpan,
      resource_attributes: { "custom.attr": "value" },
    },
  ];
  render(<TraceDetail traceId="abc" spans={spans} />, { wrapper });
  expect(screen.queryByText(/Infrastructure/)).not.toBeInTheDocument();
});

test("span context panel scrolls internally when content overflows", () => {
  const span = {
    ...baseSpan,
    attributes: Object.fromEntries(
      Array.from({ length: 50 }, (_, i) => [`attr.${i}`, `value-${i}`]),
    ),
  };
  render(<TraceDetail traceId="abc" spans={[span]} />, { wrapper });

  fireEvent.click(screen.getByRole("button", { name: /checkout: POST \/order/ }));

  const panel = screen.getByRole("complementary", { name: "Selected span context" });
  expect(panel).toHaveClass("overflow-y-auto");
});

test("renders page-header with Traces eyebrow and truncated trace ID", () => {
  render(<TraceDetail traceId="abcdef1234567890xyz" spans={[baseSpan]} />, { wrapper });
  expect(screen.getByText("Traces")).toBeInTheDocument();
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("abcdef1234567890…");
});

test("renders Back to traces link", () => {
  render(<TraceDetail traceId="abc" spans={[baseSpan]} />, { wrapper });
  const link = screen.getByRole("link", { name: "Back to traces" });
  expect(link).toBeInTheDocument();
  expect(link).toHaveAttribute("href", "/traces");
});

test("renders MetricCard row with span count, duration, services, and errors", () => {
  const errorSpan = { ...baseSpan, span_id: "222", status_code: "ERROR" };
  render(<TraceDetail traceId="abc" spans={[baseSpan, errorSpan]} />, { wrapper });
  expect(screen.getByText("Total Spans")).toBeInTheDocument();
  expect(screen.getByText("Duration")).toBeInTheDocument();
  expect(screen.getByText("Services")).toBeInTheDocument();
  expect(screen.getByText("Errors")).toBeInTheDocument();
});

test("Errors MetricCard is present when there are error spans", () => {
  const errorSpan = { ...baseSpan, span_id: "222", status_code: "ERROR" };
  render(<TraceDetail traceId="abc" spans={[baseSpan, errorSpan]} />, { wrapper });
  expect(screen.getByText("Errors")).toBeInTheDocument();
});

test("renders service color legend with unique service names", () => {
  const paymentSpan = {
    ...baseSpan,
    span_id: "222",
    service_name: "payment",
  };
  render(<TraceDetail traceId="abc" spans={[baseSpan, paymentSpan]} />, { wrapper });
  const legend = screen.getByRole("generic", { name: "Service color legend" });
  expect(legend).toBeInTheDocument();
  expect(legend).toHaveTextContent("checkout");
  expect(legend).toHaveTextContent("payment");
});

test("service color legend deduplicates services", () => {
  const span2 = { ...baseSpan, span_id: "222" };
  render(<TraceDetail traceId="abc" spans={[baseSpan, span2]} />, { wrapper });
  const legend = screen.getByRole("generic", { name: "Service color legend" });
  expect(legend.querySelectorAll("span[aria-hidden]")).toHaveLength(1);
});

test("waterfall is wrapped in a Panel with Spans heading", () => {
  render(<TraceDetail traceId="abc" spans={[baseSpan]} />, { wrapper });
  expect(screen.getByText("Spans")).toBeInTheDocument();
  expect(screen.getByText("Waterfall")).toBeInTheDocument();
});

test("correlated logs panel shows Trace-correlated logs title when no span selected", () => {
  render(<TraceDetail traceId="abc" spans={[baseSpan]} />, { wrapper });
  expect(screen.getByText("Trace-correlated logs")).toBeInTheDocument();
  expect(screen.getByText("Correlation")).toBeInTheDocument();
});

test("span context panel is not inside the horizontal scroll container", () => {
  render(<TraceDetail traceId="abc" spans={[baseSpan]} />, { wrapper });
  fireEvent.click(screen.getByRole("button", { name: /checkout: POST \/order/ }));

  const panel = screen.getByRole("complementary", { name: "Selected span context" });
  // If the panel is inside overflow-x-auto, opening it widens the scroll area
  // and the panel overflows to the right instead of pushing the waterfall left.
  expect(panel.closest(".overflow-x-auto")).toBeNull();
});
