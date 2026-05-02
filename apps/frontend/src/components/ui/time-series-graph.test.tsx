import { describe, expect, it } from "vitest";
import { buildPolylinePoints, toX, toY } from "./time-series-graph";

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
