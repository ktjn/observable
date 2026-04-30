import { describe, expect, it } from "vitest";
import { formatTimestamp } from "./formatTimestamp";

// 2025-04-26 09:31:49.123 UTC  (1745659909.123 seconds since epoch)
const NANOS = "1745659909123456789";

describe("formatTimestamp", () => {
  it("UTC mode produces compact ISO with Z suffix and millisecond precision", () => {
    expect(formatTimestamp(NANOS, true)).toBe("2025-04-26 09:31:49.123Z");
  });

  it("local mode produces compact ISO without Z suffix", () => {
    const result = formatTimestamp(NANOS, false);
    expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}$/);
    expect(result).not.toContain("Z");
    expect(result).not.toContain("T");
  });

  it("UTC mode does not contain T separator", () => {
    expect(formatTimestamp(NANOS, true)).not.toContain("T");
  });

  it("UTC mode ends with Z", () => {
    expect(formatTimestamp(NANOS, true)).toMatch(/Z$/);
  });

  it("local mode does not end with Z", () => {
    expect(formatTimestamp(NANOS, false)).not.toMatch(/Z$/);
  });

  it("handles nanosecond string with leading-zero milliseconds", () => {
    const exactSecond = "1745659909000000000";
    expect(formatTimestamp(exactSecond, true)).toBe("2025-04-26 09:31:49.000Z");
  });

  it("accepts numeric input via String() coercion", () => {
    expect(formatTimestamp(1745659909123000000 as unknown as string, true)).toMatch(
      /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}Z$/,
    );
  });
});
