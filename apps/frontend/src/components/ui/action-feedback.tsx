import { Button } from "./button";
import { cn } from "./cn";

export type ActionFeedbackStatus = "idle" | "pending" | "success" | "error";

export interface ActionFeedbackProps {
  status: ActionFeedbackStatus;
  message: string;
  detail?: string;
  onRetry?: () => void;
  onDismiss?: () => void;
  className?: string;
}

const statusStyles: Record<Exclude<ActionFeedbackStatus, "idle">, { border: string; badge: string; label: string }> = {
  pending: {
    border: "border-[var(--brand)]",
    badge: "text-[var(--brand)]",
    label: "In progress",
  },
  success: {
    border: "border-[var(--good)]",
    badge: "text-[var(--good)]",
    label: "Completed",
  },
  error: {
    border: "border-[var(--bad)]",
    badge: "text-[var(--bad)]",
    label: "Failed",
  },
};

/**
 * Live-announced action progress, completion, and failure feedback (OBS-24-09, UX-09).
 */
export function ActionFeedback({
  status,
  message,
  detail,
  onRetry,
  onDismiss,
  className,
}: ActionFeedbackProps) {
  if (status === "idle") return null;

  const style = statusStyles[status];
  const isError = status === "error";

  return (
    <div
      role={isError ? "alert" : "status"}
      aria-live={isError ? "assertive" : "polite"}
      data-testid="action-feedback"
      data-action-status={status}
      className={cn(
        "modern-panel flex flex-wrap items-center justify-between gap-3 border-l-4 px-4 py-3 text-sm",
        style.border,
        className,
      )}
    >
      <div className="grid gap-0.5">
        <div className="flex items-center gap-2">
          <span className={cn("text-xs font-bold uppercase tracking-wide", style.badge)}>
            {style.label}
          </span>
          <span className="font-semibold text-[var(--text)]">{message}</span>
        </div>
        {detail && <p className="m-0 text-xs text-[var(--muted)]">{detail}</p>}
      </div>
      {(onRetry || onDismiss) && (
        <div className="flex items-center gap-2">
          {onRetry && (
            <Button type="button" variant="secondary" onClick={onRetry}>
              Retry
            </Button>
          )}
          {onDismiss && (
            <Button type="button" variant="ghost" onClick={onDismiss}>
              Dismiss
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
