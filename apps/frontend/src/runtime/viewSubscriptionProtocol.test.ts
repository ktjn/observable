import { describe, expect, it } from "vitest";
import {
  applyKeyedChanges,
  compareRevisions,
  deriveViewTimeRange,
  encodeViewsParam,
  parseViewSubscriptionEvent,
  viewSubscriptionUrl,
} from "./viewSubscriptionProtocol";
import type { ViewSubscriptionDefinition } from "./viewSubscriptionProtocol";

const healthView: ViewSubscriptionDefinition = {
  viewId: "health",
  query: {
    kind: "service-health",
    serviceId: "11111111-1111-4111-8111-111111111111",
    environmentClass: "production",
  },
  timeRange: { mode: "rolling", window: "1h" },
  queryKey: ["service-health", "t1", "svc"],
};

describe("parseViewSubscriptionEvent", () => {
  it("parses a snapshot tagged by view, query revision and result revision", () => {
    const event = parseViewSubscriptionEvent(
      "snapshot",
      JSON.stringify({
        v: 1,
        viewId: "health",
        queryRevision: "r1",
        revision: "1842",
        data: { service: "payments", health: "normal" },
      }),
    );
    expect(event).toEqual({
      type: "snapshot",
      viewId: "health",
      queryRevision: "r1",
      revision: "1842",
      data: { service: "payments", health: "normal" },
    });
  });

  it("parses a delta carrying passed-through keyed ops", () => {
    const event = parseViewSubscriptionEvent(
      "delta",
      JSON.stringify({
        v: 1,
        viewId: "health",
        queryRevision: "r1",
        revision: "1843",
        changes: [{ op: "upsert", key: "payments", value: { health: "degraded" } }],
      }),
    );
    expect(event).toEqual({
      type: "delta",
      viewId: "health",
      queryRevision: "r1",
      revision: "1843",
      changes: [{ op: "upsert", key: "payments", value: { health: "degraded" } }],
    });
  });

  it("parses an invalidate hint that carries no revision or data", () => {
    const event = parseViewSubscriptionEvent(
      "invalidate",
      JSON.stringify({ v: 1, viewId: "expensive-aggregate", queryRevision: "r1" }),
    );
    expect(event).toEqual({
      type: "invalidate",
      viewId: "expensive-aggregate",
      queryRevision: "r1",
    });
  });

  it("parses a reset and its low-cardinality reason", () => {
    const event = parseViewSubscriptionEvent(
      "reset",
      JSON.stringify({ v: 1, reason: "cursor_unavailable" }),
    );
    expect(event).toEqual({ type: "reset", reason: "cursor_unavailable" });
  });

  it("rejects an unknown event name", () => {
    expect(parseViewSubscriptionEvent("ack", JSON.stringify({ v: 1 }))).toBeNull();
  });

  it("rejects a payload from an incompatible protocol version", () => {
    const raw = JSON.stringify({
      v: 2,
      viewId: "health",
      queryRevision: "r1",
      revision: "1",
      data: {},
    });
    expect(parseViewSubscriptionEvent("snapshot", raw)).toBeNull();
  });

  it("rejects malformed JSON instead of throwing", () => {
    expect(parseViewSubscriptionEvent("snapshot", "{not json")).toBeNull();
  });

  it("rejects a data event missing its view or revision tags", () => {
    expect(
      parseViewSubscriptionEvent("replace", JSON.stringify({ v: 1, viewId: "health", data: {} })),
    ).toBeNull();
    expect(
      parseViewSubscriptionEvent(
        "snapshot",
        JSON.stringify({ v: 1, queryRevision: "r1", revision: "1", data: {} }),
      ),
    ).toBeNull();
  });

  it("rejects a delta whose changes are not keyed upsert/delete ops", () => {
    const raw = JSON.stringify({
      v: 1,
      viewId: "health",
      queryRevision: "r1",
      revision: "2",
      changes: [{ op: "patch", key: "a" }],
    });
    expect(parseViewSubscriptionEvent("delta", raw)).toBeNull();
  });

  it("parses a lag-conflated replace carrying supersedesThrough and reason", () => {
    const event = parseViewSubscriptionEvent(
      "replace",
      JSON.stringify({
        v: 1,
        viewId: "health",
        queryRevision: "r1",
        revision: "5",
        supersedesThrough: "4",
        reason: "subscriber_lag",
        data: { payments: { health: "degraded" } },
      }),
    );
    expect(event).toEqual({
      type: "replace",
      viewId: "health",
      queryRevision: "r1",
      revision: "5",
      supersedesThrough: "4",
      reason: "subscriber_lag",
      data: { payments: { health: "degraded" } },
    });
  });

  it("rejects a replace whose supersedesThrough exceeds revision", () => {
    const raw = JSON.stringify({
      v: 1,
      viewId: "health",
      queryRevision: "r1",
      revision: "3",
      supersedesThrough: "4",
      data: {},
    });
    expect(parseViewSubscriptionEvent("replace", raw)).toBeNull();
  });
});

