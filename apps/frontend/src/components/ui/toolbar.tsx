import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

export interface ToolbarProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

export function Toolbar({ className, children, ...props }: ToolbarProps) {
  return (
    <div role="toolbar" className={cn("modern-toolbar", className)} {...props}>
      {children}
    </div>
  );
}
