import { render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { VirtualTable } from "./VirtualTable";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getVirtualItems: () =>
      Array.from({ length: count }, (_, i) => ({
        key: i,
        index: i,
        start: i * 40,
        end: (i + 1) * 40,
      })),
    getTotalSize: () => count * 40,
    measureElement: (_el: Element | null) => {},
  }),
}));

test("renders table with aria-label, headers, and all rows", () => {
  const rows = ["apple", "banana", "cherry"];
  render(
    <VirtualTable
      rows={rows}
      renderHead={() => (
        <tr>
          <th>Fruit</th>
        </tr>
      )}
      renderRow={(row, ref, index) => (
        <tr key={index} ref={ref} data-index={index}>
          <td>{row}</td>
        </tr>
      )}
      ariaLabel="Fruit table"
    />,
  );

  expect(screen.getByRole("table", { name: "Fruit table" })).toBeInTheDocument();
  expect(screen.getByRole("columnheader", { name: "Fruit" })).toBeInTheDocument();
  expect(screen.getByText("apple")).toBeInTheDocument();
  expect(screen.getByText("banana")).toBeInTheDocument();
  expect(screen.getByText("cherry")).toBeInTheDocument();
});

test("renders an empty table when rows is empty", () => {
  render(
    <VirtualTable
      rows={[]}
      renderHead={() => (
        <tr>
          <th>Fruit</th>
        </tr>
      )}
      renderRow={(row, ref, index) => (
        <tr key={index} ref={ref} data-index={index}>
          <td>{String(row)}</td>
        </tr>
      )}
      ariaLabel="Empty table"
    />,
  );

  expect(screen.getByRole("table", { name: "Empty table" })).toBeInTheDocument();
  expect(screen.queryByRole("cell")).not.toBeInTheDocument();
});
