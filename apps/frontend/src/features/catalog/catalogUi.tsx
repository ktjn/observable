import { Link } from "@tanstack/react-router";
import { keepPreviousData, useQuery, type UseQueryResult } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import { EmptyState } from "../../components/ui/empty-state";
import { ErrorState } from "../../components/ui/error-state";
import { LoadingState } from "../../components/ui/loading-state";
import { TablePanel } from "../../components/ui/table-panel";
import { useRuntime } from "../../hooks/useRuntime";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useAuth } from "../../hooks/useAuth";
import type {
  CatalogPrincipal, CatalogRuntime, CoverageStatus, EnvironmentClass, ExpectationStatus, HealthState,
} from "../../runtime/catalogContract";
import { environmentClasses } from "../../runtime/catalogContract";

export function useCatalogQuery<T>(
  key: readonly unknown[],
  read: (catalog: CatalogRuntime, tenant: string, signal: AbortSignal) => Promise<T>,
  refreshMs?: number,
) {
  const { catalog } = useRuntime();
  const { tenantId } = useTenantContext();
  const { data: user } = useAuth();
  return useQuery({
    queryKey: ["catalog", user?.user_id, tenantId, ...key],
    queryFn: ({ signal }) => read(catalog, tenantId, signal),
    enabled: Boolean(user),
    retry: false,
    gcTime: 0,
    // A refreshing view keeps what it shows until the new answer arrives.
    ...(refreshMs ? { refetchInterval: refreshMs, refetchIntervalInBackground: false, placeholderData: keepPreviousData } : {}),
  });
}

export function CatalogResult<T>({
  query, label, children,
}: {
  query: UseQueryResult<T>;
  label: string;
  children: (data: T) => ReactNode;
}) {
  if (query.isFetching || query.isPending) {
    return <div role="status"><LoadingState>Loading {label}…</LoadingState></div>;
  }
  if (query.isError) {
    return <ErrorState title={`Could not load ${label}`} description="Try again to request current information."
      actions={<Button variant="secondary" onClick={() => void query.refetch()}>Retry</Button>} />;
  }
  return children(query.data);
}

export function CatalogTable({
  title, headings, rows, empty,
}: {
  title: string;
  headings: string[];
  rows: { id: string; cells: ReactNode[] }[];
  empty?: string;
}) {
  if (!rows.length) return <EmptyState compact title={empty ?? `No ${title.toLowerCase()}`} />;
  return (
    <TablePanel>
      <table aria-label={title} className="w-full text-left">
        <thead><tr>{headings.map((heading) => <th key={heading} scope="col">{heading}</th>)}</tr></thead>
        <tbody>{rows.map(({ id, cells }) => (
          <tr key={id}>{cells.map((cell, index) => <td key={headings[index]} className="break-words">{cell}</td>)}</tr>
        ))}</tbody>
      </table>
    </TablePanel>
  );
}

export function ServiceLink({ id, children }: { id: string; children: ReactNode }) {
  return <Link to="/catalog/services/$serviceId" params={{ serviceId: id }} className="font-semibold underline">{children}</Link>;
}

export function ProviderLink({ id, children }: { id: string; children: ReactNode }) {
  return <Link to="/catalog/providers/$providerId" params={{ providerId: id }} className="font-semibold underline">{children}</Link>;
}

export function labelOf(value: string) {
  return value.charAt(0).toUpperCase() + value.slice(1).replace(/_/g, " ");
}

export function principalName(principal: CatalogPrincipal | undefined | null) {
  return principal
    ? `${principal.team_name || principal.principal_id} (${labelOf(principal.state)})`
    : "Unassigned";
}

export function CoverageLabel({ status }: { status: CoverageStatus }) {
  const tone = { covered: "good", partial: "warn", uncovered: "bad", unknown: "neutral" } as const;
  return <Badge role="presentation" tone={tone[status]} className="text-xs normal-case">{labelOf(status)}</Badge>;
}

const healthTone = { normal: "good", degraded: "warn", unavailable: "bad", planned_maintenance: "info", unknown: "neutral" } as const;
const expectationTone = { compliant: "good", at_risk: "warn", breached: "bad", unknown: "neutral" } as const;

/** Evaluated health; unknown is never toned like a healthy state. */
export function HealthLabel({ state }: { state: HealthState }) {
  return <Badge role="presentation" tone={healthTone[state] ?? "neutral"} className="text-xs normal-case">{labelOf(state)}</Badge>;
}

export function ExpectationLabel({ status }: { status: ExpectationStatus }) {
  return <Badge role="presentation" tone={expectationTone[status] ?? "neutral"} className="text-xs normal-case">{labelOf(status)}</Badge>;
}

const reasons: Record<string, string> = {
  current: "Current evidence",
  missing: "No evidence",
  stale: "Stale evidence",
  restricted: "Restricted evidence",
  evaluation_unavailable: "Evaluation unavailable",
  no_approved_objective: "No approved objective",
};

export function reasonOf(reason: string | null | undefined) {
  return reason ? reasons[reason] ?? labelOf(reason) : "—";
}

export const classLabels: Record<EnvironmentClass, string> = {
  production: "Production",
  non_production: "Non-production",
};

export function EnvironmentClassPicker({ value, onChange }: { value: EnvironmentClass; onChange: (value: EnvironmentClass) => void }) {
  return (
    <div role="group" aria-label="Environment class" className="flex flex-wrap gap-2">
      {environmentClasses.map((environmentClass) => (
        <Button key={environmentClass} variant={environmentClass === value ? "primary" : "secondary"}
          aria-pressed={environmentClass === value} onClick={() => onChange(environmentClass)}>
          {classLabels[environmentClass]}
        </Button>
      ))}
    </div>
  );
}
