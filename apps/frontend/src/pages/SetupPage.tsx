import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Panel } from "../components/ui/panel";
import {
  getFirstSignalStatus,
  LOCAL_DEV_API_KEY,
  LOCAL_DEV_TENANT,
  LOCAL_DEV_TENANT_ID,
  OTLP_HTTP_TRACE_ENDPOINT,
  REDACTED_LOCAL_API_KEY,
} from "../api/setup";

export default function SetupPage() {
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");
  const { data, isLoading, refetch } = useQuery({
    queryKey: ["setup", "first-signal"],
    queryFn: getFirstSignalStatus,
  });

  const statusText = isLoading
    ? "Checking telemetry"
    : data?.state === "detected"
      ? "First signal detected"
      : data?.state === "error"
        ? "First signal check failed"
        : "Waiting for first signal";

  async function copyApiKey() {
    try {
      await navigator.clipboard.writeText(LOCAL_DEV_API_KEY);
      setCopyState("copied");
    } catch {
      setCopyState("failed");
    }
  }

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
              <dt>Tenant</dt>
              <dd>{LOCAL_DEV_TENANT}</dd>
            </div>
            <div>
              <dt>Tenant ID</dt>
              <dd>{LOCAL_DEV_TENANT_ID}</dd>
            </div>
            <div>
              <dt>OTLP HTTP traces</dt>
              <dd>{OTLP_HTTP_TRACE_ENDPOINT}</dd>
            </div>
            <div>
              <dt>API key</dt>
              <dd>{REDACTED_LOCAL_API_KEY}</dd>
            </div>
          </dl>
          <div className="setup-actions">
            <Button variant="secondary" onClick={() => void copyApiKey()}>
              Copy API key
            </Button>
            <span className="text-xs font-bold uppercase text-[var(--muted)]" role="status">
              {copyState === "copied"
                ? "Copied"
                : copyState === "failed"
                  ? "Copy unavailable"
                : "Redacted in the UI"}
            </span>
          </div>
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