describe("compareRevisions", () => {
  it("orders revisions numerically, not lexicographically", () => {
    expect(compareRevisions("9", "10")).toBeLessThan(0);
    expect(compareRevisions("10", "9")).toBeGreaterThan(0);
    expect(compareRevisions("10", "10")).toBe(0);
  });

  it("handles revisions beyond Number.MAX_SAFE_INTEGER", () => {
    expect(compareRevisions("18446744073709551615", "18446744073709551614")).toBeGreaterThan(0);
  });

  it("treats a non-numeric revision as unordered", () => {
    expect(compareRevisions("abc", "1")).toBeNaN();
  });
});

describe("applyKeyedChanges", () => {
  it("upserts into a keyed record", () => {
    expect(applyKeyedChanges({ a: 1 }, [{ op: "upsert", key: "b", value: 2 }])).toEqual({
      a: 1,
      b: 2,
    });
  });

  it("replaces an existing key without touching its siblings", () => {
    const current = { a: { health: "normal" }, b: { health: "normal" } };
    expect(
      applyKeyedChanges(current, [{ op: "upsert", key: "a", value: { health: "degraded" } }]),
    ).toEqual({ a: { health: "degraded" }, b: { health: "normal" } });
  });

  it("deletes a key", () => {
    expect(applyKeyedChanges({ a: 1, b: 2 }, [{ op: "delete", key: "a" }])).toEqual({ b: 2 });
  });

  it("does not mutate the cached object it was given", () => {
    const current = { a: 1 };
    applyKeyedChanges(current, [{ op: "delete", key: "a" }]);
    expect(current).toEqual({ a: 1 });
  });

  it("returns undefined when there is nothing to patch", () => {
    expect(applyKeyedChanges(undefined, [{ op: "upsert", key: "a", value: 1 }])).toBeUndefined();
  });

  it("returns undefined for a non-keyed cached value", () => {
    expect(applyKeyedChanges([1, 2, 3], [{ op: "upsert", key: "a", value: 1 }])).toBeUndefined();
  });
});

describe("encodeViewsParam", () => {
  it("carries only the wire fields of each definition", () => {
    const parsed = JSON.parse(encodeViewsParam([healthView])) as unknown[];
    expect(parsed).toEqual([
      {
        viewId: "health",
        query: {
          kind: "service-health",
          serviceId: "11111111-1111-4111-8111-111111111111",
          environmentClass: "production",
        },
        timeRange: { mode: "rolling", window: "1h" },
      },
    ]);
  });

  it("is stable for the same definitions so an equal view set does not reopen the stream", () => {
    expect(encodeViewsParam([healthView])).toBe(encodeViewsParam([{ ...healthView }]));
  });
});

describe("viewSubscriptionUrl", () => {
  it("URL-encodes the view definition array into the views query parameter", () => {
    const url = viewSubscriptionUrl([healthView]);
    expect(url.startsWith("/v1/ui/subscriptions/events?views=")).toBe(true);
    const views = new URL(url, "http://localhost").searchParams.get("views");
    expect(views).toBe(encodeViewsParam([healthView]));
  });

  it("never carries a session credential or tenant selector", () => {
    const url = viewSubscriptionUrl([healthView]);
    expect(url).not.toMatch(/session|token|tenant/i);
  });
});

describe("deriveViewTimeRange", () => {
  it("maps a relative preset to a rolling window", () => {
    expect(deriveViewTimeRange({ preset: "15m" })).toEqual({ mode: "rolling", window: "15m" });
  });

  it("maps an absolute custom range to a pinned fixed range", () => {
    expect(deriveViewTimeRange({ preset: null, fromMs: 1, toMs: 2 })).toEqual({
      mode: "fixed",
      from: 1,
      to: 2,
    });
  });
});
