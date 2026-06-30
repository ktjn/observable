import { render, screen, act } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { DataFreshness } from "./data-freshness";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

test("renders 'Updated Xs ago' for seconds-old data", () => {
  const now = 1_700_000_000_000;
  vi.setSystemTime(now);
  render(<DataFreshness dataUpdatedAt={now - 12_000} />);
  expect(screen.getByText("Updated 12s ago")).toBeInTheDocument();
});

test("renders 'Updated Xm ago' for minutes-old data", () => {
  const now = 1_700_000_000_000;
  vi.setSystemTime(now);
  render(<DataFreshness dataUpdatedAt={now - 2 * 60_000} />);
  expect(screen.getByText("Updated 2m ago")).toBeInTheDocument();
});

test("renders 'Updated Xh ago' for hours-old data", () => {
  const now = 1_700_000_000_000;
  vi.setSystemTime(now);
  render(<DataFreshness dataUpdatedAt={now - 2 * 3_600_000} />);
  expect(screen.getByText("Updated 2h ago")).toBeInTheDocument();
});

test("renders nothing when dataUpdatedAt is 0", () => {
  vi.setSystemTime(1_700_000_000_000);
  const { container } = render(<DataFreshness dataUpdatedAt={0} />);
  expect(container.firstChild).toBeNull();
});

test("interval updates the display after 30s", () => {
  const now = 1_700_000_000_000;
  vi.setSystemTime(now);
  render(<DataFreshness dataUpdatedAt={now - 10_000} />);
  expect(screen.getByText("Updated 10s ago")).toBeInTheDocument();

  // Advance the fake clock by 30s — this fires the interval and Date.now() becomes now+30_000
  act(() => {
    vi.advanceTimersByTime(30_000);
  });

  expect(screen.getByText("Updated 40s ago")).toBeInTheDocument();
});
