import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Button } from "./ui/button"
import { useRuntime } from "../hooks/useRuntime";
import { useAuth } from "../hooks/useAuth";

export function UserMenu() {
  const queryClient = useQueryClient();
  const runtime = useRuntime();
  const { data: user, isLoading } = useAuth();

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
    <div className="flex items-center gap-2 text-sm">
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
