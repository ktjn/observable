import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useReadonlyViewSubscription } from "../hooks/useReadonlyViewSubscription";
import { playgroundViewSubscriptions } from "../runtime/playgroundViewSubscriptions";
import type { ViewSubscriptionDefinition } from "../runtime/viewSubscriptionProtocol";

/**
 * Browser-only harness for read-only view subscriptions (PUSH-010) — not a
 * product page. It renders a pushed view through exactly the same hook, manager
 * and transport contract a production view uses, with the playground fake as the
 * stream source, and exposes the fake's control surface on `window` so an e2e
 * run can script snapshot/delta/disconnect/tenant-switch scenarios with no
 * backend and no broker.
 *
 * Registered only in playground builds (see `router.ts`).
 */

const VIEW_ID = "service-status";
const VIEW_QUERY_KEY = ["playground-view", VIEW_ID] as const;

const control = playgroundViewSubscriptions.control;

// Seeded at module scope so the snapshot is scripted before React can open the
// first connection.
control.setSnapshotData(VIEW_ID, {
  checkout: { health: "normal" },
  payments: { health: "normal" },
});

const VIEWS: ViewSubscriptionDefinition[] = [
  {
    viewId: VIEW_ID,
    query: { kind: "service-status" },
    timeRange: { mode: "rolling", window: "15m" },
    queryKey: VIEW_QUERY_KEY,
  },
];

type ServiceRows = Record<string, { health: string }>;

declare global {
  interface Window {
    __observableViewSubscriptions?: typeof control;
  }
}

export default function PlaygroundViewSubscriptions() {
  const [tenantId, setTenantId] = useState(() => control.sessionTenantId());
  const [selectedRow, setSelectedRow] = useState<string | null>(null);
  const [note, setNote] = useState("");

  useEffect(() => {
    window.__observableViewSubscriptions = control;
    return control.onTenantChange(setTenantId);
  }, []);

  const { status, lastResetReason } = useReadonlyViewSubscription({ tenantId, views: VIEWS });

  // Data reaches this view only through the stream: the query is never fetched,
  // it just reads the cache the subscription writes into.
  const { data } = useQuery<ServiceRows>({
    queryKey: VIEW_QUERY_KEY,
    queryFn: async () => ({}),
    enabled: false,
  });

  const rows = Object.entries(data ?? {});

  return (
    <div style={{ padding: "2rem", fontFamily: "monospace" }}>
      <h1>Read-only view subscription</h1>
      <p>
        status: <span data-testid="subscription-status">{status}</span>
      </p>
      <p>
        tenant: <span data-testid="subscription-tenant">{tenantId}</span>
      </p>
      <p>
        reset reason: <span data-testid="subscription-reset-reason">{lastResetReason ?? "none"}</span>
      </p>
      <p>
        selected: <span data-testid="selected-row">{selectedRow ?? "none"}</span>
      </p>
      <label>
        note (unsaved form state):{" "}
        <input
          data-testid="unsaved-note"
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
      </label>
      <ul data-testid="service-rows">
        {rows.map(([service, value]) => (
          <li key={service} data-testid={`row-${service}`}>
            <button type="button" onClick={() => setSelectedRow(service)}>
              {service}
            </button>
            : <span data-testid={`health-${service}`}>{value.health}</span>
          </li>
        ))}
      </ul>
      <p data-testid="row-count">{rows.length}</p>
    </div>
  );
}
