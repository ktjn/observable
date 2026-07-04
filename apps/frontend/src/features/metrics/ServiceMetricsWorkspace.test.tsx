import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { TenantContextProvider } from "../../hooks/useTenantContext";
import { ServiceMetricsWorkspace } from "./ServiceMetricsWorkspace";

vi.mock("../../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: vi.fn(() => ({
    preset: "1h",
    fromMs: 1_700_096_400_000,
    toMs: 1_700_100_000_000,
    setPreset: vi.fn(),
    setCustomRange: vi.fn(),
    clearCustomRange: vi.fn(),
  })),
}));

// Deliberately ordered as the catalog would come back pre-fix: alphabetical,
// with the low-cardinality metric first. The workspace should still default
// to (auto-select) the entry with the most backing series.
const metrics = [
  {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    metric_name: "a.rarely_reported",
    description: "",
    unit: "1",
    metric_type: "sum",
    service_name: "checkout",
    environment: "prod",
    series_count: 1,
  },
  {
    tenant_id: "00000000-0000-0000-0000-000000000001",
    metric_name: "b.request_latency",
    description: "",
    unit: "ms",
    metric_type: "gauge",
    service_name: "checkout",
    environment: "prod",
    series_count: 42,
  },
];

vi.mock("../../api/metrics", () => ({
  listMetrics: vi.fn(async () => ({ metrics })),
  getMetricGroupPoints: vi.fn(async () => ({ points: [] })),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

function renderWorkspace() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <TenantContextProvider>
      <QueryClientProvider client={client}>
        <ServiceMetricsWorkspace />
      </QueryClientProvider>
    </TenantContextProvider>,
  );
}

test("auto-selects the first (highest series_count) metric instead of showing an empty state", async () => {
  renderWorkspace();

  await waitFor(() => expect(screen.getByText("b.request_latency")).toBeInTheDocument());
  expect(screen.queryByText("No metric selected")).not.toBeInTheDocument();
});
