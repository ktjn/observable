import { fireEvent, render, screen } from "@testing-library/react";
import { PillFilter } from "./pill-filter";

const pills = [
  { key: "all", label: "All", count: 42, activeColor: "var(--brand)" },
  { key: "error", label: "Error", count: 5, activeColor: "var(--bad)" },
  { key: "ok", label: "OK", count: 37, activeColor: "var(--good)" },
];

test("renders all pills with labels and counts", () => {
  render(<PillFilter pills={pills} activeKey="all" onSelect={() => {}} />);

  expect(screen.getByText("All")).toBeInTheDocument();
  expect(screen.getByText("Error")).toBeInTheDocument();
  expect(screen.getByText("OK")).toBeInTheDocument();
  expect(screen.getByText("(42)")).toBeInTheDocument();
  expect(screen.getByText("(5)")).toBeInTheDocument();
  expect(screen.getByText("(37)")).toBeInTheDocument();
});

test("applies aria-label when provided", () => {
  render(<PillFilter pills={pills} activeKey="all" onSelect={() => {}} ariaLabel="Filter by status" />);
  expect(screen.getByLabelText("Filter by status")).toBeInTheDocument();
});

test("calls onSelect with pill key on click", async () => {
  const onSelect = vi.fn();
  render(<PillFilter pills={pills} activeKey="all" onSelect={onSelect} />);

  fireEvent.click(screen.getByText("Error"));
  expect(onSelect).toHaveBeenCalledWith("error");
});
