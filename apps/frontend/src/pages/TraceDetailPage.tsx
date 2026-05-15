import { useParams } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { getTrace } from "../api/traces";
import { TraceDetail } from "./TraceDetail";
import { EmptyState } from "../components/ui/empty-state";
import { LoadingState } from "../components/ui/loading-state";
import { useTenantContext } from "../hooks/useTenantContext";

export default function TraceDetailPage() {
  const { traceId } = useParams({ from: "/traces/$traceId" });
  const { tenantId } = useTenantContext();
  const { data, isLoading } = useQuery({
    queryKey: ["trace", tenantId, traceId],
    queryFn: () => getTrace(tenantId, traceId),
  });
  if (isLoading) return <LoadingState>Loading trace…</LoadingState>;
  if (!data) return <EmptyState title="Trace not found." />;
  return <TraceDetail traceId={data.trace_id} spans={data.spans} events={data.events} />;
}
