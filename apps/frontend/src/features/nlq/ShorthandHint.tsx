import type { ReactNode } from "react";

const ROWS: [string, string][] = [
  ["m:<name>", "metric name"],
  ["f:<field>:<val>", "filter (explicit)"],
  ["<field>:<val>", "filter (shorthand)"],
  ["op:<type>", "operation override"],
  ['"quoted text"', "search term (exact)"],
  ["unquoted word", "search term"],
];

interface ShorthandHintProps {
  children: ReactNode;
  className?: string;
}

/**
 * Wraps an input element and shows a shorthand syntax reference card on hover.
 * CSS-only — no JS required.
 */
export function ShorthandHint({ children, className = "relative group flex-1" }: ShorthandHintProps) {
  return (
    <div className={className}>
      {children}

      <div
        className="pointer-events-none absolute left-0 top-full z-[1000] mt-1 hidden w-72 max-w-[min(18rem,calc(100vw-2rem))] group-hover:block"
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
