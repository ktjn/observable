import { useParams } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { getTrace } from "../api/traces";
import { TraceDetail } from "./TraceDetail";

export default function TraceDetailPage() {
  const { traceId } = useParams({ from: "/traces/$traceId" });
  const { data, isLoading } = useQuery({
    queryKey: ["trace", traceId],
    queryFn: () => getTrace(traceId),
  });
  if (isLoading) return <p>Loading…</p>;
  if (!data) return <p>Not found</p>;
  return <TraceDetail traceId={data.trace_id} spans={data.spans} />;
}
