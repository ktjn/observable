import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import { TenantContextProvider } from "../hooks/useTenantContext";
import { TraceDetail } from "./TraceDetail";

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
  expect(screen.getByText("5.00ms")).toBeInTheDocument();
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
