// apps/frontend/src/components/ui/loading-state.tsx
import type { ReactNode } from "react";
import { cn } from "./cn";

export function LoadingState({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("p-12 text-center text-[var(--muted)]", className)}>
      {children}
    </div>
  );
}
