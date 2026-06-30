import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import ChangeEventsPage from "./ChangeEventsPage";
import * as changeEventsApi from "../../api/changeEvents";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "tenant-1" }),
}));
vi.mock("../../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: () => ({ fromMs: 0, toMs: 1000 }),
}));
vi.mock("../../lib/timeDisplay", () => ({
  useTimeDisplay: () => ({ format: "iso-local-ms" }),
}));

function renderPage() {
  const client = new QueryClient();
  return render(
    <QueryClientProvider client={client}>
      <ChangeEventsPage />
    </QueryClientProvider>,
  );
}

describe("ChangeEventsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renders an empty state when no events are returned", async () => {
    vi.spyOn(changeEventsApi, "listChangeEvents").mockResolvedValue({ items: [] });
    renderPage();
    await waitFor(() => expect(screen.getByText("No change events found")).toBeInTheDocument());
  });

  it("renders the change events summary stat-card row", async () => {
    vi.spyOn(changeEventsApi, "listChangeEvents").mockResolvedValue({
      items: [
        {
          change_event_id: "ce-2",
          tenant_id: "tenant-1",
          project_id: null,
          event_type: "deploy" as import("../../api/changeEvents").ChangeEventType,
          service_name: "api",
          environment: "production",
          title: "Deploy v1.2.3",
          description: null,
          occurred_at: new Date(500).toISOString(),
          source: "ci",
          created_by: "bot",
          metadata: null,
        },
      ],
    });
    renderPage();
    const summary = await screen.findByLabelText("Change events summary");
    expect(summary).toBeInTheDocument();
  });

  it("renders a row for each returned event", async () => {
    vi.spyOn(changeEventsApi, "listChangeEvents").mockResolvedValue({
      items: [
        {
          change_event_id: "ce-1",
          tenant_id: "tenant-1",
          project_id: null,
          event_type: "incident",
          service_name: "checkout",
          environment: "production",
          title: "Payment gateway flapping",
          description: null,
          occurred_at: new Date(500).toISOString(),
          source: "manual",
          created_by: "oncall",
          metadata: null,
        },
      ],
    });
    renderPage();
    await waitFor(() => expect(screen.getByText("Payment gateway flapping")).toBeInTheDocument());
    expect(screen.getByText("checkout")).toBeInTheDocument();
  });
});
