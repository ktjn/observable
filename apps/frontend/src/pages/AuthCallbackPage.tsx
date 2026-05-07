import { useEffect } from "react";

export default function AuthCallbackPage() {
  useEffect(() => {
    // Forward the query string (?code=&state=) to the backend callback endpoint,
    // which handles the PKCE code exchange and sets the session cookie.
    const qs = window.location.search;
    window.location.href = `/v1/auth/callback${qs}`;
  }, []);

  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100vh" }}>
      Completing sign-in…
    </div>
  );
}
