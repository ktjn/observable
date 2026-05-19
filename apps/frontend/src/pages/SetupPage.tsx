import { useQuery } from "@tanstack/react-query";
import { Button } from "../components/ui/button";
import { Panel } from "../components/ui/panel";
import { Badge } from "../components/ui/badge";
import {
  getFirstSignalStatus,
  OTLP_GRPC_ENDPOINT,
  OTLP_HTTP_JSON_LOGS,
  OTLP_HTTP_JSON_METRICS,
  OTLP_HTTP_JSON_TRACES,
} from "../api/setup";
import { useTenantContext } from "../hooks/useTenantContext";

export default function SetupPage() {
  const { tenantId } = useTenantContext();
  const { data, isLoading, refetch } = useQuery({
    queryKey: ["setup", "first-signal", tenantId],
    queryFn: () => getFirstSignalStatus(tenantId),
  });

  const statusText = isLoading
    ? "Checking telemetry"
    : data?.state === "detected"
      ? "First signal detected"
      : data?.state === "error"
        ? "First signal check failed"
        : "Waiting for first signal";

  return (
    <section className="page-stack" aria-labelledby="setup-heading">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Onboarding</div>
          <h1 id="setup-heading">Setup</h1>
        </div>
        <Button variant="secondary" onClick={() => void refetch()}>
          Recheck
        </Button>
      </div>

      <div className="detail-grid">
        <Panel eyebrow="Local ingest" title="Collector endpoint">
          <dl className="definition-grid">
            <div>
              <dt>OTLP gRPC Ingestion URL</dt>
              <dd><code className="text-xs">{OTLP_GRPC_ENDPOINT}</code></dd>
            </div>
            <div>
              <dt>OTLP HTTP/JSON Traces</dt>
              <dd><code className="text-xs">{OTLP_HTTP_JSON_TRACES}</code></dd>
            </div>
            <div>
              <dt>OTLP HTTP/JSON Metrics</dt>
              <dd><code className="text-xs">{OTLP_HTTP_JSON_METRICS}</code></dd>
            </div>
            <div>
              <dt>OTLP HTTP/JSON Logs</dt>
              <dd><code className="text-xs">{OTLP_HTTP_JSON_LOGS}</code></dd>
            </div>
          </dl>
        </Panel>

        <Panel
          eyebrow="Validation"
          title="First signal"
          actions={
            <Badge tone={data?.state === "detected" ? "good" : "warn"}>
              {statusText}
            </Badge>
          }
        >
          <dl className="definition-grid">
            <div>
              <dt>Traces</dt>
              <dd>{data?.traces ?? 0}</dd>
            </div>
            <div>
              <dt>Logs</dt>
              <dd>{data?.logs ?? 0}</dd>
            </div>
            <div>
              <dt>Metrics</dt>
              <dd>{data?.metrics ?? 0}</dd>
            </div>
          </dl>
        </Panel>
      </div>
    </section>
  );
}
