import { render, screen } from "@testing-library/react";
import { afterEach } from "vitest";
import { Badge } from "./badge";

afterEach(() => {
  delete document.documentElement.dataset.theme;
});

test("renders badge content", () => {
  render(<Badge tone="good">Healthy</Badge>);
  expect(screen.getByText("Healthy")).toBeInTheDocument();
});

test("marks status badges with a status role", () => {
  render(<Badge tone="bad">Breach</Badge>);
  expect(screen.getByRole("status")).toHaveTextContent("Breach");
});

test("renders under the dark theme contract", () => {
  document.documentElement.dataset.theme = "dark";
  render(<Badge tone="warn">Watch</Badge>);
  expect(screen.getByText("Watch")).toBeInTheDocument();
});
