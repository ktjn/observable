import type { ReactNode } from "react";
import { cn } from "./cn";

export interface PillDefinition {
  key: string;
  label: string;
  count: number;
  /** Optional CSS color value for the active state (e.g. "var(--bad)"). Defaults to "var(--brand)". */
  activeColor?: string;
  /** Optional icon element shown before the label. */
  icon?: ReactNode;
}

export interface PillFilterProps {
  pills: PillDefinition[];
  activeKey: string;
  onSelect: (key: string) => void;
  className?: string;
  rounded?: boolean;
  ariaLabel?: string;
}

export function PillFilter({
  pills,
  activeKey,
  onSelect,
  className,
  rounded = false,
  ariaLabel,
}: PillFilterProps) {
  return (
    <div className={cn("flex items-center gap-1 flex-wrap", className)} aria-label={ariaLabel}>
      {pills.map((pill) => {
        const isActive = activeKey === pill.key;
        const activeColor = pill.activeColor ?? "var(--brand)";
        return (
          <button
            key={pill.key}
            type="button"
            onClick={() => onSelect(pill.key)}
            style={isActive ? { borderColor: activeColor, color: activeColor } : undefined}
            className={cn(
              "flex items-center gap-1 px-2.5 py-1 text-xs font-bold border transition-colors",
              rounded && "rounded",
              isActive
                ? ""
                : "border-[var(--border)] text-[var(--muted)] hover:border-[var(--brand)] hover:text-[var(--text)]"
            )}
          >
            {pill.icon && <span className="opacity-70">{pill.icon}</span>}
            <span className={cn(pill.icon && "capitalize")}>{pill.label}</span>
            <span aria-hidden="true">({pill.count})</span>
          </button>
        );
      })}
    </div>
  );
}
