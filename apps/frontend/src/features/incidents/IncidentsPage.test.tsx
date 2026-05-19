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

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    Link: ({ children, ...props }: React.AnchorHTMLAttributes<HTMLAnchorElement> & { children?: React.ReactNode }) =>
      <a {...props}>{children}</a>,
  };
});

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
  await waitFor(() => screen.getByRole("heading", { level: 1, name: "Incidents" }));
  expect(screen.getByText("Reliability")).toBeInTheDocument();
});

test("renders MetricCard summary row with correct counts", async () => {
  vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: sampleIncidents });
  renderPage();
  // Wait for the data to load by checking for a metric value or the table content
  await waitFor(() => expect(screen.getByText("Database CPU spike")).toBeInTheDocument());
  expect(screen.getByText("Total")).toBeInTheDocument();
  // Use getAllByText for labels that appear in multiple places (pill + metric)
  const triggeredLabels = screen.getAllByText("Triggered");
  expect(triggeredLabels.length).toBeGreaterThanOrEqual(1);
  const acknowledgedLabels = screen.getAllByText("Acknowledged");
  expect(acknowledgedLabels.length).toBeGreaterThanOrEqual(1);
  const resolvedLabels = screen.getAllByText("Resolved");
  expect(resolvedLabels.length).toBeGreaterThanOrEqual(1);
  expect(screen.getByText("MTTR")).toBeInTheDocument();
  // 3 total, 1 triggered, 1 acknowledged, 1 resolved
  expect(screen.getByRole("group", { name: "Incident summary" })).toBeInTheDocument();
  // Verify numeric values are rendered (Total=3, each status=1, MTTR=30m)
  expect(screen.getByText("3")).toBeInTheDocument();
  expect(screen.getAllByText("1").length).toBeGreaterThanOrEqual(3);
  expect(screen.getByText("30m")).toBeInTheDocument();
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
  expect(screen.getAllByText("—").length).toBeGreaterThanOrEqual(1);
});
