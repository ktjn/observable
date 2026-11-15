import { describe, expect, it, vi } from "vitest";
import { createEventSourceViewSubscriptionTransport } from "./viewSubscriptions";
import type {
  ViewStreamHandlers,
  ViewSubscriptionDefinition,
} from "../runtime/viewSubscriptionProtocol";
import { encodeViewsParam } from "../runtime/viewSubscriptionProtocol";

const CONNECTING = 0;
const OPEN = 1;
const CLOSED = 2;

/** Minimal stand-in for the native `EventSource`, which jsdom does not provide. */
class FakeEventSource {
  static instances: FakeEventSource[] = [];
  static CONNECTING = CONNECTING;
  static OPEN = OPEN;
  static CLOSED = CLOSED;

  readyState = CONNECTING;
  closed = false;
  private listeners = new Map<string, Set<(event: MessageEvent) => void>>();
  onerror: ((event: Event) => void) | null = null;
  onopen: ((event: Event) => void) | null = null;

  constructor(readonly url: string) {
    FakeEventSource.instances.push(this);
  }

  addEventListener(type: string, listener: (event: MessageEvent) => void) {
    const set = this.listeners.get(type) ?? new Set();
    set.add(listener);
    this.listeners.set(type, set);
  }

  close() {
    this.closed = true;
    this.readyState = CLOSED;
  }

  emit(type: string, data: string) {
    for (const listener of this.listeners.get(type) ?? []) {
      listener({ data } as MessageEvent);
    }
  }

  open() {
    this.readyState = OPEN;
    this.onopen?.(new Event("open"));
  }

  fail(readyState: number) {
    this.readyState = readyState;
    this.onerror?.(new Event("error"));
  }
}

const view: ViewSubscriptionDefinition = {
  viewId: "health",
  query: { kind: "service-status" },
  timeRange: { mode: "rolling", window: "15m" },
  queryKey: ["view", "health"],
};

function handlers() {
  return {
    onOpen: vi.fn<ViewStreamHandlers["onOpen"]>(),
    onEvent: vi.fn<ViewStreamHandlers["onEvent"]>(),
    onError: vi.fn<ViewStreamHandlers["onError"]>(),
  };
}

function setup() {
  FakeEventSource.instances = [];
  const transport = createEventSourceViewSubscriptionTransport(
    FakeEventSource as unknown as typeof EventSource,
  );
  const sink = handlers();
  const handle = transport.open([view], sink);
  const source = FakeEventSource.instances[0]!;
  return { transport, sink, handle, source };
}

describe("EventSource view subscription transport", () => {
  it("registers the view definitions on the stream request itself", () => {
    const { source } = setup();
    expect(source.url.startsWith("/v1/ui/subscriptions/events?views=")).toBe(true);
    const views = new URL(source.url, "http://localhost").searchParams.get("views");
    expect(views).toBe(encodeViewsParam([view]));
  });

  it("reports the stream as open", () => {
    const { source, sink } = setup();
    source.open();
    expect(sink.onOpen).toHaveBeenCalledTimes(1);
  });

  it("forwards a parsed snapshot", () => {
    const { source, sink } = setup();
    source.emit(
      "snapshot",
      JSON.stringify({ v: 1, viewId: "health", queryRevision: "r1", revision: "7", data: { a: 1 } }),
    );
    expect(sink.onEvent).toHaveBeenCalledWith({
      type: "snapshot",
      viewId: "health",
      queryRevision: "r1",
      revision: "7",
      data: { a: 1 },
    });
  });

  it("forwards every protocol event type", () => {
    const { source, sink } = setup();
    source.emit(
      "delta",
      JSON.stringify({
        v: 1,
        viewId: "health",
        queryRevision: "r1",
        revision: "8",
        changes: [{ op: "delete", key: "a" }],
      }),
    );
    source.emit(
      "replace",
      JSON.stringify({ v: 1, viewId: "health", queryRevision: "r1", revision: "9", data: {} }),
    );
    source.emit("invalidate", JSON.stringify({ v: 1, viewId: "health", queryRevision: "r1" }));
    source.emit("reset", JSON.stringify({ v: 1, reason: "subscriber_lag" }));
    expect(sink.onEvent.mock.calls.map(([event]) => event.type)).toEqual([
      "delta",
      "replace",
      "invalidate",
      "reset",
    ]);
  });

  it("drops an unparseable payload instead of forwarding it", () => {
    const { source, sink } = setup();
    source.emit("snapshot", "{not json");
    source.emit("snapshot", JSON.stringify({ v: 99, viewId: "health" }));
    expect(sink.onEvent).not.toHaveBeenCalled();
  });

  it("classifies a dropped connection the browser will retry as transient", () => {
    const { source, sink } = setup();
    source.open();
    source.fail(CONNECTING);
    expect(sink.onError).toHaveBeenCalledWith("transient");
  });

  it("classifies a connection the browser gave up on as fatal", () => {
    const { source, sink } = setup();
    source.fail(CLOSED);
    expect(sink.onError).toHaveBeenCalledWith("fatal");
  });

  it("closes the underlying EventSource so the manager owns reconnection", () => {
    const { source, handle } = setup();
    handle.close();
    expect(source.closed).toBe(true);
  });

  it("delivers nothing after close", () => {
    const { source, sink, handle } = setup();
    handle.close();
    source.emit(
      "snapshot",
      JSON.stringify({ v: 1, viewId: "health", queryRevision: "r1", revision: "1", data: {} }),
    );
    source.fail(CLOSED);
    source.open();
    expect(sink.onEvent).not.toHaveBeenCalled();
    expect(sink.onError).not.toHaveBeenCalled();
    expect(sink.onOpen).not.toHaveBeenCalled();
  });

  it("is safe to close twice", () => {
    const { handle } = setup();
    handle.close();
    expect(() => handle.close()).not.toThrow();
  });
});
