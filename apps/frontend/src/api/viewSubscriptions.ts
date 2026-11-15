import {
  VIEW_SUBSCRIPTION_EVENT_NAMES,
  parseViewSubscriptionEvent,
  viewSubscriptionUrl,
} from "../runtime/viewSubscriptionProtocol";
import type {
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
      const source = new Ctor(viewSubscriptionUrl(definitions));
      let closed = false;

      source.onopen = () => {
        if (!closed) handlers.onOpen();
      };

      for (const name of VIEW_SUBSCRIPTION_EVENT_NAMES) {
        source.addEventListener(name, (event) => {
          if (closed) return;
          const parsed = parseViewSubscriptionEvent(name, (event as MessageEvent<string>).data);
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
    return user.active_tenant_id ?? null;
  } catch {
    return null;
  }
}

export const httpViewSubscriptions = {
  transport: createEventSourceViewSubscriptionTransport(),
  confirmTenant: confirmSessionTenant,
};
