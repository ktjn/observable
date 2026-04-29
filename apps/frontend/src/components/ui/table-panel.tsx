import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

export interface TablePanelProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

export function TablePanel({ children, className, ...props }: TablePanelProps) {
  return (
    <div
      className={cn(
        "bg-[var(--surface)] border border-[var(--border)] rounded-lg overflow-x-auto",
        className,
      )}
      {...props}
    >
      {children}
    </div>
  );
}
