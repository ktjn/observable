import { useGlobalDateRange, PRESET_OPTIONS, type CompareMode } from "../hooks/useGlobalDateRange";

export function GlobalDateRangePicker() {
  const { preset, fromMs, toMs, compare, setPreset, clearCustomRange, setCompare } = useGlobalDateRange();

  const compareSelect = (
    <select
      aria-label="Baseline comparison"
      className="context-pill themed-select"
      value={compare ?? ""}
      onChange={(e) => setCompare((e.target.value || null) as CompareMode | null)}
    >
      <option value="">No baseline</option>
      <option value="prev">vs Previous window</option>
      <option value="prev_day">vs Previous day</option>
      <option value="prev_week">vs Previous week</option>
    </select>
  );

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
        {compareSelect}
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2">
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
      {compareSelect}
    </div>
  );
}

function formatMs(ms: number): string {
  const d = new Date(ms);
  return `${d.getMonth() + 1}/${d.getDate()} ${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`;
}
