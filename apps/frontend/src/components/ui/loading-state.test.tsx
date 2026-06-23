import { render, screen } from "@testing-library/react";
import { LoadingState } from "./loading-state";

test("renders children as content", () => {
  render(<LoadingState>Loading data…</LoadingState>);
  expect(screen.getByText("Loading data…")).toBeInTheDocument();
});

test("renders skeleton with animate-pulse class", () => {
  const { container } = render(<LoadingState variant="skeleton" className="h-[168px]" />);
  const div = container.firstChild as HTMLElement;
  expect(div.className).toContain("animate-pulse");
  expect(div.getAttribute("aria-hidden")).toBe("true");
});

test("renders spinner variant with spinning element", () => {
  const { container } = render(<LoadingState variant="spinner">Connecting…</LoadingState>);
  expect(screen.getByText("Connecting…")).toBeInTheDocument();
  const spinner = container.querySelector(".animate-spin");
  expect(spinner).toBeInTheDocument();
});

test("merges additional className via prop", () => {
  render(<LoadingState className="text-[var(--bad)]">Error!</LoadingState>);
  const el = screen.getByText("Error!");
  expect(el.className).toContain("text-[var(--bad)]");
});
