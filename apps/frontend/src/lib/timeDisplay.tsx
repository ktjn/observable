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
  { value: "iso-local-ms", label: "ISO8601 Client TZ [ms]" },
  { value: "iso-utc-ms",   label: "ISO8601 UTC [ms]" },
  { value: "iso-local-ns", label: "ISO8601 Client TZ [ns]" },
  { value: "iso-utc-ns",   label: "ISO8601 UTC [ns]" },
  { value: "unix-ms",      label: "Unix time [ms]" },
  { value: "unix-ns",      label: "Unix time [ns]" },
];

export const TIME_FORMAT_STORAGE_KEY = "observable.time-format";
export const DEFAULT_TIME_FORMAT: TimeFormat = "iso-local-ms";

interface TimeDisplayContextValue {
  format: TimeFormat;
  setFormat: (format: TimeFormat) => void;
}

const TimeDisplayContext = createContext<TimeDisplayContextValue | undefined>(undefined);

export function TimeDisplayProvider({ children }: { children: ReactNode }) {
  const [format, setFormatState] = useState<TimeFormat>(() => getStoredTimeFormat());

  const value = useMemo<TimeDisplayContextValue>(
    () => ({
      format,
      setFormat: (nextFormat) => {
        window.localStorage.setItem(TIME_FORMAT_STORAGE_KEY, nextFormat);
        setFormatState(nextFormat);
      },
    }),
    [format],
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
