import { describe, it, expect } from "vitest";
import {
  otelSeverity,
  formatLogMessage,
  formatContextValue,
  getSeverityColor,
} from "./logFormatting";

describe("otelSeverity", () => {
  it("returns TRACE with neutral tone for severity < 5", () => {
    expect(otelSeverity(1)).toEqual({ label: "TRACE", tone: "neutral" });
    expect(otelSeverity(4)).toEqual({ label: "TRACE", tone: "neutral" });
    expect(otelSeverity(0)).toEqual({ label: "TRACE", tone: "neutral" });
  });

  it("returns DEBUG with info tone for severity 5–8", () => {
    expect(otelSeverity(5)).toEqual({ label: "DEBUG", tone: "info" });
    expect(otelSeverity(8)).toEqual({ label: "DEBUG", tone: "info" });
  });

  it("returns INFO with good tone for severity 9–12", () => {
    expect(otelSeverity(9)).toEqual({ label: "INFO", tone: "good" });
    expect(otelSeverity(12)).toEqual({ label: "INFO", tone: "good" });
  });

  it("returns WARN with warn tone for severity 13–16", () => {
    expect(otelSeverity(13)).toEqual({ label: "WARN", tone: "warn" });
    expect(otelSeverity(16)).toEqual({ label: "WARN", tone: "warn" });
  });

  it("returns ERROR with bad tone for severity 17–20", () => {
    expect(otelSeverity(17)).toEqual({ label: "ERROR", tone: "bad" });
    expect(otelSeverity(20)).toEqual({ label: "ERROR", tone: "bad" });
  });

  it("returns FATAL with bad tone for severity >= 21", () => {
    expect(otelSeverity(21)).toEqual({ label: "FATAL", tone: "bad" });
    expect(otelSeverity(24)).toEqual({ label: "FATAL", tone: "bad" });
    expect(otelSeverity(100)).toEqual({ label: "FATAL", tone: "bad" });
  });
});

describe("formatLogMessage", () => {
  it("passes through a plain string unchanged", () => {
    expect(formatLogMessage("hello world")).toBe("hello world");
  });

  it("extracts the message field from an object", () => {
    expect(formatLogMessage({ message: "extracted message", level: "info" })).toBe(
      "extracted message"
    );
  });

  it("extracts the msg field when message is absent", () => {
    expect(formatLogMessage({ msg: "short msg", ts: 1234567890 })).toBe("short msg");
  });

  it("falls back to key=value pairs when no message/msg/body field exists", () => {
    const result = formatLogMessage({ foo: "bar", count: 3 });
    expect(result).toBe("foo=bar count=3");
  });

  it("returns empty string for null", () => {
    expect(formatLogMessage(null)).toBe("");
  });

  it("returns empty string for undefined", () => {
    expect(formatLogMessage(undefined)).toBe("");
  });

  it("converts a number body to string", () => {
    expect(formatLogMessage(42)).toBe("42");
  });

  it("converts a boolean body to string", () => {
    expect(formatLogMessage(true)).toBe("true");
  });
});

describe("formatContextValue", () => {
  it("passes through a string unchanged", () => {
    expect(formatContextValue("hello")).toBe("hello");
  });

  it("converts a number to string", () => {
    expect(formatContextValue(42)).toBe("42");
  });

  it("converts a boolean to string", () => {
    expect(formatContextValue(false)).toBe("false");
  });

  it("returns empty string for null", () => {
    expect(formatContextValue(null)).toBe("");
  });

  it("returns empty string for undefined", () => {
    expect(formatContextValue(undefined)).toBe("");
  });

  it("JSON-serialises an object", () => {
    expect(formatContextValue({ a: 1 })).toBe('{"a":1}');
  });

  it("JSON-serialises an array", () => {
    expect(formatContextValue([1, 2, 3])).toBe("[1,2,3]");
  });
});

describe("getSeverityColor", () => {
  it("returns --bad for severity >= 13 (WARN and above)", () => {
    expect(getSeverityColor(13)).toBe("var(--bad)");
    expect(getSeverityColor(17)).toBe("var(--bad)");
    expect(getSeverityColor(21)).toBe("var(--bad)");
    expect(getSeverityColor(100)).toBe("var(--bad)");
  });

  it("returns --warn for severity 9–12 (INFO range)", () => {
    expect(getSeverityColor(9)).toBe("var(--warn)");
    expect(getSeverityColor(12)).toBe("var(--warn)");
  });

  it("returns --brand for severity 5–8 (DEBUG range)", () => {
    expect(getSeverityColor(5)).toBe("var(--brand)");
    expect(getSeverityColor(8)).toBe("var(--brand)");
  });

  it("returns --muted for severity < 5 (TRACE range)", () => {
    expect(getSeverityColor(0)).toBe("var(--muted)");
    expect(getSeverityColor(4)).toBe("var(--muted)");
  });
});
