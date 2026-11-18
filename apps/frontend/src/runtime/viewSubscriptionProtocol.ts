/**
 * Wire protocol of `GET /v1/ui/subscriptions/events` as seen from the browser
 * (spec/09-api.md "Server-managed read-only view subscriptions").
 *
 * This module is pure: it parses and validates what arrives on the stream and
 * serializes what goes out on the `views` query parameter. It owns no connection
 * and no cache, which is what lets the real `EventSource` adapter and the
 * playground fake share exactly one notion of "a view subscription event".
 *
 * Validation is deliberately total — an event this module cannot vouch for
 * returns `null` rather than reaching the cache, because a half-understood
 * payload is indistinguishable from corruption (PUSH-019).
 */

/** Protocol version carried as `v` on every payload; mismatches are discarded. */
export const VIEW_SUBSCRIPTION_PROTOCOL_VERSION = 1;

export const VIEW_SUBSCRIPTION_PATH = "/v1/ui/subscriptions/events";

/** The SSE event names this client understands. */
export const VIEW_SUBSCRIPTION_EVENT_NAMES = [
  "snapshot",
  "delta",
  "replace",
  "invalidate",
  "reset",
] as const;

export type ViewSubscriptionEventName = (typeof VIEW_SUBSCRIPTION_EVENT_NAMES)[number];

/**
 * A rolling window keeps advancing its local clock; a fixed range is pinned
 * historical data, which never subscribes (PUSH-015, and the server rejects it).
 */
export type ViewTimeRange =
  | { mode: "rolling"; window: string }
  | { mode: "fixed"; from: number; to: number };

/** The allowlisted read-only view queries the server accepts. */
export type ViewSubscriptionQuery =
  | {
      kind: "service-health";
      serviceId: string;
      environmentClass: "production" | "non_production";
    }
  | { kind: "service-status"; environment?: string };

/** A keyed change taken verbatim from a domain event. */
export type ViewChange =
  | { op: "upsert"; key: string; value: unknown }
  | { op: "delete"; key: string };

export type ViewSubscriptionEvent =
  | {
      type: "snapshot";
      viewId: string;
      queryRevision: string;
      revision: string;
      data: unknown;
    }
  | {
      type: "replace";
      viewId: string;
      queryRevision: string;
      revision: string;
      data: unknown;
    }
  | {
      type: "delta";
      viewId: string;
      queryRevision: string;
      revision: string;
      changes: ViewChange[];
    }
  | { type: "invalidate"; viewId: string; queryRevision: string }
  | { type: "reset"; reason: string };

/**
 * One subscribed read-only view: what to ask the server for, and which query
 * cache entry its results belong to.
 *
 * `queryKey` is the TanStack Query key the view's data lives under, so a data
 * event is applied straight to the cache the consuming component already reads
 * from — no second GET (PUSH-004/PUSH-019).
 */
export interface ViewSubscriptionDefinition {
  viewId: string;
  query: ViewSubscriptionQuery;
  timeRange: ViewTimeRange;
  queryKey: readonly unknown[];
  /**
   * How to fold a `snapshot` or `replace` payload into this view's cached value.
   * Defaults to replacing the cached value with `data`. A view that enriches an
   * existing cached record (such as overlaying pushed service status or health
   * onto a summary query) can supply its own reducer; returning `undefined`
   * leaves the cache untouched until the base query resolves.
   */
  applyData?: (current: unknown, data: unknown) => unknown;
  /**
   * How to fold a `delta`'s keyed ops into this view's cached value. Defaults to
   * `applyKeyedChanges`, which treats the cached value as a keyed record.
   * A view whose cached shape is not a keyed record must supply its own reducer
   * or it falls back to an authoritative refetch.
   */
  applyDelta?: (current: unknown, changes: ViewChange[]) => unknown;
}

/** An open stream. Closing it must be idempotent. */
export interface ViewStreamHandle {
  close(): void;
}

/**
 * `transient` — the connection dropped and retrying is reasonable.
 * `fatal` — the transport will not retry on its own (a non-2xx registration
 * response, or a rejected event stream), so the session must be re-confirmed
 * before another attempt.
 */
export type ViewStreamErrorKind = "transient" | "fatal";

export interface ViewStreamHandlers {
  onOpen(): void;
  onEvent(event: ViewSubscriptionEvent): void;
  onError(kind: ViewStreamErrorKind): void;
}

/**
 * The seam between the view-subscription manager and a stream source. The real
 * `EventSource` adapter and the playground fake are interchangeable here, which
 * is what lets playground-mode components behave identically (PUSH-010).
 */
export interface ViewSubscriptionTransport {
  open(
    definitions: readonly ViewSubscriptionDefinition[],
    handlers: ViewStreamHandlers,
  ): ViewStreamHandle;
}

