import { useCallback, useMemo, useState } from "react";

interface StoredColumnPreferences {
  columnOrder: string[];
  hiddenColumns: string[];
}

export interface ColumnPreferences {
  columnOrder: string[];
  visibleColumns: string[];
  toggleColumn: (key: string) => void;
  reorderColumns: (order: string[]) => void;
  applyColumns: (order: string[]) => void;
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function readStoredPreferences(storageKey: string): StoredColumnPreferences | null {
  try {
    const raw = window.localStorage.getItem(storageKey);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (
      typeof parsed !== "object" ||
      parsed === null ||
      !isStringArray((parsed as Record<string, unknown>).columnOrder) ||
      !isStringArray((parsed as Record<string, unknown>).hiddenColumns)
    ) {
      return null;
    }
    return parsed as StoredColumnPreferences;
  } catch {
    return null;
  }
}

/**
 * Persists column order and visibility to localStorage under `storageKey`,
 * seeded from `defaultOrder` the first time (or whenever stored data is missing or malformed).
 */
export function useColumnPreferences(storageKey: string, defaultOrder: readonly string[]): ColumnPreferences {
  const [state, setState] = useState<StoredColumnPreferences>(
    () => readStoredPreferences(storageKey) ?? { columnOrder: [...defaultOrder], hiddenColumns: [] },
  );

  const persist = useCallback(
    (next: StoredColumnPreferences) => {
      setState(next);
      window.localStorage.setItem(storageKey, JSON.stringify(next));
    },
    [storageKey],
  );

  const toggleColumn = useCallback(
    (key: string) => {
      persist(
        state.columnOrder.includes(key)
          ? {
              columnOrder: state.columnOrder,
              hiddenColumns: state.hiddenColumns.includes(key)
                ? state.hiddenColumns.filter((k) => k !== key)
                : [...state.hiddenColumns, key],
            }
          : { columnOrder: [...state.columnOrder, key], hiddenColumns: state.hiddenColumns.filter((k) => k !== key) },
      );
    },
    [persist, state],
  );

  const reorderColumns = useCallback(
    (order: string[]) => {
      persist({ columnOrder: order, hiddenColumns: state.hiddenColumns });
    },
    [persist, state.hiddenColumns],
  );

  const applyColumns = useCallback(
    (order: string[]) => {
      persist({ columnOrder: order, hiddenColumns: [] });
    },
    [persist],
  );

  const visibleColumns = useMemo(
    () => state.columnOrder.filter((key) => !state.hiddenColumns.includes(key)),
    [state],
  );

  return { columnOrder: state.columnOrder, visibleColumns, toggleColumn, reorderColumns, applyColumns };
}
