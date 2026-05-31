import { useGlobalServiceFilter } from "../hooks/useGlobalServiceFilter";
import { ServiceMetricsWorkspace } from "../features/metrics/ServiceMetricsWorkspace";

export default function MetricsSearch() {
  const { service } = useGlobalServiceFilter();
  return (
    <ServiceMetricsWorkspace
      initialService={service ?? ""}
      lockedService={false}
    />
  );
}
