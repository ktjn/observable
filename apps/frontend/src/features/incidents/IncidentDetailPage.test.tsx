import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import * as incidentsApi from "../../api/incidents";
import { IncidentDetailPage } from "./IncidentDetailPage";

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
    useParams: () => ({ incidentId: "inc-1" }),
    Link: ({
      children,
      to,
      params,
      ...props
    }: {
      children?: React.ReactNode;
      to: string;
      params?: Record<string, string>;
    } & React.AnchorHTMLAttributes<HTMLAnchorElement>) => (
      <a href={params ? `${to}/${Object.values(params)[0]}` : to} {...props}>
        {children}
      </a>
    ),
  };
});

const baseDetail: incidentsApi.IncidentDetailResponse = {
  incident_id: "inc-1",
  title: "CPU spike",
  severity: "critical",
  status: "triggered",
  dedup_key: "rule-abc",
  triggered_at: "2026-05-18T10:00:00Z",
  resolved_at: null,
  triggered_by_rule_id: "rule-abc",
  runbook_url: null,
  rule_name: "High CPU Alert",
  impacted_service: null,
  timeline: [
    {
      event_time: "2026-05-18T10:00:01Z",
      event_type: "triggered",
      actor: "system",
      message: "Alert rule transitioned to active",
    },
    {
      event_time: "2026-05-18T10:00:05Z",
      event_type: "alert_fired",
      actor: "system",
      message: "High CPU Alert fired: value=95.30",
    },
  ],
};

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <IncidentDetailPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
});

test("renders timeline events with humanized labels", async () => {
  vi.spyOn(incidentsApi, "getIncident").mockResolvedValue(baseDetail);
  renderPage();
  await waitFor(() => screen.getByRole("heading", { level: 1, name: "CPU spike" }));
  expect(screen.getByText("triggered")).toBeInTheDocument();
  expect(screen.getByText("alert fired")).toBeInTheDocument();
});

test("renders view rule link on alert_fired when triggered_by_rule_id is set", async () => {
  vi.spyOn(incidentsApi, "getIncident").mockResolvedValue(baseDetail);
  renderPage();
  await waitFor(() => screen.getByText("→ View rule"));
  expect(screen.getByText("→ View rule")).toBeInTheDocument();
});

test("does not render view rule link when triggered_by_rule_id is null", async () => {
  vi.spyOn(incidentsApi, "getIncident").mockResolvedValue({
    ...baseDetail,
    triggered_by_rule_id: null,
  });
  renderPage();
  await waitFor(() => screen.getByText("alert fired"));
  expect(screen.queryByText("→ View rule")).not.toBeInTheDocument();
});

test("renders runbook_url as link when present", async () => {
  vi.spyOn(incidentsApi, "getIncident").mockResolvedValue({
    ...baseDetail,
    runbook_url: "https://runbooks.example.com/cpu-high",
  });
  renderPage();
  await waitFor(() => screen.getByText("https://runbooks.example.com/cpu-high"));
  const link = screen.getByRole("link", { name: "https://runbooks.example.com/cpu-high" });
  expect(link).toHaveAttribute("href", "https://runbooks.example.com/cpu-high");
});

test("does not render Runbook section when runbook_url is null", async () => {
  vi.spyOn(incidentsApi, "getIncident").mockResolvedValue(baseDetail);
  renderPage();
  await waitFor(() => screen.getByRole("heading", { level: 1 }));
  expect(screen.queryByText("Runbook")).not.toBeInTheDocument();
});
