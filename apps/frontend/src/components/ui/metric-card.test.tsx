import { render, screen } from "@testing-library/react";
import { MetricCard } from "./metric-card";

test("renders metric label and value", () => {
  render(<MetricCard label="Avg P95" value="184ms" tone="good" />);
  expect(screen.getByText("Avg P95")).toBeInTheDocument();
  expect(screen.getByText("184ms")).toBeInTheDocument();
});

// --- sparkline tests ---

test("renders sparkline SVG when sparkline prop has ≥2 points", () => {
  const { container } = render(
    <MetricCard label="P95" value="100ms" sparkline={[10, 20, 30]} />,
  );
  const svg = container.querySelector("svg[aria-hidden]");
  expect(svg).toBeInTheDocument();
});

test("does NOT render sparkline SVG when sparkline prop has <2 points", () => {
  const { container } = render(
    <MetricCard label="P95" value="100ms" sparkline={[10]} />,
  );
  const svg = container.querySelector("svg[aria-hidden]");
  expect(svg).not.toBeInTheDocument();
});

test("does NOT render sparkline SVG when sparkline prop is empty", () => {
  const { container } = render(
    <MetricCard label="P95" value="100ms" sparkline={[]} />,
  );
  const svg = container.querySelector("svg[aria-hidden]");
  expect(svg).not.toBeInTheDocument();
});

test("sparkline SVG is aria-hidden", () => {
  const { container } = render(
    <MetricCard label="P95" value="100ms" sparkline={[5, 10, 8, 12]} />,
  );
  const svg = container.querySelector("svg");
  expect(svg).toHaveAttribute("aria-hidden", "true");
});

// --- delta tests ---

test("renders delta as '+X.X%' with correct text when positive", () => {
  render(<MetricCard label="P95" value="100ms" delta={0.05} />);
  expect(screen.getByText("+5.0%")).toBeInTheDocument();
  expect(screen.getByText("vs prev window")).toBeInTheDocument();
});

test("renders delta as '-X.X%' when negative", () => {
  render(<MetricCard label="P95" value="100ms" delta={-0.03} />);
  expect(screen.getByText("-3.0%")).toBeInTheDocument();
  expect(screen.getByText("vs prev window")).toBeInTheDocument();
});

test("renders 'No change' when delta is 0", () => {
  render(<MetricCard label="P95" value="100ms" delta={0} />);
  expect(screen.getByText("No change")).toBeInTheDocument();
  expect(screen.queryByText("vs prev window")).not.toBeInTheDocument();
});

test("delta span has descriptive aria-label for positive delta", () => {
  render(<MetricCard label="P95" value="100ms" delta={0.05} />);
  const deltaEl = screen.getByLabelText(
    "increased 5.0 percent vs previous window",
  );
  expect(deltaEl).toBeInTheDocument();
});

test("delta span has descriptive aria-label for negative delta", () => {
  render(<MetricCard label="P95" value="100ms" delta={-0.03} />);
  const deltaEl = screen.getByLabelText(
    "decreased 3.0 percent vs previous window",
  );
  expect(deltaEl).toBeInTheDocument();
});

test("does not render delta section when delta prop is undefined", () => {
  render(<MetricCard label="P95" value="100ms" />);
  expect(screen.queryByText("vs prev window")).not.toBeInTheDocument();
  expect(screen.queryByText("No change")).not.toBeInTheDocument();
});
