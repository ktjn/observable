import { render, screen } from "@testing-library/react";
import { DurationCell, ErrorRateCell, LatencyCell } from "./metric-cells";

// ErrorRateCell
test("ErrorRateCell renders red for ≥5%", () => {
  render(<ErrorRateCell value={0.05} />);
  const el = screen.getByText("5.00%");
  expect(el.style.color).toBe("var(--bad)");
});

test("ErrorRateCell renders warn color for ≥1%", () => {
  render(<ErrorRateCell value={0.02} />);
  const el = screen.getByText("2.00%");
  expect(el.style.color).toBe("var(--warn)");
});

test("ErrorRateCell renders good color for <1%", () => {
  render(<ErrorRateCell value={0.005} />);
  const el = screen.getByText("0.50%");
  expect(el.style.color).toBe("var(--good)");
});

// LatencyCell
test("LatencyCell renders red for ≥500ms", () => {
  render(<LatencyCell valueMs={600} />);
  const el = screen.getByText("600ms");
  expect(el.style.color).toBe("var(--bad)");
});

test("LatencyCell renders warn color for ≥100ms", () => {
  render(<LatencyCell valueMs={200} />);
  const el = screen.getByText("200ms");
  expect(el.style.color).toBe("var(--warn)");
});

test("LatencyCell renders good color for <100ms", () => {
  render(<LatencyCell valueMs={50} />);
  const el = screen.getByText("50ms");
  expect(el.style.color).toBe("var(--good)");
});

// DurationCell
test("DurationCell renders red for ≥500ms (in nanoseconds input)", () => {
  render(<DurationCell durationNs={600_000_000} />);
  const el = screen.getByText("600.00ms");
  expect(el.style.color).toBe("var(--bad)");
});

test("DurationCell renders good color for fast span", () => {
  render(<DurationCell durationNs={5_000_000} />);
  const el = screen.getByText("5.00ms");
  expect(el.style.color).toBe("var(--good)");
});
