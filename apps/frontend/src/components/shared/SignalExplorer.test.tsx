import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { vi, expect, test, afterEach, beforeEach } from "vitest";
import { SignalExplorer } from "./SignalExplorer";
import type { SignalExplorerProps } from "./SignalExplorer";
import { TenantContextProvider } from "../../hooks/useTenantContext";

vi.mock("../../api/nlq", () => ({
  submitNlqQuery: vi.fn(),
}));

vi.mock("../../api/setup", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../api/setup")>();
  return {
    ...actual,
    getConfig: vi.fn(),
  };
});

vi.mock("../../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: () => ({ fromMs: Date.now() - 3600_000, toMs: Date.now() }),
}));

import { submitNlqQuery } from "../../api/nlq";
import { getConfig } from "../../api/setup";
const mockSubmit = vi.mocked(submitNlqQuery);
const mockGetConfig = vi.mocked(getConfig);

beforeEach(() => {
  mockGetConfig.mockResolvedValue({
    llm_key_configured: true,
    llm_url: null,
    llm_model: null,
    llm_provider: "remote",
    webllm_model: null,
  });
});

afterEach(() => {
  vi.clearAllMocks();
});

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <TenantContextProvider>
      <QueryClientProvider client={qc}>{children}</QueryClientProvider>
    </TenantContextProvider>
  );
}

function makeProps(overrides: Partial<SignalExplorerProps> = {}): SignalExplorerProps {
  return {
    title: "Logs",
    service: "",
    onServiceChange: vi.fn(),
    showHeader: true,
    showPromote: false,
    saveStatus: "idle",
    onPromote: vi.fn(),
    histogram: <div data-testid="histogram" />,
    renderTable: (selectedId, onSelect) => (
      <button data-testid="table" onClick={() => onSelect("row-1")}>
        {selectedId ?? "none selected"}
      </button>
    ),
    renderPanel: (selectedId, onClose) => (
      <div data-testid="panel" data-selected={selectedId}>
        <button onClick={onClose}>Close</button>
      </div>
    ),
    ...overrides,
  };
}

test("renders the title in the page header", () => {
  render(<SignalExplorer {...makeProps()} />, { wrapper });
  expect(screen.getByRole("heading", { name: "Logs" })).toBeInTheDocument();
});

test("renders the histogram slot", () => {
  render(<SignalExplorer {...makeProps()} />, { wrapper });
  expect(screen.getByTestId("histogram")).toBeInTheDocument();
});

test("panel is hidden initially — renderTable receives null selectedId", () => {
  render(<SignalExplorer {...makeProps()} />, { wrapper });
  expect(screen.getByTestId("table")).toHaveTextContent("none selected");
  expect(screen.queryByTestId("panel")).not.toBeInTheDocument();
});

test("clicking a row opens the panel with that id", () => {
  render(<SignalExplorer {...makeProps()} />, { wrapper });
  fireEvent.click(screen.getByTestId("table"));
  expect(screen.getByTestId("panel")).toHaveAttribute("data-selected", "row-1");
  expect(screen.getByTestId("table")).toHaveTextContent("row-1");
});

test("clicking the same row again closes the panel", () => {
  render(<SignalExplorer {...makeProps()} />, { wrapper });
  fireEvent.click(screen.getByTestId("table")); // open
  fireEvent.click(screen.getByTestId("table")); // close (same id)
  expect(screen.queryByTestId("panel")).not.toBeInTheDocument();
});

test("panel close button clears selection", () => {
  render(<SignalExplorer {...makeProps()} />, { wrapper });
  fireEvent.click(screen.getByTestId("table")); // open
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(screen.queryByTestId("panel")).not.toBeInTheDocument();
});

test("panel container has w-1/4 class when open", () => {
  render(<SignalExplorer {...makeProps()} />, { wrapper });
  fireEvent.click(screen.getByTestId("table")); // open
  const panelContainer = screen.getByTestId("panel").parentElement!;
  expect(panelContainer.className).toMatch(/w-1\/4/);
});

test("query input calls onQuerySubmit with the raw text", async () => {
  const onQuerySubmit = vi.fn();
  const baseIr = { operation: "table", signals: ["logs"], filters: [], time_range: { from: "now-1h", to: "now" } };
  mockSubmit.mockResolvedValue({
    type: "ir",
    ir: {
      operation: "table",
      signals: ["logs"],
      filters: [{ field: "service_name", op: "=", value: "checkout" }],
      group_by: [],
      time_range: { from: "now-1h", to: "now" },
      metric: null,
      window: null,
      resolution: null,
      visualization_hint: null,
    },
  });

  render(<SignalExplorer {...makeProps({ onQuerySubmit, baseIr })} />, { wrapper });
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "checkout" } });
  fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));

  await waitFor(() => expect(onQuerySubmit).toHaveBeenCalledWith("checkout"));
});
