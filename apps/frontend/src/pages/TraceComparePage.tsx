import { useSearch } from "@tanstack/react-router";
import TraceCompare from "./TraceCompare";

export default function TraceComparePage() {
  const { left, right } = useSearch({ from: "/traces/compare" });
  return <TraceCompare initialLeftTraceId={left ?? ""} initialRightTraceId={right ?? ""} />;
}
