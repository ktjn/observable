import { render, screen } from "@testing-library/react";
import { MetricCard } from "./metric-card";

test("renders metric label and value", () => {
  render(<MetricCard label="Avg P95" value="184ms" tone="good" />);
  expect(screen.getByText("Avg P95")).toBeInTheDocument();
  expect(screen.getByText("184ms")).toBeInTheDocument();
});
