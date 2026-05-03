import { ServiceMetricsWorkspace } from "../features/metrics/ServiceMetricsWorkspace";

export default function MetricsSearch() {
  return (
    <ServiceMetricsWorkspace
      initialService={new URLSearchParams(window.location.search).get("service") ?? ""}
      lockedService={false}
    />
  );
}
