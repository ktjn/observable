import { render, screen, fireEvent } from "@testing-library/react";
import { vi, expect, test } from "vitest";
import { SignalExplorer } from "./SignalExplorer";
import type { SignalExplorerProps } from "./SignalExplorer";

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
  render(<SignalExplorer {...makeProps()} />);
  expect(screen.getByRole("heading", { name: "Logs" })).toBeInTheDocument();
});

test("renders the histogram slot", () => {
  render(<SignalExplorer {...makeProps()} />);
  expect(screen.getByTestId("histogram")).toBeInTheDocument();
});

test("panel is hidden initially — renderTable receives null selectedId", () => {
  render(<SignalExplorer {...makeProps()} />);
  expect(screen.getByTestId("table")).toHaveTextContent("none selected");
  expect(screen.queryByTestId("panel")).not.toBeInTheDocument();
});

test("clicking a row opens the panel with that id", () => {
  render(<SignalExplorer {...makeProps()} />);
  fireEvent.click(screen.getByTestId("table"));
  expect(screen.getByTestId("panel")).toHaveAttribute("data-selected", "row-1");
  expect(screen.getByTestId("table")).toHaveTextContent("row-1");
});

test("clicking the same row again closes the panel", () => {
  render(<SignalExplorer {...makeProps()} />);
  fireEvent.click(screen.getByTestId("table")); // open
  fireEvent.click(screen.getByTestId("table")); // close (same id)
  expect(screen.queryByTestId("panel")).not.toBeInTheDocument();
});

test("panel close button clears selection", () => {
  render(<SignalExplorer {...makeProps()} />);
  fireEvent.click(screen.getByTestId("table")); // open
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(screen.queryByTestId("panel")).not.toBeInTheDocument();
});

test("panel container has w-1/4 class when open", () => {
  render(<SignalExplorer {...makeProps()} />);
  fireEvent.click(screen.getByTestId("table")); // open
  const panelContainer = screen.getByTestId("panel").parentElement!;
  expect(panelContainer.className).toMatch(/w-1\/4/);
});

test("service input calls onServiceChange on change", () => {
  const onServiceChange = vi.fn();
  render(<SignalExplorer {...makeProps({ onServiceChange })} />);
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "checkout" } });
  expect(onServiceChange).toHaveBeenCalledWith("checkout");
});
