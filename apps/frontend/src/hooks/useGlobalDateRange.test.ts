import { describe, expect, it, vi } from "vitest";
import { deriveRange, presetToMs, PRESET_OPTIONS, DEFAULT_PRESET } from "./useGlobalDateRange";

describe("presetToMs", () => {
  it("converts 5m to 5 minutes in ms", () => {
    expect(presetToMs("5m")).toBe(5 * 60 * 1000);
  });
  it("converts 1h to 60 minutes in ms", () => {
    expect(presetToMs("1h")).toBe(60 * 60 * 1000);
  });
  it("converts 12h to 720 minutes in ms", () => {
    expect(presetToMs("12h")).toBe(12 * 60 * 60 * 1000);
  });
});

describe("deriveRange", () => {
  it("uses from/to when both are present", () => {
    const result = deriveRange({ from: 1000, to: 2000 });
    expect(result).toEqual({ fromMs: 1000, toMs: 2000 });
  });

  it("falls back to preset when from/to are absent", () => {
    const now = 100_000;
    vi.spyOn(Date, "now").mockReturnValue(now);
    const result = deriveRange({ preset: "5m" });
    expect(result).toEqual({ fromMs: now - 5 * 60 * 1000, toMs: now });
    vi.restoreAllMocks();
  });

  it("uses DEFAULT_PRESET when nothing is provided", () => {
    const now = 100_000;
    vi.spyOn(Date, "now").mockReturnValue(now);
    const result = deriveRange({});
    expect(result).toEqual({ fromMs: now - presetToMs(DEFAULT_PRESET), toMs: now });
    vi.restoreAllMocks();
  });
});

describe("PRESET_OPTIONS", () => {
  it("has 6 options", () => {
    expect(PRESET_OPTIONS).toHaveLength(6);
  });
  it("starts with 5m and ends with 12h", () => {
    expect(PRESET_OPTIONS[0].value).toBe("5m");
    expect(PRESET_OPTIONS[5].value).toBe("12h");
  });
});
