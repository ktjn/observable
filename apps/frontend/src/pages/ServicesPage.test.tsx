import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import * as servicesApi from "../api/services";
import ServicesPage from "./ServicesPage";

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
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
  DEFAULT_PRESET: "1h",
  PRESET_OPTIONS: [{ value: "1h", label: "Last 1 hour" }],
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
  health_state: "breach",
  active_alert_count: 2,
  latest_deployment: "v2.3.1",
};

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <ServicesPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
});

test("renders active alert count and latest deploy columns", async () => {
  vi.spyOn(servicesApi, "listServiceSummaries").mockResolvedValue({ items: [sampleSummary] });

  renderPage();

  await waitFor(() => expect(screen.getByText("checkout")).toBeInTheDocument());
  const row = screen.getByText("checkout").closest("tr");
  expect(row).not.toBeNull();
  expect(row).toHaveTextContent("2");
  expect(row).toHaveTextContent("v2.3.1");
  expect(row).toHaveTextContent("Breach");
});
