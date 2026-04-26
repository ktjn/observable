import { describe, expect, it } from "vitest";
import { formatTimestamp } from "./formatTimestamp";

// 2026-04-26T09:35:09.123456789Z  →  ms = 1745659909123, sub-ms = "456789"
const NANOS = "1745659909123456789";

describe("formatTimestamp", () => {
  it("UTC mode produces full ISO 8601 string ending with Z", () => {
    const result = formatTimestamp(NANOS, true);
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{9}Z$/);
    expect(result.endsWith("Z")).toBe(true);
  });

  it("UTC mode preserves all 9 fractional digits including sub-ms", () => {
    const result = formatTimestamp(NANOS, true);
    // The last 9 chars before Z must be "123456789"
    const fracPart = result.replace(/.*\.(\d+)Z$/, "$1");
    expect(fracPart).toBe("123456789");
  });

  it("local mode produces full ISO 8601 string without Z suffix", () => {
    const result = formatTimestamp(NANOS, false);
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{9}$/);
    expect(result.endsWith("Z")).toBe(false);
  });

  it("local mode preserves the 6 sub-ms digits", () => {
    const result = formatTimestamp(NANOS, false);
    const fracPart = result.replace(/.*\.(\d+)$/, "$1");
    // Last 6 of the 9 fractional digits must be "456789"
    expect(fracPart.slice(3)).toBe("456789");
  });

  it("UTC and local modes produce different strings when timezone offset is non-zero (or same if UTC+0)", () => {
    const utcResult = formatTimestamp(NANOS, true);
    const localResult = formatTimestamp(NANOS, false);
    // They differ in format (Z suffix) at minimum
    expect(utcResult).not.toBe(localResult);
  });

  it("handles nanos with leading zeros in sub-ms portion", () => {
    // timestamp where sub-ms digits are "000001"
    const nanosWithZeroSubMs = "1745659909123000001";
    const result = formatTimestamp(nanosWithZeroSubMs, true);
    const fracPart = result.replace(/.*\.(\d+)Z$/, "$1");
    expect(fracPart.slice(3)).toBe("000001");
  });

  it("UTC mode matches toISOString ms portion", () => {
    const ms = Math.floor(Number(NANOS) / 1_000_000);
    const date = new Date(ms);
    const isoMs = date.toISOString().replace(/Z$/, "");
    const result = formatTimestamp(NANOS, true);
    // Result should start with the same ISO prefix up to ms
    expect(result.startsWith(isoMs)).toBe(true);
  });

  it("accepts a numeric value coerced to string (runtime API may return number)", () => {
    // Simulate the API returning a JS number instead of a string
    const result = formatTimestamp(1745659909123456789 as unknown as string, true);
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{9}Z$/);
  });
});
