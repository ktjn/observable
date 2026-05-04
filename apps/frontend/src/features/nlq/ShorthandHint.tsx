const ROWS: [string, string][] = [
  ["m:<name>", "metric name"],
  ["f:<field>:<val>", "filter (explicit)"],
  ["<field>:<val>", "filter (shorthand)"],
  ["op:<type>", "operation override"],
  ['"quoted text"', "search term (exact)"],
  ["unquoted word", "search term"],
];

/**
 * Hover-triggered tooltip explaining the '/' shorthand syntax (ADR-029).
 * CSS-only — no JS required.
 */
export function ShorthandHint() {
  return (
    <div className="relative group shrink-0">
      <button
        type="button"
        aria-label="Shorthand syntax reference"
        className="flex h-8 w-8 items-center justify-center rounded text-xs text-[var(--muted)] hover:bg-[var(--surface-subtle)] hover:text-[var(--text)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)]"
      >
        ?
      </button>

      {/* Tooltip — opens on hover via group-hover, pointer-events-none so it doesn't steal hover */}
      <div
        className="pointer-events-none absolute bottom-full right-0 z-20 mb-1 hidden w-72 group-hover:block"
        role="tooltip"
      >
        <div className="rounded border border-[var(--border)] bg-[var(--surface-raised)] p-3 text-xs shadow-lg">
          <p className="mb-1 font-bold text-[var(--text-strong)]">Shorthand syntax</p>
          <p className="mb-2 text-[var(--muted)]">
            Prefix with <code className="font-mono">/</code> to bypass the AI instantly:
          </p>

          <table className="w-full border-collapse">
            <tbody>
              {ROWS.map(([syntax, desc]) => (
                <tr key={syntax}>
                  <td className="pr-3 py-0.5 font-mono text-[10px] text-[var(--brand)] whitespace-nowrap">
                    {syntax}
                  </td>
                  <td className="py-0.5 text-[10px] text-[var(--muted)]">{desc}</td>
                </tr>
              ))}
            </tbody>
          </table>

          <p className="mt-2 text-[10px] text-[var(--muted)]">
            Examples:{" "}
            <code className="font-mono">/error</code>
            {" · "}
            <code className="font-mono">/service:checkout p99</code>
          </p>
        </div>
      </div>
    </div>
  );
}
