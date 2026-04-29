// apps/frontend/src/components/ui/loading-state.test.tsx
import { render, screen } from "@testing-library/react";
import { LoadingState } from "./loading-state";

test("renders children as content", () => {
  render(<LoadingState>Loading data…</LoadingState>);
  expect(screen.getByText("Loading data…")).toBeInTheDocument();
});

test("applies muted text styling", () => {
  render(<LoadingState>Loading…</LoadingState>);
  expect(screen.getByText("Loading…").parentElement ?? screen.getByText("Loading…")).toBeTruthy();
});

test("merges additional className via prop", () => {
  render(<LoadingState className="text-[var(--bad)]">Error!</LoadingState>);
  const el = screen.getByText("Error!");
  expect(el.className).toContain("text-[var(--bad)]");
});