/** The `views` wire shape, stripped of browser-only concerns. */
interface WireViewDefinition {
  viewId: string;
  query: ViewSubscriptionQuery;
  timeRange: ViewTimeRange;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function nonEmptyString(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

function parseChanges(raw: unknown): ViewChange[] | null {
  if (!Array.isArray(raw)) return null;
  const changes: ViewChange[] = [];
  for (const entry of raw) {
    if (!isRecord(entry)) return null;
    const key = nonEmptyString(entry.key);
    if (key === null) return null;
    if (entry.op === "upsert") {
      changes.push({ op: "upsert", key, value: entry.value });
    } else if (entry.op === "delete") {
      changes.push({ op: "delete", key });
    } else {
      return null;
    }
  }
  return changes;
}

/**
 * Parses one SSE event into a validated `ViewSubscriptionEvent`, or `null` when
 * the event name, protocol version or required tags cannot be vouched for.
 */
export function parseViewSubscriptionEvent(
  eventName: string,
  rawData: string,
): ViewSubscriptionEvent | null {
  if (!(VIEW_SUBSCRIPTION_EVENT_NAMES as readonly string[]).includes(eventName)) {
    return null;
  }

  let payload: unknown;
  try {
    payload = JSON.parse(rawData);
  } catch {
    return null;
  }
  if (!isRecord(payload)) return null;
  if (payload.v !== VIEW_SUBSCRIPTION_PROTOCOL_VERSION) return null;

  if (eventName === "reset") {
    return { type: "reset", reason: nonEmptyString(payload.reason) ?? "unspecified" };
  }

  const viewId = nonEmptyString(payload.viewId);
  const queryRevision = nonEmptyString(payload.queryRevision);
  if (viewId === null || queryRevision === null) return null;

  if (eventName === "invalidate") {
    return { type: "invalidate", viewId, queryRevision };
  }

  const revision = nonEmptyString(payload.revision);
  if (revision === null) return null;

  if (eventName === "delta") {
    const changes = parseChanges(payload.changes);
    if (changes === null) return null;
    return { type: "delta", viewId, queryRevision, revision, changes };
  }

  if (!("data" in payload)) return null;
  return {
    type: eventName === "snapshot" ? "snapshot" : "replace",
    viewId,
    queryRevision,
    revision,
    data: payload.data,
  };
}

/**
 * Orders two result revisions. Revisions are u64 decimal strings on the wire, so
 * they are compared as `BigInt` rather than as numbers or lexically. Returns
 * `NaN` when either side is not a revision, which callers treat as "cannot prove
 * this is newer" and therefore discard.
 */
export function compareRevisions(a: string, b: string): number {
  let left: bigint;
  let right: bigint;
  try {
    left = BigInt(a);
    right = BigInt(b);
  } catch {
    return Number.NaN;
  }
  if (left === right) return 0;
  return left > right ? 1 : -1;
}

/**
 * Default `delta` reducer: folds keyed upsert/delete ops into a cached keyed
 * record, without mutating the cached object. Returns `undefined` when there is
 * nothing it can safely patch, which the manager escalates to an authoritative
 * refetch rather than guessing.
 */
export function applyKeyedChanges(current: unknown, changes: ViewChange[]): unknown {
  if (!isRecord(current)) return undefined;
  const next: Record<string, unknown> = { ...current };
  for (const change of changes) {
    if (change.op === "upsert") {
      next[change.key] = change.value;
    } else {
      delete next[change.key];
    }
  }
  return next;
}

function toWire(definition: ViewSubscriptionDefinition): WireViewDefinition {
  return {
    viewId: definition.viewId,
    query: definition.query,
    timeRange: definition.timeRange,
  };
}

/**
 * Serializes view definitions into the `views` parameter value. The output is a
 * stable function of the definitions, so an unchanged view set produces an
 * unchanged string and the manager does not reopen the stream (PUSH-005).
 */
export function encodeViewsParam(definitions: readonly ViewSubscriptionDefinition[]): string {
  return JSON.stringify(definitions.map(toWire));
}

/**
 * The stream URL for a view set. Credentials travel in the `HttpOnly` session
 * cookie; nothing identity-bearing is ever placed in this URL.
 */
export function viewSubscriptionUrl(
  definitions: readonly ViewSubscriptionDefinition[],
  path: string = VIEW_SUBSCRIPTION_PATH,
): string {
  return `${path}?views=${encodeURIComponent(encodeViewsParam(definitions))}`;
}

export function isPinnedTimeRange(timeRange: ViewTimeRange): boolean {
  return timeRange.mode === "fixed";
}

/**
 * Translates the existing global time-range selection (spec/05-frontend.md 9.8)
 * into a view time range: a relative preset is a rolling window, an absolute
 * custom range is pinned historical data.
 */
export function deriveViewTimeRange(range: {
  preset: string | null;
  fromMs?: number;
  toMs?: number;
}): ViewTimeRange {
  if (range.preset !== null) {
    return { mode: "rolling", window: range.preset };
  }
  return { mode: "fixed", from: range.fromMs ?? 0, to: range.toMs ?? 0 };
}
