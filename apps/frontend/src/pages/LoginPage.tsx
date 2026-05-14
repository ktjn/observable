import { Button } from "../components/ui/button"
import { initiateLogin } from "../api/auth";
import { useSearch } from "@tanstack/react-router";
import { useEffect } from "react";

const ERROR_MESSAGES: Record<string, string> = {
  session_expired: "Your sign-in session expired. Please try again.",
  auth_failed: "Sign-in failed. Please try again.",
  no_access: "Your account has no access to this workspace.",
  provider_error: "The identity provider is unavailable. Try again shortly.",
  server_error: "An internal error occurred. Please try again.",
};

export default function LoginPage() {
  const { error } = useSearch({ from: "/login" });
  const errorMessage = error ? (ERROR_MESSAGES[error] ?? "Sign-in failed. Please try again.") : null;

  useEffect(() => {
    if (!error) {
      initiateLogin();
    }
  }, [error]);

  return (
    <div className="flex flex-col items-center justify-center min-h-screen bg-background text-foreground p-8 gap-6">
      <div className="text-4xl font-bold tracking-tight">Observable</div>
      {errorMessage && (
        <div
          role="alert"
          className="rounded-md border border-destructive/50 bg-destructive/10 px-4 py-3 text-sm text-destructive max-w-sm text-center"
        >
          {errorMessage}
        </div>
      )}
      <Button
        onClick={initiateLogin}
        className="px-8 h-12"
      >
        Sign in
      </Button>
    </div>
  );
}
