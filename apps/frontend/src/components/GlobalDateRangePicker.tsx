import { useGlobalDateRange, PRESET_OPTIONS } from "../hooks/useGlobalDateRange";

export function GlobalDateRangePicker() {
  const { preset, fromMs, toMs, setPreset, clearCustomRange } = useGlobalDateRange();

  if (preset === null) {
    return (
      <div className="flex items-center gap-2">
        <span className="context-pill font-mono text-xs">
          {formatMs(fromMs)} → {formatMs(toMs)}
        </span>
        <button
          type="button"
          className="context-pill"
          style={{ cursor: "pointer" }}
          aria-label="Reset time range"
          onClick={clearCustomRange}
        >
          Reset range
        </button>
      </div>
    );
  }

  return (
    <select
      aria-label="Global time range"
      className="context-pill themed-select"
      value={preset}
      onChange={(e) => setPreset(e.target.value as typeof preset)}
    >
      {PRESET_OPTIONS.map((opt) => (
        <option key={opt.value} value={opt.value}>{opt.label}</option>
      ))}
    </select>
  );
}

function formatMs(ms: number): string {
  const d = new Date(ms);
  return `${d.getMonth() + 1}/${d.getDate()} ${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`;
}
