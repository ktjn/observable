import { describe, expect, it } from "vitest";
import { normalizeService } from "./useGlobalServiceFilter";

describe("normalizeService", () => {
  it("returns the string for a non-empty string", () => {
    expect(normalizeService("checkout")).toBe("checkout");
  });

  it("trims whitespace from both ends", () => {
    expect(normalizeService("  checkout  ")).toBe("checkout");
  });

  it("returns undefined for an empty string", () => {
    expect(normalizeService("")).toBeUndefined();
  });

  it("returns undefined for a whitespace-only string", () => {
    expect(normalizeService("   ")).toBeUndefined();
  });

  it("returns undefined for undefined", () => {
    expect(normalizeService(undefined)).toBeUndefined();
  });

  it("returns undefined for null", () => {
    expect(normalizeService(null)).toBeUndefined();
  });

  it("returns undefined for a number", () => {
    expect(normalizeService(42)).toBeUndefined();
  });
});
