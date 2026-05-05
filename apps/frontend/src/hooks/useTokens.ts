import { useQuery } from "@tanstack/react-query";
import { listTokens } from "../api/tokens";
import { useTenantContext } from "./useTenantContext";

export const TOKENS_QUERY_KEY = ["tokens"] as const;

/** Global cache of ingestion tokens — the authoritative source for registered environments and tenants. */
export function useTokens() {
  const { tenantId } = useTenantContext();
  return useQuery({
    queryKey: ["tokens", tenantId],
    queryFn: () => listTokens(tenantId),
    staleTime: 30_000,
  });
}

/** Distinct active environments derived from the token master. */
export function useRegisteredEnvironments(): string[] {
  const { data } = useTokens();
  if (!data) return [];
  const envs = Array.from(
    new Set(
      data.tokens
        .filter((t) => !t.revoked && t.environment)
        .map((t) => t.environment),
    ),
  ).sort();
  return envs;
}

/** Distinct tenants derived from the token master. */
export function useRegisteredTenants(): string[] {
  const { data } = useTokens();
  if (!data) return [];
  return Array.from(new Set(data.tokens.map((t) => t.tenant_name))).sort();
}
