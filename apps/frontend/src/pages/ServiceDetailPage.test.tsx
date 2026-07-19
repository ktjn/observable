import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import * as servicesApi from "../api/services";
import { TimeDisplayProvider } from "../lib/timeDisplay";
import ServiceDetailPage from "./ServiceDetailPage";

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    useParams: () => ({ serviceId: "checkout" }),
    useLocation: () => ({ pathname: "/services/checkout" }),
    Link: ({
      children,
      to,
      ...props
    }: React.AnchorHTMLAttributes<HTMLAnchorElement> & { to?: string; children?: React.ReactNode }) => (
      <a href={to ?? "#"} {...props}>
        {children}
      </a>
    ),
  };
});

vi.mock("../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: () => ({
    preset: "1h",
    fromMs: 0,
    toMs: 3_600_000,
    setPreset: vi.fn(),
    setCustomRange: vi.fn(),
    clearCustomRange: vi.fn(),
  }),
}));

const sampleSummary: servicesApi.ServiceSummary = {
  service_name: "checkout",
  request_rate: 12.5,
  error_rate: 0.02,
  p95_latency_ms: 245,
  health_state: "healthy",
  active_alert_count: 0,
  latest_deployment: "v2.3.1",
};

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <TimeDisplayProvider>
        <ServiceDetailPage />
      </TimeDisplayProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
});

test("shows the Infrastructure tab and no Signal Entry Points or Ask panel", async () => {
  vi.spyOn(servicesApi, "getServiceSummary").mockResolvedValue({ service: sampleSummary });
  vi.spyOn(servicesApi, "getServiceResponseTimeHistory").mockResolvedValue({ buckets: [] });

  renderPage();

  await waitFor(() => expect(screen.getByRole("heading", { name: "checkout" })).toBeInTheDocument());
  expect(screen.getByRole("link", { name: "Infrastructure" })).toBeInTheDocument();
  expect(screen.queryByText("Signal Entry Points")).not.toBeInTheDocument();
  expect(screen.queryByText("Natural Language Query")).not.toBeInTheDocument();
  expect(screen.getByRole("link", { name: /Ask in Workbench/i })).toBeInTheDocument();
});
