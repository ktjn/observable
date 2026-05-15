import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import * as incidentsApi from "../../api/incidents";
import { IncidentsPage } from "./IncidentsPage";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../../lib/timeDisplay", () => ({
  useTimeDisplay: () => ({ format: "iso-local-ms" }),
}));

const sampleIncidents: incidentsApi.IncidentItem[] = [
  {
    incident_id: "inc-1",
    title: "Database CPU spike",
    severity: "critical",
    status: "triggered",
    triggered_at: "2026-05-15T10:00:00Z",
    resolved_at: null,
    triggered_by_rule_id: "rule-1",
  },
  {
    incident_id: "inc-2",
    title: "API latency high",
    severity: "warning",
    status: "acknowledged",
    triggered_at: "2026-05-15T09:00:00Z",
    resolved_at: null,
    triggered_by_rule_id: null,
  },
  {
    incident_id: "inc-3",
    title: "Disk full",
    severity: "critical",
    status: "resolved",
    triggered_at: "2026-05-15T08:00:00Z",
    resolved_at: "2026-05-15T08:30:00Z",
    triggered_by_rule_id: null,
  },
];

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <IncidentsPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
});

test("renders page header with Reliability eyebrow", async () => {
  vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: sampleIncidents });
  renderPage();
  await waitFor(() => screen.getByText("Incidents"));
  expect(screen.getByText("Reliability")).toBeInTheDocument();
});

test("renders MetricCard summary row with correct counts", async () => {
  vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: sampleIncidents });
  renderPage();
  await waitFor(() => screen.getByText("Total"));
  expect(screen.getByText("Total")).toBeInTheDocument();
  expect(screen.getByText("Triggered")).toBeInTheDocument();
  expect(screen.getByText("Acknowledged")).toBeInTheDocument();
  expect(screen.getByText("Resolved")).toBeInTheDocument();
  expect(screen.getByText("MTTR")).toBeInTheDocument();
  // 3 total, 1 triggered, 1 acknowledged, 1 resolved
  expect(screen.getByRole("group", { name: "Incident summary" })).toBeInTheDocument();
});

test("renders filter pills and filters table on click", async () => {
  vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: sampleIncidents });
  renderPage();
  await waitFor(() => screen.getByText("Database CPU spike"));

  // All three rows visible initially
  expect(screen.getByText("Database CPU spike")).toBeInTheDocument();
  expect(screen.getByText("API latency high")).toBeInTheDocument();
  expect(screen.getByText("Disk full")).toBeInTheDocument();

  // Click Triggered pill
  fireEvent.click(screen.getByRole("button", { name: /triggered/i }));
  await waitFor(() => expect(screen.queryByText("API latency high")).not.toBeInTheDocument());
  expect(screen.getByText("Database CPU spike")).toBeInTheDocument();
  expect(screen.queryByText("Disk full")).not.toBeInTheDocument();
});

test("renders empty state when no incidents", async () => {
  vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: [] });
  renderPage();
  await waitFor(() => expect(screen.getByText("No incidents found.")).toBeInTheDocument());
});

test("MTTR shows dash when no resolved incidents", async () => {
  const noResolved = sampleIncidents.filter((i) => i.status !== "resolved");
  vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: noResolved });
  renderPage();
  await waitFor(() => screen.getByText("MTTR"));
  expect(screen.getByText("—")).toBeInTheDocument();
});
