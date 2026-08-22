import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import type * as dashboardsApi from "../api/dashboards";
import DashboardsPage from "./DashboardsPage";

vi.mock("../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

const { mockNavigate } = vi.hoisted(() => ({
  mockNavigate: vi.fn(),
}));

vi.mock("@tanstack/react-router", () => ({
  useRouter: () => ({ navigate: mockNavigate }),
  Link: ({
    children,
    to,
    params,
    ...props
  }: React.AnchorHTMLAttributes<HTMLAnchorElement> & {
    to?: string;
    params?: Record<string, string>;
    children?: React.ReactNode;
  }) => (
    <a href={to ? to.replace(/\$(\w+)/g, (_, key: string) => params?.[key] ?? "") : "#"} {...props}>
      {children}
    </a>
  ),
}));

const listMock = vi.fn();
const deleteMock = vi.fn();
const createMock = vi.fn();
const exportMock = vi.fn();
const importMock = vi.fn();

vi.mock("../hooks/useRuntime", () => ({
  useRuntime: () => ({
    mode: "http",
    dashboards: {
      list: listMock,
      delete: deleteMock,
      create: createMock,
      export: exportMock,
      import: importMock,
    },
  }),
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
  listMock.mockReset();
  deleteMock.mockReset();
  createMock.mockReset();
  exportMock.mockReset();
  importMock.mockReset();
});

test("renders dashboard list when data loads", async () => {
  listMock.mockResolvedValue({ items: [sampleDashboard] });

  renderPage();

  await waitFor(() => expect(screen.getByText("My Dashboard")).toBeInTheDocument());
  expect(screen.getByText("1 panel")).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Open" })).toBeInTheDocument();
});

test("renders empty state when no dashboards", async () => {
  listMock.mockResolvedValue({ items: [] });

  renderPage();

  await waitFor(() => expect(screen.getByText("No dashboards yet")).toBeInTheDocument());
});

test("Export button calls exportDashboard and creates a download blob", async () => {
  listMock.mockResolvedValue({ items: [sampleDashboard] });
  exportMock.mockResolvedValue({
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

  await waitFor(() => expect(exportMock).toHaveBeenCalledWith("test-tenant", "dash-1"));
  expect(createObjectURLSpy).toHaveBeenCalledWith(expect.any(Blob));
});

test("Import button opens file picker and calls importDashboard on valid JSON", async () => {
  listMock.mockResolvedValue({ items: [] });
  importMock.mockResolvedValue(sampleDashboard);

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

  await waitFor(() => expect(importMock).toHaveBeenCalledWith("test-tenant", exportPayload));
});

test("Import shows error message on failure", async () => {
  listMock.mockResolvedValue({ items: [] });
  importMock.mockRejectedValue(new Error("Dashboard import failed: 422"));

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
  listMock.mockResolvedValue({ items: [] });

  renderPage();

  await waitFor(() => screen.getByText("No dashboards yet"));
  expect(screen.getByRole("button", { name: "New dashboard" })).toBeInTheDocument();
});

test('clicking "New dashboard" shows inline name input', async () => {
  listMock.mockResolvedValue({ items: [] });

  renderPage();

  await waitFor(() => screen.getByText("No dashboards yet"));
  fireEvent.click(screen.getByRole("button", { name: "New dashboard" }));

  expect(screen.getByRole("textbox", { name: "New dashboard name" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Create" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
});

test("submitting with a name calls createDashboard", async () => {
  listMock.mockResolvedValue({ items: [] });
  createMock.mockResolvedValue({
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
    expect(createMock).toHaveBeenCalledWith("test-tenant", { name: "Alpha", panels: [] }),
  );
  await waitFor(() =>
    expect(mockNavigate).toHaveBeenCalledWith({ to: "/dashboards/dash-new" }),
  );
});

test("dashboard card shows created_at date", async () => {
  listMock.mockResolvedValue({ items: [sampleDashboard] });

  renderPage();

  await waitFor(() => screen.getByText("My Dashboard"));
  const expectedDate = new Date("2026-05-05T00:00:00Z").toLocaleDateString();
  expect(screen.getByText(`Created ${expectedDate}`)).toBeInTheDocument();
});

test("dashboard card shows visibility badge", async () => {
  listMock.mockResolvedValue({ items: [sampleDashboard] });

  renderPage();

  await waitFor(() => screen.getByText("My Dashboard"));
  expect(screen.getByText("private")).toBeInTheDocument();
});
