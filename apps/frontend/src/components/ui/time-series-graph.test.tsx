import { describe, expect, it } from "vitest";
import { buildPolylinePoints, toX, toY, pixelToMs, TimeSeriesGraph } from "./time-series-graph";
import { render } from "@testing-library/react";
import { fireEvent } from "@testing-library/react";
import { vi } from "vitest";

describe("toX", () => {
  it("maps range start to 0", () => {
    expect(toX(1000, 1000, 2000, 400)).toBe(0);
  });
  it("maps range end to width", () => {
    expect(toX(2000, 1000, 2000, 400)).toBe(400);
  });
  it("maps midpoint to half width", () => {
    expect(toX(1500, 1000, 2000, 400)).toBe(200);
  });
  it("returns 0 when range is zero", () => {
    expect(toX(1000, 1000, 1000, 400)).toBe(0);
  });
});

describe("toY", () => {
  it("maps max value to plotTop", () => {
    expect(toY(100, 0, 100, 10, 70)).toBe(10);
  });
  it("maps min value to plotBottom", () => {
    expect(toY(0, 0, 100, 10, 70)).toBe(70);
  });
  it("maps mid value to vertical midpoint", () => {
    expect(toY(50, 0, 100, 10, 70)).toBe(40);
  });
  it("returns midpoint when value range is zero", () => {
    expect(toY(5, 5, 5, 10, 70)).toBe(40);
  });
});

describe("buildPolylinePoints", () => {
  it("returns empty string for no points", () => {
    const series = { key: "s", label: "S", color: "#fff", points: [] };
    expect(buildPolylinePoints(series, 0, 100, 400, 10, 70)).toBe("");
  });
  it("maps a single point correctly", () => {
    const series = {
      key: "s", label: "S", color: "#fff",
      points: [{ timestampMs: 50, value: 50 }],
    };
    // toX(50, 0, 100, 400) = 200; toY(50, 50, 50, 10, 70) = 40 (zero-range midpoint)
    expect(buildPolylinePoints(series, 0, 100, 400, 10, 70)).toBe("200,40");
  });
  it("maps two points spanning the full range", () => {
    const series = {
      key: "s", label: "S", color: "#fff",
      points: [
        { timestampMs: 0, value: 0 },
        { timestampMs: 100, value: 100 },
      ],
    };
    // Point 1: toX(0,0,100,400)=0, toY(0,0,100,10,70)=70 → "0,70"
    // Point 2: toX(100,0,100,400)=400, toY(100,0,100,10,70)=10 → "400,10"
    expect(buildPolylinePoints(series, 0, 100, 400, 10, 70)).toBe("0,70 400,10");
  });
});

describe("pixelToMs", () => {
  it("maps x=0 to rangeStartMs", () => {
    expect(pixelToMs(0, 1000, 2000, 400)).toBe(1000);
  });
  it("maps x=width to rangeEndMs", () => {
    expect(pixelToMs(400, 1000, 2000, 400)).toBe(2000);
  });
  it("maps x=200 to midpoint", () => {
    expect(pixelToMs(200, 1000, 2000, 400)).toBe(1500);
  });
});

describe("TimeSeriesGraph brush", () => {
  it("does not fire onRangeSelect when prop is not provided", () => {
    const onRangeSelect = vi.fn();
    render(
      <TimeSeriesGraph
        series={[]}
        rangeStartMs={1000}
        rangeEndMs={2000}
        ariaLabel="test graph"
      />
    );
    const svg = document.querySelector("svg")!;
    fireEvent.pointerDown(svg, { clientX: 10 });
    fireEvent.pointerUp(svg, { clientX: 50 });
    expect(onRangeSelect).not.toHaveBeenCalled();
  });

  it("fires onRangeSelect with from/to ms when brush drag completes", () => {
    const onRangeSelect = vi.fn();
    const rangeStartMs = 0;
    const rangeEndMs = 1000;
    render(
      <TimeSeriesGraph
        series={[]}
        rangeStartMs={rangeStartMs}
        rangeEndMs={rangeEndMs}
        onRangeSelect={onRangeSelect}
        ariaLabel="test graph"
      />
    );
    const svg = document.querySelector("svg")!;
    Object.defineProperty(svg, "getBoundingClientRect", {
      value: () => ({ left: 0, width: 400, top: 0, height: 80 } as DOMRect),
    });
    fireEvent.pointerDown(svg, { clientX: 0, pointerId: 1 });
    fireEvent.pointerMove(svg, { clientX: 200, pointerId: 1 });
    fireEvent.pointerUp(svg, { clientX: 200, pointerId: 1 });
    expect(onRangeSelect).toHaveBeenCalledOnce();
    const [from, to] = onRangeSelect.mock.calls[0];
    expect(from).toBe(0);
    expect(to).toBe(500);
  });
});
