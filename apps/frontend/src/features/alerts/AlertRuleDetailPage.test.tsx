import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import * as alertsApi from "../../api/alerts";
import { AlertRuleDetailPage } from "./AlertRuleDetailPage";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../../lib/timeDisplay", () => ({
  useTimeDisplay: () => ({ format: "iso-local-ms" }),
}));

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    useParams: () => ({ ruleId: "rule-1" }),
  };
});

const sampleRule: alertsApi.AlertRuleDetailResponse = {
  rule_id: "rule-1",
  name: "High CPU Alert",
  severity: "critical",
  alert_type: "threshold",
  condition: { metric_name: "cpu_usage", operator: "gt", threshold: 90 },
  silenced: false,
  firing: true,
  firings: [
    {
      firing_id: "f-1",
      state: "active",
      value: 95.3,
      occurred_at: "2026-05-18T10:00:05Z",
      resolved_at: null,
    },
    {
      firing_id: "f-2",
      state: "resolved",
      value: 91.0,
      occurred_at: "2026-05-18T09:00:00Z",
      resolved_at: "2026-05-18T09:30:00Z",
    },
  ],
  runbook_url: null,
};

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <AlertRuleDetailPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
});

test("renders rule name and severity", async () => {
  vi.spyOn(alertsApi, "getAlertRule").mockResolvedValue(sampleRule);
  renderPage();
  await waitFor(() => screen.getByRole("heading", { level: 1, name: "High CPU Alert" }));
  expect(screen.getByText("critical")).toBeInTheDocument();
  expect(screen.getByText("threshold")).toBeInTheDocument();
});

test("renders condition summary for threshold rule", async () => {
  vi.spyOn(alertsApi, "getAlertRule").mockResolvedValue(sampleRule);
  renderPage();
  await waitFor(() => screen.getByText("cpu_usage > 90"));
});

test("renders firings table with correct row count", async () => {
  vi.spyOn(alertsApi, "getAlertRule").mockResolvedValue(sampleRule);
  renderPage();
  await waitFor(() => screen.getByRole("table", { name: "Firing history" }));
  // 1 header row + 2 data rows
  expect(screen.getAllByRole("row")).toHaveLength(3);
});

test("renders empty state when no firings", async () => {
  vi.spyOn(alertsApi, "getAlertRule").mockResolvedValue({ ...sampleRule, firings: [] });
  renderPage();
  await waitFor(() => screen.getByText("No firings recorded."));
});
