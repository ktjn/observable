import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { getTopology, listServices } from "../api/services";
import { TopologyMap } from "../components/topology/TopologyMap";
import type { TopologyEdge } from "../api/services";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/empty-state";
import { ErrorState } from "../components/ui/error-state";
import { LoadingState } from "../components/ui/loading-state";
import { TablePanel } from "../components/ui/table-panel";
import { QueryInput } from "../features/nlq/QueryInput";
import { deriveViewFiltersFromIr, type NlqIrLike } from "../features/nlq/queryFilters";
import { useTenantContext } from "../hooks/useTenantContext";
import { LogExplorer } from "./LogSearch";

const TOPOLOGY_BASE_IR: NlqIrLike = {
  operation: "catalog",
  signals: ["metrics"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};

export default function ServiceTopologyPage() {
  const [environment, setEnvironment] = useState<string>("all");
  const [focusedService, setFocusedService] = useState<string | null>(null);
  const [edgePopover, setEdgePopover] = useState<{
    edge: TopologyEdge;
    x: number;
    y: number;
  } | null>(null);
  const { tenantId } = useTenantContext();

  const { data, isLoading, error } = useQuery({
    queryKey: ["topology", tenantId, environment],
    queryFn: () =>
      getTopology(tenantId, { environment: environment === "all" ? undefined : environment }),
  });

  const { data: servicesData } = useQuery({
    queryKey: ["services", tenantId],
    queryFn: () => listServices(tenantId),
  });

  const allServiceNames = (servicesData?.items ?? []).filter((s) => s !== "");

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Topology</div>
          <h1>Service Overview</h1>
        </div>
      </div>

      <div className="toolbar-row">
        <QueryInput
          baseIr={TOPOLOGY_BASE_IR}
          placeholder='Focus topology, e.g. "prod payments service"'
          onIr={(ir) => {
            const filters = deriveViewFiltersFromIr(ir, "topology");
            setEnvironment(filters.environment ?? "all");
            setFocusedService(filters.service ?? null);
            setEdgePopover(null);
          }}
        />
      </div>

      {focusedService && (
        <div className="flex gap-4 items-center py-2">
          <Button variant="secondary" onClick={() => setFocusedService(null)}>
            ← All services
          </Button>
          <span>Viewing: {focusedService}</span>
          <Link to="/services/$serviceId" params={{ serviceId: focusedService }}>
            → Service detail
          </Link>
        </div>
      )}

      <TablePanel className="overflow-hidden relative bg-[var(--surface-inset)] h-[calc(100vh-12rem)]">
        {isLoading ? (
          <LoadingState>Loading topology…</LoadingState>
        ) : error ? (
          <ErrorState title="Failed to load topology" description={String(error)} />
        ) : (
          <div className="relative h-full w-full flex flex-col">
            {/* Popover uses SVG midpoint coordinates transformed by zoom. */}
            {edgePopover && (
              <div
                style={{
                  position: "absolute",
                  left: edgePopover.x,
                  top: edgePopover.y,
                  zIndex: 10,
                  background: "var(--bg-surface, #1a1a1a)",
                  border: "1px solid var(--border, #444)",
                  borderRadius: "4px",
                  padding: "0.5rem",
                  display: "flex",
                  flexDirection: "column",
                  gap: "0.25rem",
                }}
              >
                <a
                  href={`/traces?caller=${encodeURIComponent(edgePopover.edge.caller)}&callee=${encodeURIComponent(edgePopover.edge.callee)}`}
                >
                  View Traces
                </a>
                <a
                  href={`/logs?service=${encodeURIComponent(edgePopover.edge.caller)}`}
                >
                  View Logs
                </a>
              </div>
            )}
            {allServiceNames.length === 0 ? (
              <EmptyState title="No services found" description="No services found in the selected time range." />
            ) : (
              <div className="flex flex-col flex-1 gap-2 min-h-0">
                {(!data || data.edges.length === 0) && (
                  <p className="text-xs text-[var(--muted)] shrink-0">
                    No observed call relationships yet — services shown as standalone nodes.
                  </p>
                )}
                <div className="flex-1 min-h-0">
                  <TopologyMap
                    edges={data?.edges ?? []}
                    allServices={allServiceNames}
                    focusedService={focusedService}
                    onNodeClick={(svc) => {
                      setEdgePopover(null);
                      setFocusedService((prev) => (prev === svc ? null : svc));
                    }}
                    onEdgeClick={(edge, x, y) => setEdgePopover({ edge, x, y })}
                    onBackgroundClick={() => setEdgePopover(null)}
                  />
                </div>
              </div>
            )}
          </div>
        )}
      </TablePanel>

      {focusedService && (
        <section aria-label="Focused service logs">
          <LogExplorer
            key={focusedService}
            initialService={focusedService}
            lockedService
            showHeader={false}
            showServiceColumn={false}
            showPromote={false}
            tableAriaLabel="Focused service logs"
          />
        </section>
      )}
    </section>
  );
}
