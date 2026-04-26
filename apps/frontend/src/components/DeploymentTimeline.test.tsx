import { describe, it, expect } from "vitest";
import { markerPosition, markerColor } from "./DeploymentTimeline";

describe("markerPosition", () => {
  const rangeStart = new Date("2024-01-01T00:00:00Z").getTime();
  const rangeEnd   = new Date("2024-01-01T01:00:00Z").getTime();
  const width = 400;

  it("places marker at left edge when at rangeStart", () => {
    expect(markerPosition(rangeStart, rangeStart, rangeEnd, width)).toBe(0);
  });

  it("places marker at right edge when at rangeEnd", () => {
    expect(markerPosition(rangeEnd, rangeStart, rangeEnd, width)).toBe(400);
  });

  it("places marker at midpoint when halfway", () => {
    const mid = (rangeStart + rangeEnd) / 2;
    expect(markerPosition(mid, rangeStart, rangeEnd, width)).toBe(200);
  });

  it("clamps to 0 when marker is before range", () => {
    expect(markerPosition(rangeStart - 1000, rangeStart, rangeEnd, width)).toBe(0);
  });

  it("clamps to width when marker is after range", () => {
    expect(markerPosition(rangeEnd + 1000, rangeStart, rangeEnd, width)).toBe(400);
  });
});

describe("markerColor", () => {
  it("returns green for success",       () => expect(markerColor("success")).toBe("#22c55e"));
  it("returns blue for in_progress",    () => expect(markerColor("in_progress")).toBe("#3b82f6"));
  it("returns red for failed",          () => expect(markerColor("failed")).toBe("#ef4444"));
  it("returns orange for rolled_back",  () => expect(markerColor("rolled_back")).toBe("#f97316"));
  it("returns grey for unknown status", () => expect(markerColor("unknown")).toBe("#9ca3af"));
});
