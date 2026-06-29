import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { Histogram, type HistogramBucket } from "./histogram";

type Cat = "ok" | "error";

const buckets: HistogramBucket<Cat>[] = [
  { startMs: 0, endMs: 1000, total: 5, categories: { ok: 5, error: 0 } },
  { startMs: 1000, endMs: 2000, total: 3, categories: { ok: 1, error: 2 } },
];

const categoryColors: Record<Cat, string> = { ok: "fill-[var(--good)]", error: "fill-[var(--bad)]" };

function renderHistogram(onRangeSelect?: (from: number, to: number) => void) {
  return render(
    <Histogram
      buckets={buckets}
      categoryOrder={["ok", "error"]}
      categoryColors={categoryColors}
      format={(ms) => String(ms)}
      onRangeSelect={onRangeSelect}
      ariaLabel="Test histogram"
    />,
  );
}

function make10Buckets(): HistogramBucket<Cat>[] {
  return Array.from({ length: 10 }, (_, i) => ({
    startMs: i * 1000,
    endMs: (i + 1) * 1000,
    total: i + 1,
    categories: { ok: i + 1, error: 0 },
  }));
}

describe("Histogram", () => {
  test("renders an SVG element", () => {
    renderHistogram();
    const group = screen.getByRole("group", { name: "Test histogram" });
    expect(group.querySelector("svg")).toBeInTheDocument();
  });

  test("renders one bar segment per non-zero category with the expected title", () => {
    renderHistogram();
    const group = screen.getByRole("group", { name: "Test histogram" });
    expect(group.querySelector("[title='0 ok: 5']")).toBeInTheDocument();
    expect(group.querySelector("[title='1000 ok: 1']")).toBeInTheDocument();
    expect(group.querySelector("[title='1000 error: 2']")).toBeInTheDocument();
  });

  test("does not render a segment for a zero-count category", () => {
    renderHistogram();
    const group = screen.getByRole("group", { name: "Test histogram" });
    expect(group.querySelector("[title='0 error: 0']")).not.toBeInTheDocument();
  });

  test("calls onRangeSelect with bucket boundaries on drag", () => {
    const onRangeSelect = vi.fn();
    renderHistogram(onRangeSelect);
    const group = screen.getByRole("group", { name: "Test histogram" });
    const svg = group.querySelector("svg")!;
    const rect = { left: 0, width: 200, top: 0, height: 100, right: 200, bottom: 100, x: 0, y: 0, toJSON() {} };
    svg.getBoundingClientRect = () => rect as DOMRect;
    svg.dispatchEvent(new PointerEvent("pointerdown", { clientX: 10, bubbles: true }));
    svg.dispatchEvent(new PointerEvent("pointermove", { clientX: 190, bubbles: true }));
    svg.dispatchEvent(new PointerEvent("pointerup", { clientX: 190, bubbles: true }));
    expect(onRangeSelect).toHaveBeenCalledWith(0, 2000);
  });

  // Axes tests
  test("renders at least 3 x-axis time tick labels with 10 buckets", () => {
    const { container } = render(
      <Histogram
        buckets={make10Buckets()}
        categoryOrder={["ok", "error"]}
        categoryColors={categoryColors}
        format={(ms) => `t${ms}`}
        ariaLabel="Axes histogram"
      />,
    );
    const texts = container.querySelectorAll("text");
    // Should have at least 3 time ticks plus the y-axis max label
    expect(texts.length).toBeGreaterThanOrEqual(3);
  });

  test("x-axis tick labels use the format prop with 10 buckets", () => {
    const { container } = render(
      <Histogram
        buckets={make10Buckets()}
        categoryOrder={["ok", "error"]}
        categoryColors={categoryColors}
        format={(ms) => `t${ms}`}
        ariaLabel="Axes histogram"
      />,
    );
    const texts = Array.from(container.querySelectorAll("text")).map((el) => el.textContent ?? "");
    // The first bucket startMs is 0, so first tick should be "t0"
    expect(texts).toContain("t0");
    // The last bucket startMs is 9000, so last tick should be "t9000"
    expect(texts).toContain("t9000");
  });

  test("renders y-axis max label with 10 buckets", () => {
    const { container } = render(
      <Histogram
        buckets={make10Buckets()}
        categoryOrder={["ok", "error"]}
        categoryColors={categoryColors}
        format={(ms) => `t${ms}`}
        ariaLabel="Axes histogram"
      />,
    );
    const texts = Array.from(container.querySelectorAll("text")).map((el) => el.textContent ?? "");
    // max is 10 (last bucket has total = 10), which is > 1 so should be shown
    expect(texts).toContain("10");
  });

  test("renders no text elements when buckets is empty", () => {
    const { container } = render(
      <Histogram
        buckets={[]}
        categoryOrder={["ok", "error"]}
        categoryColors={categoryColors}
        format={(ms) => `t${ms}`}
        ariaLabel="Empty histogram"
      />,
    );
    const texts = container.querySelectorAll("text");
    expect(texts.length).toBe(0);
  });

  test("does not render y-axis max label when max is 1 (placeholder)", () => {
    const { container } = render(
      <Histogram
        buckets={[{ startMs: 0, endMs: 1000, total: 1, categories: { ok: 1, error: 0 } }]}
        categoryOrder={["ok", "error"]}
        categoryColors={categoryColors}
        format={(ms) => `t${ms}`}
        ariaLabel="Single bucket histogram"
      />,
    );
    const texts = Array.from(container.querySelectorAll("text")).map((el) => el.textContent ?? "");
    expect(texts).not.toContain("1");
  });
});
