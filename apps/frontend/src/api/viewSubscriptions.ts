import {
  VIEW_SUBSCRIPTION_EVENT_NAMES,
  extractStreamIdFromEventId,
  parseViewSubscriptionEvent,
  viewSubscriptionUrl,
} from "../runtime/viewSubscriptionProtocol";
import type {
  ViewAckItem,
  ViewStreamHandle,
  ViewStreamHandlers,
  ViewSubscriptionDefinition,
  ViewSubscriptionTransport,
} from "../runtime/viewSubscriptionProtocol";
import { me } from "./auth";

/**
 * The production stream source: a same-origin native `EventSource` against
 * `GET /v1/ui/subscriptions/events`, authenticated by the `HttpOnly` session
 * cookie. View definitions travel on this same request — there is no separate
 * registration call, and reopening the GET *is* re-registration.
 *
 * Reconnection policy deliberately does not live here. On any error the adapter
 * reports a classified failure and the view-subscription manager decides whether
 * to retry, so backoff, staleness and the authorization recheck are one
 * transport-independent policy shared with the playground fake.
 */
export function createEventSourceViewSubscriptionTransport(
  EventSourceCtor?: typeof EventSource,
): ViewSubscriptionTransport {
  return {
    open(
      definitions: readonly ViewSubscriptionDefinition[],
      handlers: ViewStreamHandlers,
    ): ViewStreamHandle {
      // Resolved per open, not at module load: `EventSource` is absent in the
      // jsdom test environment, where this transport is never opened.
      const Ctor = EventSourceCtor ?? globalThis.EventSource;
      if (!Ctor) {
        return {
          close() {},
        };
      }
      const source = new Ctor(viewSubscriptionUrl(definitions));
      let closed = false;

      source.onopen = () => {
        if (!closed) handlers.onOpen();
      };

      for (const name of VIEW_SUBSCRIPTION_EVENT_NAMES) {
        source.addEventListener(name, (event) => {
          if (closed) return;
          const msg = event as MessageEvent<string>;
          if (typeof msg.lastEventId === "string" && msg.lastEventId.length > 0) {
            const streamId = extractStreamIdFromEventId(msg.lastEventId);
            if (streamId !== null) {
              handlers.onStreamId?.(streamId);
            }
          }
          const parsed = parseViewSubscriptionEvent(name, msg.data);
          if (parsed !== null) handlers.onEvent(parsed);
        });
      }

      source.onerror = () => {
        if (closed) return;
        // `CLOSED` means the browser will not retry by itself — a non-2xx
        // registration response (401/403/503) or a rejected event stream — so
        // the session has to be re-established before another attempt.
        handlers.onError(source.readyState === 2 ? "fatal" : "transient");
      };

      return {
        close() {
          if (closed) return;
          closed = true;
          source.close();
        },
      };
    },

    async ack(streamId: string, acks: readonly ViewAckItem[]): Promise<void> {
      if (acks.length === 0 || !streamId) return;
      await fetch(`/v1/ui/subscriptions/${encodeURIComponent(streamId)}/ack`, {
        method: "POST",
        credentials: "include",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ acks }),
      });
    },
  };
}

/**
 * Resolves the authoritative active session tenant. Returns `null` when the
 * session does not authorize anything, so the manager fails closed instead of
 * reconnect-looping an unauthorized stream.
 */
export async function confirmSessionTenant(): Promise<string | null> {
  try {
    const user = await me();
    return user.active_tenant_id ?? (user.tenants?.length === 1 ? user.tenants[0]?.tenant_id ?? null : null);
  } catch {
    return null;
  }
}

export const httpViewSubscriptions = {
  transport: createEventSourceViewSubscriptionTransport(),
  confirmTenant: confirmSessionTenant,
};
