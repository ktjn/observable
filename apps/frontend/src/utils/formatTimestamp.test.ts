import { describe, expect, it } from "vitest";
import { formatTimestamp } from "./formatTimestamp";

// 2025-04-26 09:31:49.123456789 UTC
const NANOS = "1745659909123456789";

describe("formatTimestamp", () => {
  describe("iso-utc-ms", () => {
    it("produces compact ISO with Z suffix and millisecond precision", () => {
      expect(formatTimestamp(NANOS, "iso-utc-ms")).toBe("2025-04-26 09:31:49.123Z");
    });

    it("does not contain T separator", () => {
      expect(formatTimestamp(NANOS, "iso-utc-ms")).not.toContain("T");
    });

    it("ends with Z", () => {
      expect(formatTimestamp(NANOS, "iso-utc-ms")).toMatch(/Z$/);
    });

    it("handles leading-zero milliseconds", () => {
      const exactSecond = "1745659909000000000";
      expect(formatTimestamp(exactSecond, "iso-utc-ms")).toBe("2025-04-26 09:31:49.000Z");
    });
  });

  describe("iso-local-ms", () => {
    it("produces compact ISO without Z suffix", () => {
      const result = formatTimestamp(NANOS, "iso-local-ms");
      expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}$/);
      expect(result).not.toContain("Z");
      expect(result).not.toContain("T");
    });

    it("is the default when format is omitted via fallback", () => {
      const result = formatTimestamp(NANOS, "iso-local-ms");
      expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}$/);
    });
  });

  describe("iso-utc-ns", () => {
    it("includes sub-millisecond digits and Z suffix", () => {
      const result = formatTimestamp(NANOS, "iso-utc-ns");
      expect(result).toMatch(/Z$/);
      expect(result).toMatch(/2025-04-26 09:31:49\.123456789Z/);
    });
  });

  describe("iso-local-ns", () => {
    it("includes sub-millisecond digits and no Z suffix", () => {
      const result = formatTimestamp(NANOS, "iso-local-ns");
      expect(result).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{9}$/);
      expect(result).not.toContain("Z");
    });
  });

  describe("unix-ms", () => {
    it("returns millisecond timestamp as string", () => {
      expect(formatTimestamp(NANOS, "unix-ms")).toBe("1745659909123");
    });
  });

  describe("unix-ns", () => {
    it("returns the raw nanosecond string", () => {
      expect(formatTimestamp(NANOS, "unix-ns")).toBe(NANOS);
    });
  });

  describe("numeric input", () => {
    it("accepts a number via string coercion", () => {
      expect(formatTimestamp(1745659909123000000, "iso-utc-ms")).toMatch(
        /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}Z$/,
      );
    });
  });
});
