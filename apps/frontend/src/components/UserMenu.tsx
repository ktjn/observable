import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { me, logout } from "../api/auth";

export function UserMenu() {
  const queryClient = useQueryClient();
  const { data, isLoading } = useQuery({
    queryKey: ["me"],
    queryFn: me,
    retry: false,
  });

  const logoutMutation = useMutation({
    mutationFn: logout,
    onSuccess: () => {
      queryClient.clear();
      window.location.href = "/login";
    },
  });

  if (isLoading) return null;
  if (!data) {
    return (
      <a href="/login" style={{ color: "var(--text-muted, #888)", fontSize: "0.85rem" }}>
        Sign in
      </a>
    );
  }

  return (
    <div style={{ display: "flex", alignItems: "center", gap: "0.75rem", fontSize: "0.85rem" }}>
      <span style={{ color: "var(--text-muted, #888)" }}>{data.email}</span>
      <button
        onClick={() => logoutMutation.mutate()}
        disabled={logoutMutation.isPending}
        style={{
          background: "none",
          border: "1px solid var(--border)",
          borderRadius: "var(--radius, 4px)",
          padding: "2px 8px",
          cursor: "pointer",
          color: "var(--text)",
          fontSize: "inherit",
        }}
      >
        Sign out
      </button>
    </div>
  );
}
