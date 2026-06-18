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
});
