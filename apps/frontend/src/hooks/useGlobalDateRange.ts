import { useMemo } from "react";
import { useNavigate, useSearch } from "@tanstack/react-router";
import type { CompareMode, Preset, RootSearch } from "../router";
export { DEFAULT_PRESET } from "../router";
export type { CompareMode, Preset } from "../router";

export type PresetOption = { value: Preset; label: string };

export const PRESET_OPTIONS: PresetOption[] = [
  { value: "5m",  label: "Last 5 min" },
  { value: "15m", label: "Last 15 min" },
  { value: "30m", label: "Last 30 min" },
  { value: "1h",  label: "Last 1 hour" },
  { value: "3h",  label: "Last 3 hours" },
  { value: "12h", label: "Last 12 hours" },
  { value: "24h", label: "Last 24 hours" },
  { value: "7d",  label: "Last 7 days" },
  { value: "30d", label: "Last 30 days" },
];

const PRESET_MS: Record<Preset, number> = {
  "5m":  5  * 60 * 1000,
  "15m": 15 * 60 * 1000,
  "30m": 30 * 60 * 1000,
  "1h":  60 * 60 * 1000,
  "3h":  3  * 60 * 60 * 1000,
  "12h": 12 * 60 * 60 * 1000,
  "24h": 24 * 60 * 60 * 1000,
  "7d":  7  * 24 * 60 * 60 * 1000,
  "30d": 30 * 24 * 60 * 60 * 1000,
};

export function presetToMs(preset: Preset): number {
  return PRESET_MS[preset];
}

export function deriveRange(search: Partial<RootSearch>): { fromMs: number; toMs: number } {
  if (search.from != null && search.to != null) {
    return { fromMs: search.from, toMs: search.to };
  }
  const preset = search.preset ?? "1h";
  const toMs = Date.now();
  return { fromMs: toMs - presetToMs(preset), toMs };
}

export function deriveCompareRange(
  fromMs: number,
  toMs: number,
  search: Partial<RootSearch>,
): { fromMs: number; toMs: number; mode: CompareMode } | null {
  const mode = search.compare;
  if (!mode) return null;
  const span = Math.max(1, toMs - fromMs);
  if (mode === "prev") {
    return { fromMs: fromMs - span, toMs: fromMs, mode };
  }
  if (mode === "prev_day") {
    const day = 24 * 60 * 60 * 1000;
    return { fromMs: fromMs - day, toMs: toMs - day, mode };
  }
  if (mode === "prev_week") {
    const week = 7 * 24 * 60 * 60 * 1000;
    return { fromMs: fromMs - week, toMs: toMs - week, mode };
  }
  if (mode === "custom" && search.compareFrom != null && search.compareTo != null) {
    return { fromMs: search.compareFrom, toMs: search.compareTo, mode };
  }
  return null;
}

export interface GlobalDateRange {
  preset: Preset | null;
  fromMs: number;
  toMs: number;
  compare: CompareMode | null;
  compareRange: { fromMs: number; toMs: number; mode: CompareMode } | null;
  setPreset: (p: Preset) => void;
  setCustomRange: (from: number, to: number) => void;
  clearCustomRange: () => void;
  setCompare: (mode: CompareMode | null, compareFrom?: number, compareTo?: number) => void;
}

export function useGlobalDateRange(): GlobalDateRange {
  const search = useSearch({ strict: false }) as RootSearch;
  const navigate = useNavigate();
  const updateSearch = (nextSearch: Partial<RootSearch>) => {
    navigate({
      search: (prev: RootSearch) => ({ ...prev, ...nextSearch }),
    } as unknown as Parameters<typeof navigate>[0]);
  };

  const isCustom = search.from != null && search.to != null;
  const preset: Preset | null = isCustom ? null : (search.preset ?? "1h");

  const { fromMs, toMs } = useMemo(() => deriveRange(search), [search.preset, search.from, search.to]);
  const compareRange = useMemo(
    () => deriveCompareRange(fromMs, toMs, search),
    [fromMs, toMs, search.compare, search.compareFrom, search.compareTo],
  );

  function setPreset(p: Preset) {
    updateSearch({
      preset: p,
      from: undefined,
      to: undefined,
    });
  }

  function setCustomRange(from: number, to: number) {
    updateSearch({
      preset: undefined,
      from,
      to,
    });
  }

  function clearCustomRange() {
    updateSearch({
      preset: "1h",
      from: undefined,
      to: undefined,
    });
  }

  function setCompare(mode: CompareMode | null, compareFrom?: number, compareTo?: number) {
    updateSearch({
      compare: mode ?? undefined,
      compareFrom: mode === "custom" ? compareFrom : undefined,
      compareTo: mode === "custom" ? compareTo : undefined,
    });
  }

  return {
    preset,
    fromMs,
    toMs,
    compare: search.compare ?? null,
    compareRange,
    setPreset,
    setCustomRange,
    clearCustomRange,
    setCompare,
  };
}
