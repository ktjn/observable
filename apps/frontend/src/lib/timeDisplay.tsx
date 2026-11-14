import { createContext, useContext, useMemo, useState } from "react";
import type { ReactNode } from "react";

export type TimeFormat =
  | "iso-local-ms"
  | "iso-utc-ms"
  | "iso-local-ns"
  | "iso-utc-ns"
  | "unix-ms"
  | "unix-ns";

export const TIME_FORMAT_OPTIONS: { value: TimeFormat; label: string }[] = [
  { value: "iso-local-ms", label: "Local time (ms)" },
  { value: "iso-utc-ms",   label: "UTC (ms)" },
  { value: "iso-local-ns", label: "Local time (ns)" },
  { value: "iso-utc-ns",   label: "UTC (ns)" },
  { value: "unix-ms",      label: "Unix timestamp (ms)" },
  { value: "unix-ns",      label: "Unix timestamp (ns)" },
];

export const TIME_FORMAT_STORAGE_KEY = "observable.time-format";
export const DISPLAY_DENSITY_STORAGE_KEY = "observable.display-density";
export const DEFAULT_TIME_FORMAT: TimeFormat = "iso-local-ms";
export type DisplayDensity = "comfortable" | "compact";

interface TimeDisplayContextValue {
  format: TimeFormat;
  setFormat: (format: TimeFormat) => void;
  density: DisplayDensity;
  setDensity: (density: DisplayDensity) => void;
}

const TimeDisplayContext = createContext<TimeDisplayContextValue | undefined>(undefined);

export function TimeDisplayProvider({ children }: { children: ReactNode }) {
  const [format, setFormatState] = useState<TimeFormat>(() => getStoredTimeFormat());
  const [density, setDensityState] = useState<DisplayDensity>(() => {
    const stored = window.localStorage.getItem(DISPLAY_DENSITY_STORAGE_KEY);
    return stored === "compact" ? "compact" : "comfortable";
  });

  const value = useMemo<TimeDisplayContextValue>(
    () => ({
      format,
      setFormat: (nextFormat) => {
        window.localStorage.setItem(TIME_FORMAT_STORAGE_KEY, nextFormat);
        setFormatState(nextFormat);
      },
      density,
      setDensity: (nextDensity) => {
        window.localStorage.setItem(DISPLAY_DENSITY_STORAGE_KEY, nextDensity);
        setDensityState(nextDensity);
      },
    }),
    [format, density],
  );

  return <TimeDisplayContext.Provider value={value}>{children}</TimeDisplayContext.Provider>;
}

export function useTimeDisplay(): TimeDisplayContextValue {
  const value = useContext(TimeDisplayContext);
  if (!value) {
    throw new Error("useTimeDisplay must be used inside TimeDisplayProvider");
  }
  return value;
}

export function getStoredTimeFormat(): TimeFormat {
  const stored = window.localStorage.getItem(TIME_FORMAT_STORAGE_KEY);
  return isTimeFormat(stored) ? stored : DEFAULT_TIME_FORMAT;
}

function isTimeFormat(value: string | null): value is TimeFormat {
  return (
    value === "iso-local-ms" ||
    value === "iso-utc-ms" ||
    value === "iso-local-ns" ||
    value === "iso-utc-ns" ||
    value === "unix-ms" ||
    value === "unix-ns"
  );
}
