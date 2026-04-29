import { render, screen } from "@testing-library/react";
import { afterEach } from "vitest";
import { Badge, HealthDot } from "./badge";

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

test("HealthDot renders with role img and aria-label for healthy", () => {
  render(<HealthDot state="healthy" />);
  expect(screen.getByRole("img", { name: "healthy" })).toBeInTheDocument();
});

test("HealthDot renders for watch state", () => {
  render(<HealthDot state="watch" />);
  expect(screen.getByRole("img", { name: "watch" })).toBeInTheDocument();
});

test("HealthDot renders for breach state", () => {
  render(<HealthDot state="breach" />);
  expect(screen.getByRole("img", { name: "breach" })).toBeInTheDocument();
});

test("HealthDot renders for unknown state", () => {
  render(<HealthDot state="unknown" />);
  expect(screen.getByRole("img", { name: "unknown" })).toBeInTheDocument();
});
