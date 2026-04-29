import { render, screen } from "@testing-library/react";
import { TablePanel } from "./table-panel";

test("renders children", () => {
  render(<TablePanel><p>content</p></TablePanel>);
  expect(screen.getByText("content")).toBeInTheDocument();
});

test("forwards aria-label to the wrapper div", () => {
  render(<TablePanel aria-label="Service traces"><table /></TablePanel>);
  expect(screen.getByRole("generic", { name: "Service traces" })).toBeInTheDocument();
});

test("merges className prop", () => {
  const { container } = render(<TablePanel className="flex-1"><p>x</p></TablePanel>);
  expect(container.firstChild).toHaveClass("flex-1");
});
