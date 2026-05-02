import { useMemo, useState } from "react";

export interface UseSignalSearchOptions {
  initialService?: string;
  initialLookbackMinutes?: number;
}

export interface UseSignalSearchResult {
  service: string;
  setService: (service: string) => void;
  lookbackMinutes: number;
  setLookbackMinutes: (minutes: number) => void;
  customRangeMs: { fromMs: number; toMs: number } | null;
  handleHistogramRangeSelect: (fromMs: number, toMs: number) => void;
  handleClearRange: () => void;
  from: string;
  to: string | undefined;
  histogramFromMs: number;
  histogramToMs: number;
}

export function useSignalSearch({
  initialService = "",
  initialLookbackMinutes = 60,
}: UseSignalSearchOptions = {}): UseSignalSearchResult {
  const [service, setService] = useState(initialService);
  const [lookbackMinutes, setLookbackMinutes] = useState(initialLookbackMinutes);
  const [customRangeMs, setCustomRangeMs] = useState<{ fromMs: number; toMs: number } | null>(null);

  const { from, to, histogramFromMs, histogramToMs } = useMemo(() => {
    if (customRangeMs) {
      return {
        from: new Date(customRangeMs.fromMs).toISOString(),
        to: new Date(customRangeMs.toMs).toISOString(),
        histogramFromMs: customRangeMs.fromMs,
        histogramToMs: customRangeMs.toMs,
      };
    }
    const toMs = Date.now();
    const fromMs = toMs - lookbackMinutes * 60 * 1000;
    return {
      from: new Date(fromMs).toISOString(),
      to: undefined as string | undefined,
      histogramFromMs: fromMs,
      histogramToMs: toMs,
    };
  }, [customRangeMs, lookbackMinutes]);

  function handleHistogramRangeSelect(fromMs: number, toMs: number) {
    setCustomRangeMs({ fromMs, toMs });
  }

  function handleClearRange() {
    setCustomRangeMs(null);
  }

  return {
    service,
    setService,
    lookbackMinutes,
    setLookbackMinutes,
    customRangeMs,
    handleHistogramRangeSelect,
    handleClearRange,
    from,
    to,
    histogramFromMs,
    histogramToMs,
  };
}
