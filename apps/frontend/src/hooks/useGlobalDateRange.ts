import { useMemo } from "react";
import { useNavigate, useSearch } from "@tanstack/react-router";
import type { Preset, RootSearch } from "../router";
export { DEFAULT_PRESET } from "../router";
export type { Preset } from "../router";

export type PresetOption = { value: Preset; label: string };

export const PRESET_OPTIONS: PresetOption[] = [
  { value: "5m",  label: "Last 5 min" },
  { value: "15m", label: "Last 15 min" },
  { value: "30m", label: "Last 30 min" },
  { value: "1h",  label: "Last 1 hour" },
  { value: "3h",  label: "Last 3 hours" },
  { value: "12h", label: "Last 12 hours" },
];

const PRESET_MS: Record<Preset, number> = {
  "5m":  5  * 60 * 1000,
  "15m": 15 * 60 * 1000,
  "30m": 30 * 60 * 1000,
  "1h":  60 * 60 * 1000,
  "3h":  3  * 60 * 60 * 1000,
  "12h": 12 * 60 * 60 * 1000,
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

export interface GlobalDateRange {
  preset: Preset | null;
  fromMs: number;
  toMs: number;
  setPreset: (p: Preset) => void;
  setCustomRange: (from: number, to: number) => void;
  clearCustomRange: () => void;
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

  return { preset, fromMs, toMs, setPreset, setCustomRange, clearCustomRange };
}
