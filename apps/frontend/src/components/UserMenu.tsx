import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Button } from "./ui/button";
import { Badge } from "./ui/badge";
import { useRuntime } from "../hooks/useRuntime";
import { useAuth } from "../hooks/useAuth";
import { useAccessContext } from "../hooks/useAccessContext";

export function UserMenu() {
  const queryClient = useQueryClient();
  const runtime = useRuntime();
  const { data: user, isLoading } = useAuth();
  const { tenantName, role, isElevated, elevation, impersonating } = useAccessContext();

  const logoutMutation = useMutation({
    mutationFn: () => runtime.auth.logout(),
    onSuccess: () => {
      queryClient.clear();
      window.location.href = "/v1/auth/login";
    },
  });

  if (isLoading) return null;
  if (!user) {
    return (
      <a href="/v1/auth/login" style={{ color: "var(--text-muted, #888)", fontSize: "0.85rem" }}>
        Sign in
      </a>
    );
  }

  return (
    <div className="flex flex-wrap items-center gap-2 text-sm">
      <span
        data-testid="active-org-context"
        className="text-xs text-[var(--muted)]"
        title={`Active organization: ${tenantName} (${role})`}
      >
        {tenantName} · {role}
      </span>
      {impersonating && (
        <Badge tone="warn" data-testid="impersonating-indicator">
          Impersonating: {impersonating}
        </Badge>
      )}
      {isElevated && (
        <span
          role="status"
          data-testid="elevated-access-indicator"
          className="inline-flex items-center gap-1 rounded border border-[var(--warn)] bg-[var(--warn-bg,transparent)] px-2 py-0.5 text-xs font-bold text-[var(--text)]"
        >
          <span>ELEVATED ACCESS</span>
          {elevation?.scope && <span> ({elevation.scope})</span>}
        </span>
      )}
      <span className="text-muted-foreground">{user.email}</span>
      <Button
        variant="ghost"
        className="h-8 px-3"
        onClick={() => logoutMutation.mutate()}
        disabled={logoutMutation.isPending}
      >
        Sign out
      </Button>
    </div>
  );
}
