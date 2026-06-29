import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import * as dashboardsApi from "../api/dashboards";
import DashboardsPage from "./DashboardsPage";

vi.mock("../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

const { mockNavigate } = vi.hoisted(() => ({
  mockNavigate: vi.fn(),
}));

vi.mock("@tanstack/react-router", () => ({
  useRouter: () => ({ navigate: mockNavigate }),
}));

const sampleDashboard: dashboardsApi.Dashboard = {
  dashboard_id: "dash-1",
  name: "My Dashboard",
  visibility: "private",
  panels: [
    {
      panel_id: "panel-1",
      title: "Error Logs",
      panel_kind: "query",
      query_kind: "logs",
      service: "checkout",
      preset: "1h",
      filters: {},
      query_text: "errors in checkout",
      layout: { x: 0, y: 0, w: 6, h: 4 },
      time_range: { mode: "preset", preset: "1h" },
    },
  ],
  created_at: "2026-05-05T00:00:00Z",
};

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <DashboardsPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
  mockNavigate.mockClear();
});

test("renders dashboard list when data loads", async () => {
  vi.spyOn(dashboardsApi, "listDashboards").mockResolvedValue({ items: [sampleDashboard] });

  renderPage();

  await waitFor(() => expect(screen.getByText("My Dashboard")).toBeInTheDocument());
  expect(screen.getByText("1 panel")).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Open" })).toBeInTheDocument();
});

test("renders empty state when no dashboards", async () => {
  vi.spyOn(dashboardsApi, "listDashboards").mockResolvedValue({ items: [] });

  renderPage();

  await waitFor(() => expect(screen.getByText("No dashboards yet")).toBeInTheDocument());
});

test("Export button calls exportDashboard and creates a download blob", async () => {
  vi.spyOn(dashboardsApi, "listDashboards").mockResolvedValue({ items: [sampleDashboard] });
  const exportSpy = vi.spyOn(dashboardsApi, "exportDashboard").mockResolvedValue({
    schema_version: "1",
    name: "My Dashboard",
    panels: [{ title: "Error Logs", query_kind: "logs", service: "checkout", preset: "1h", filters: {} }],
  });
  const createObjectURLSpy = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:url");
  vi.spyOn(URL, "revokeObjectURL").mockReturnValue(undefined);
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);

  renderPage();

  await waitFor(() => screen.getByText("My Dashboard"));
  fireEvent.click(screen.getByRole("button", { name: "Export" }));

  await waitFor(() => expect(exportSpy).toHaveBeenCalledWith("test-tenant", "dash-1"));
  expect(createObjectURLSpy).toHaveBeenCalledWith(expect.any(Blob));
});

test("Import button opens file picker and calls importDashboard on valid JSON", async () => {
  vi.spyOn(dashboardsApi, "listDashboards").mockResolvedValue({ items: [] });
  const importSpy = vi.spyOn(dashboardsApi, "importDashboard").mockResolvedValue(sampleDashboard);

  renderPage();

  await waitFor(() => screen.getByText("No dashboards yet"));

  const exportPayload: dashboardsApi.DashboardExport = {
    schema_version: "1",
    name: "Imported",
    panels: [{ title: "Panel", query_kind: "traces", filters: {} }],
  };
  const file = new File([JSON.stringify(exportPayload)], "imported.dashboard.json", {
    type: "application/json",
  });

  const fileInput = screen.getByLabelText("Import dashboard JSON file") as HTMLInputElement;
  Object.defineProperty(fileInput, "files", { value: [file], configurable: true });
  fireEvent.change(fileInput);

  await waitFor(() => expect(importSpy).toHaveBeenCalledWith("test-tenant", exportPayload));
});

test("Import shows error message on failure", async () => {
  vi.spyOn(dashboardsApi, "listDashboards").mockResolvedValue({ items: [] });
  vi.spyOn(dashboardsApi, "importDashboard").mockRejectedValue(new Error("Dashboard import failed: 422"));

  renderPage();

  await waitFor(() => screen.getByText("No dashboards yet"));

  const badPayload = { schema_version: "99", name: "Bad", panels: [] };
  const file = new File([JSON.stringify(badPayload)], "bad.json", { type: "application/json" });

  const fileInput = screen.getByLabelText("Import dashboard JSON file") as HTMLInputElement;
  Object.defineProperty(fileInput, "files", { value: [file], configurable: true });
  fireEvent.change(fileInput);

  await waitFor(() =>
    expect(screen.getByRole("alert")).toHaveTextContent("Import failed: Dashboard import failed: 422"),
  );
});

// ── Slice 9: create-affordance, card metadata ─────────────────────────────────

test('"New dashboard" button appears in header', async () => {
  vi.spyOn(dashboardsApi, "listDashboards").mockResolvedValue({ items: [] });

  renderPage();

  await waitFor(() => screen.getByText("No dashboards yet"));
  expect(screen.getByRole("button", { name: "New dashboard" })).toBeInTheDocument();
});

test('clicking "New dashboard" shows inline name input', async () => {
  vi.spyOn(dashboardsApi, "listDashboards").mockResolvedValue({ items: [] });

  renderPage();

  await waitFor(() => screen.getByText("No dashboards yet"));
  fireEvent.click(screen.getByRole("button", { name: "New dashboard" }));

  expect(screen.getByRole("textbox", { name: "New dashboard name" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Create" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
});

test("submitting with a name calls createDashboard", async () => {
  vi.spyOn(dashboardsApi, "listDashboards").mockResolvedValue({ items: [] });
  const createSpy = vi.spyOn(dashboardsApi, "createDashboard").mockResolvedValue({
    ...sampleDashboard,
    dashboard_id: "dash-new",
    name: "Alpha",
  });

  renderPage();

  await waitFor(() => screen.getByText("No dashboards yet"));
  fireEvent.click(screen.getByRole("button", { name: "New dashboard" }));

  const input = screen.getByRole("textbox", { name: "New dashboard name" });
  fireEvent.change(input, { target: { value: "Alpha" } });
  fireEvent.click(screen.getByRole("button", { name: "Create" }));

  await waitFor(() =>
    expect(createSpy).toHaveBeenCalledWith("test-tenant", { name: "Alpha", panels: [] }),
  );
  await waitFor(() =>
    expect(mockNavigate).toHaveBeenCalledWith({ to: "/dashboards/dash-new" }),
  );
});

test("dashboard card shows created_at date", async () => {
  vi.spyOn(dashboardsApi, "listDashboards").mockResolvedValue({ items: [sampleDashboard] });

  renderPage();

  await waitFor(() => screen.getByText("My Dashboard"));
  const expectedDate = new Date("2026-05-05T00:00:00Z").toLocaleDateString();
  expect(screen.getByText(`Created ${expectedDate}`)).toBeInTheDocument();
});

test("dashboard card shows visibility badge", async () => {
  vi.spyOn(dashboardsApi, "listDashboards").mockResolvedValue({ items: [sampleDashboard] });

  renderPage();

  await waitFor(() => screen.getByText("My Dashboard"));
  expect(screen.getByText("private")).toBeInTheDocument();
});
