import { render, screen, fireEvent } from "@testing-library/react";
import { vi, expect, test, beforeEach } from "vitest";

const mockSetPreset = vi.fn();
const mockSetCustomRange = vi.fn();
const mockClearCustomRange = vi.fn();

vi.mock("../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: vi.fn(() => ({
    preset: "1h",
    fromMs: 1000,
    toMs: 4600000,
    setPreset: mockSetPreset,
    setCustomRange: mockSetCustomRange,
    clearCustomRange: mockClearCustomRange,
  })),
  PRESET_OPTIONS: [
    { value: "5m",  label: "Last 5 min" },
    { value: "15m", label: "Last 15 min" },
    { value: "30m", label: "Last 30 min" },
    { value: "1h",  label: "Last 1 hour" },
    { value: "3h",  label: "Last 3 hours" },
    { value: "12h", label: "Last 12 hours" },
  ],
}));

import { GlobalDateRangePicker } from "./GlobalDateRangePicker";
import { useGlobalDateRange } from "../hooks/useGlobalDateRange";

beforeEach(() => {
  vi.clearAllMocks();
});

test("renders a dropdown with preset options when no custom range", () => {
  render(<GlobalDateRangePicker />);
  const select = screen.getByRole("combobox", { name: /time range/i });
  expect(select).toBeInTheDocument();
  expect(screen.getByRole("option", { name: "Last 1 hour" })).toBeInTheDocument();
  expect(screen.getByRole("option", { name: "Last 5 min" })).toBeInTheDocument();
  expect(screen.getByRole("option", { name: "Last 12 hours" })).toBeInTheDocument();
});

test("selecting a different preset calls setPreset", () => {
  render(<GlobalDateRangePicker />);
  fireEvent.change(screen.getByRole("combobox"), { target: { value: "3h" } });
  expect(mockSetPreset).toHaveBeenCalledWith("3h");
});

test("shows custom range label and reset button when preset is null", () => {
  vi.mocked(useGlobalDateRange).mockReturnValue({
    preset: null,
    fromMs: 1746100800000,
    toMs: 1746104400000,
    setPreset: mockSetPreset,
    setCustomRange: mockSetCustomRange,
    clearCustomRange: mockClearCustomRange,
  });
  render(<GlobalDateRangePicker />);
  expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: /reset/i })).toBeInTheDocument();
});

test("reset button calls clearCustomRange", () => {
  vi.mocked(useGlobalDateRange).mockReturnValue({
    preset: null,
    fromMs: 1746100800000,
    toMs: 1746104400000,
    setPreset: mockSetPreset,
    setCustomRange: mockSetCustomRange,
    clearCustomRange: mockClearCustomRange,
  });
  render(<GlobalDateRangePicker />);
  fireEvent.click(screen.getByRole("button", { name: /reset/i }));
  expect(mockClearCustomRange).toHaveBeenCalledOnce();
});
