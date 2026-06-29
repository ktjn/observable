import { describe, it, expect } from "vitest";
import { TIME_FORMAT_OPTIONS, DEFAULT_TIME_FORMAT } from "./timeDisplay";

describe("timeDisplay", () => {
  it("should have correct timezone labels that are user-friendly", () => {
    const options = TIME_FORMAT_OPTIONS;

    expect(options).toEqual([
      { value: "iso-local-ms", label: "Local time (ms)" },
      { value: "iso-utc-ms", label: "UTC (ms)" },
      { value: "iso-local-ns", label: "Local time (ns)" },
      { value: "iso-utc-ns", label: "UTC (ns)" },
      { value: "unix-ms", label: "Unix timestamp (ms)" },
      { value: "unix-ns", label: "Unix timestamp (ns)" },
    ]);
  });

  it("should have default time format with 'Local time (ms)' label", () => {
    const defaultOption = TIME_FORMAT_OPTIONS.find(
      (opt) => opt.value === DEFAULT_TIME_FORMAT
    );
    expect(defaultOption).toBeDefined();
    expect(defaultOption?.label).toBe("Local time (ms)");
  });

  it("should not contain technical jargon like 'ISO8601' or '[ms]' in labels", () => {
    TIME_FORMAT_OPTIONS.forEach((option) => {
      expect(option.label).not.toMatch(/ISO8601/);
      expect(option.label).not.toMatch(/\[ms\]/);
      expect(option.label).not.toMatch(/\[ns\]/);
    });
  });
});
