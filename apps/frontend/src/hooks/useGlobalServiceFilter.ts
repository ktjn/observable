import { useNavigate, useSearch } from "@tanstack/react-router";
import type { RootSearch } from "../router";

/**
 * Validates and normalises a raw search param value to a trimmed string
 * or undefined. Used by validateSearch in router.ts and by this hook.
 */
export function normalizeService(raw: unknown): string | undefined {
  return typeof raw === "string" && raw.trim() ? raw.trim() : undefined;
}

export interface GlobalServiceFilter {
  service: string | undefined;
  setService: (next: string | undefined) => void;
}

/**
 * Read and write the global service filter from the root route's URL search
 * params. Mirrors useGlobalDateRange — setting the service preserves the
 * existing date range params, and vice versa.
 */
export function useGlobalServiceFilter(): GlobalServiceFilter {
  const search = useSearch({ strict: false }) as RootSearch;
  const navigate = useNavigate();

  const setService = (next: string | undefined) => {
    navigate({
      search: (prev: RootSearch) => ({
        ...prev,
        service: normalizeService(next),
      }),
    } as unknown as Parameters<typeof navigate>[0]);
  };

  return { service: search.service, setService };
}
