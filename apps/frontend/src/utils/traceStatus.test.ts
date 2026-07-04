import { describe, expect, it } from "vitest";
import { formatStatusLabel } from "./traceStatus";

describe("formatStatusLabel", () => {
  it("renders UNSET as OK", () => {
    expect(formatStatusLabel("UNSET")).toBe("OK");
  });

  it("renders OK as OK", () => {
    expect(formatStatusLabel("OK")).toBe("OK");
  });

  it("renders ERROR as ERROR", () => {
    expect(formatStatusLabel("ERROR")).toBe("ERROR");
  });
});
