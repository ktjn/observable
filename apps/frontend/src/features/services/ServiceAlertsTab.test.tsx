import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, describe, it, expect, beforeEach } from "vitest";
import { ServiceAlertsTab } from "./ServiceAlertsTab";
import * as alertsApi from "../../api/alerts";
import * as incidentsApi from "../../api/incidents";

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
    Link: ({
      children,
      ...props
    }: React.AnchorHTMLAttributes<HTMLAnchorElement> & {
      children?: React.ReactNode;
    }) => <a {...props}>{children}</a>,
  };
});

const firingRule: alertsApi.AlertRuleItem = {
  rule_id: "rule-1",
  name: "High CPU",
  metric_name: "cpu_usage",
  operator: "gt",
  threshold: 90,
  severity: "critical",
  silenced: false,
  state: "active",
  firing: true,
  last_fired_at: "2026-05-15T10:00:00Z",
  notification_channels: [],
  auto_trigger_incident: false,
};

const okRule: alertsApi.AlertRuleItem = {
  rule_id: "rule-2",
  name: "Low Memory",
  metric_name: "memory_free",
  operator: "lt",
  threshold: 10,
  severity: "warning",
  silenced: false,
  state: "ok",
  firing: false,
  last_fired_at: null,
  notification_channels: [],
  auto_trigger_incident: false,
};

const openIncident: incidentsApi.IncidentItem = {
  incident_id: "inc-1",
  title: "Database overload",
  severity: "critical",
  status: "triggered",
  triggered_at: "2026-05-15T09:00:00Z",
  triggered_by_rule_id: "rule-1",
};

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("ServiceAlertsTab", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("shows section headings", async () => {
    vi.spyOn(alertsApi, "listAlertRules").mockResolvedValue({ items: [] });
    vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: [] });

    render(<ServiceAlertsTab />, { wrapper });

    await waitFor(() => screen.getByText("Firing Alert Rules"));
    expect(screen.getByText("Open Incidents")).toBeInTheDocument();
  });

  it("shows only firing rules, not OK rules", async () => {
    vi.spyOn(alertsApi, "listAlertRules").mockResolvedValue({
      items: [firingRule, okRule],
    });
    vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: [] });

    render(<ServiceAlertsTab />, { wrapper });

    await waitFor(() => screen.getByText("High CPU"));
    expect(screen.queryByText("Low Memory")).not.toBeInTheDocument();
  });

  it("shows open incidents with link to detail", async () => {
    vi.spyOn(alertsApi, "listAlertRules").mockResolvedValue({ items: [] });
    vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({
      items: [openIncident],
    });

    render(<ServiceAlertsTab />, { wrapper });

    await waitFor(() => screen.getByText("Database overload"));
    expect(screen.getByText("Database overload")).toBeInTheDocument();
  });

  it("shows no-firing and no-incidents placeholders when empty", async () => {
    vi.spyOn(alertsApi, "listAlertRules").mockResolvedValue({
      items: [okRule],
    });
    vi.spyOn(incidentsApi, "listIncidents").mockResolvedValue({ items: [] });

    render(<ServiceAlertsTab />, { wrapper });

    await waitFor(() => screen.getByText("No firing rules."));
    expect(screen.getByText("No open incidents.")).toBeInTheDocument();
  });
});
