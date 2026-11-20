import { render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { VirtualTable } from "./VirtualTable";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getVirtualItems: () =>
      Array.from({ length: count }, (_, i) => ({
        key: i,
        index: i,
        start: i * 32,
        end: (i + 1) * 32,
      })),
    getTotalSize: () => count * 32,
    measureElement: (_el: Element | null) => {},
  }),
}));

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

test("renders rows reported by the virtualizer", () => {
  const rows = Array.from({ length: 120 }, (_, i) => `row-${i}`);
  renderTable(rows);
  expect(screen.getByText("row-0")).toBeInTheDocument();
  expect(screen.getByText("row-119")).toBeInTheDocument();
});
