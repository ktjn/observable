import { describe, expect, test } from "vitest";
import { detectQueryMode, toShorthandQuery } from "./detectQueryMode";

describe("detectQueryMode", () => {
  test("empty string is ai (caller should no-op on empty anyway)", () => {
    expect(detectQueryMode("")).toBe("ai");
  });

  test("explicit slash prefix is always filter", () => {
    expect(detectQueryMode("/anything goes here")).toBe("filter");
  });

  test("field:value shorthand is filter", () => {
    expect(detectQueryMode("service:checkout")).toBe("filter");
    expect(detectQueryMode("environment:prod")).toBe("filter");
  });

  test("m: metric shorthand is filter", () => {
    expect(detectQueryMode("m:http_requests")).toBe("filter");
  });

  test("op: operation shorthand is filter", () => {
    expect(detectQueryMode("op:topk")).toBe("filter");
  });

  test("bare word is search", () => {
    expect(detectQueryMode("error")).toBe("search");
  });

  test("wildcard-wrapped word is search", () => {
    expect(detectQueryMode("*error*")).toBe("search");
    expect(detectQueryMode("error*")).toBe("search");
    expect(detectQueryMode("*error")).toBe("search");
  });

  test("word with dots/dashes/underscores is still search", () => {
    expect(detectQueryMode("checkout-service")).toBe("search");
    expect(detectQueryMode("http.server.errors")).toBe("search");
  });

  test("multi-word phrase is ai", () => {
    expect(detectQueryMode("show checkout services")).toBe("ai");
  });

  test("a full question is ai", () => {
    expect(detectQueryMode("what is p99 latency over the last hour?")).toBe("ai");
  });

  test("quoted phrase with spaces is ai (not a single shorthand token)", () => {
    expect(detectQueryMode('"timeout error"')).toBe("ai");
  });

  test("raw IR JSON text is ai (passed through untouched to the existing raw-IR path)", () => {
    expect(detectQueryMode('{"operation":"catalog"}')).toBe("ai");
  });
});

describe("toShorthandQuery", () => {
  test("filter mode gets a bare slash prefix", () => {
    expect(toShorthandQuery("service:checkout", "filter")).toBe("/service:checkout");
  });

  test("search mode strips wildcards and prefixes slash", () => {
    expect(toShorthandQuery("*error*", "search")).toBe("/error");
    expect(toShorthandQuery("error*", "search")).toBe("/error");
    expect(toShorthandQuery("*error", "search")).toBe("/error");
    expect(toShorthandQuery("error", "search")).toBe("/error");
  });

  test("already-slash-prefixed text is not double-prefixed", () => {
    expect(toShorthandQuery("/service:checkout", "filter")).toBe("/service:checkout");
  });

  test("ai mode passes text through unchanged", () => {
    expect(toShorthandQuery("show checkout services", "ai")).toBe("show checkout services");
  });
});
