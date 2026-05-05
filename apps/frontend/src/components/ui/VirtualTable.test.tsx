import { render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { VirtualTable } from "./VirtualTable";

function renderTable(rows: string[]) {
  return render(
    <VirtualTable
      rows={rows}
      renderHead={() => (
        <tr>
          <th>Item</th>
        </tr>
      )}
      renderRow={(row, _ref, index) => (
        <tr key={index} data-index={index}>
          <td>{row}</td>
        </tr>
      )}
      ariaLabel="Test table"
    />,
  );
}

test("renders table with aria-label and headers", () => {
  renderTable(["apple", "banana", "cherry"]);
  expect(screen.getByRole("table", { name: "Test table" })).toBeInTheDocument();
  expect(screen.getByRole("columnheader", { name: "Item" })).toBeInTheDocument();
});

test("renders an empty table when rows is empty", () => {
  renderTable([]);
  expect(screen.getByRole("table", { name: "Test table" })).toBeInTheDocument();
  expect(screen.queryByRole("cell")).not.toBeInTheDocument();
});

test("renders only the first page of rows on initial load", () => {
  const rows = Array.from({ length: 80 }, (_, i) => `row-${i}`);
  renderTable(rows);
  expect(screen.getByText("row-0")).toBeInTheDocument();
  expect(screen.getByText("row-49")).toBeInTheDocument();
  expect(screen.queryByText("row-50")).not.toBeInTheDocument();
});

test("reveals the next page when the sentinel enters the viewport", async () => {
  let observerCallback: IntersectionObserverCallback | null = null;
  window.IntersectionObserver = class {
    constructor(cb: IntersectionObserverCallback) {
      observerCallback = cb;
    }
    observe() {}
    unobserve() {}
    disconnect() {}
  } as unknown as typeof IntersectionObserver;

  const rows = Array.from({ length: 120 }, (_, i) => `row-${i}`);
  renderTable(rows);

  expect(screen.queryByText("row-50")).not.toBeInTheDocument();

  // Simulate sentinel entering viewport
  observerCallback!([{ isIntersecting: true } as IntersectionObserverEntry], {} as IntersectionObserver);

  await vi.waitFor(() => {
    expect(screen.getByText("row-50")).toBeInTheDocument();
    expect(screen.getByText("row-99")).toBeInTheDocument();
    expect(screen.queryByText("row-100")).not.toBeInTheDocument();
  });
});
