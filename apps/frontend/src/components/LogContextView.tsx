import { useQuery } from "@tanstack/react-query";
import { getLogContext } from "../api/logs";
import { Button } from "./ui/button";
import { useTimeDisplay } from "../lib/timeDisplay";
import { LogList } from "./shared/LogList";
import { useTenantContext } from "../hooks/useTenantContext";

interface Props {
  logId: string;
  onClose: () => void;
}

export function LogContextView({ logId, onClose }: Props) {
  const { tenantId } = useTenantContext();
  const { data, isLoading } = useQuery({
    queryKey: ["logs", "context", tenantId, logId],
    queryFn: () => getLogContext(tenantId, logId),
  });
  const { format } = useTimeDisplay();

  return (
    <div className="mt-3 p-3 bg-[var(--surface-inset)] border border-[var(--border)]">
      <div className="flex justify-between items-center mb-3">
        <h4 className="m-0 text-sm font-bold text-[var(--text-strong)]">Surrounding Logs</h4>
        <Button variant="secondary" onClick={onClose}>Close</Button>
      </div>
      <LogList
        logs={data?.logs ?? []}
        loading={isLoading}
        pivotId={logId}
        showTraceLink
        timeFormat={format}
      />
    </div>
  );
}
