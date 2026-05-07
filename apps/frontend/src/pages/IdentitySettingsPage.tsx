import { useQuery } from "@tanstack/react-query";
import { me } from "../api/auth";

export default function IdentitySettingsPage() {
  const { data: user, isLoading } = useQuery({
    queryKey: ["me"],
    queryFn: me,
    retry: false,
  });

  if (isLoading) return <div className="content-shell">Loading…</div>;

  const isTenantAdmin = user?.tenants.some((t) => t.role === "tenant_admin");
  if (!isTenantAdmin) {
    return (
      <div className="content-shell" style={{ padding: "1.5rem" }}>
        <p>Only tenant administrators can view identity settings.</p>
      </div>
    );
  }

  const issuer =
    typeof window !== "undefined"
      ? (window as Window & { __OBSERVABLE_ZITADEL_ISSUER__?: string }).__OBSERVABLE_ZITADEL_ISSUER__ ?? "http://localhost:8082"
      : "http://localhost:8082";

  return (
    <div style={{ padding: "1.5rem", maxWidth: "640px", display: "flex", flexDirection: "column", gap: "1rem" }}>
      <h1 style={{ fontSize: "1.1rem", fontWeight: 700, margin: 0 }}>Identity Settings</h1>

      <table style={{ borderCollapse: "collapse", width: "100%", fontSize: "0.9rem" }}>
        <tbody>
          <tr>
            <td style={{ padding: "0.4rem 1rem 0.4rem 0", fontWeight: 600, whiteSpace: "nowrap" }}>Provider</td>
            <td>Zitadel 2.71.x</td>
          </tr>
          <tr>
            <td style={{ padding: "0.4rem 1rem 0.4rem 0", fontWeight: 600, whiteSpace: "nowrap" }}>Issuer URL</td>
            <td>
              <code style={{ fontFamily: "monospace", fontSize: "0.85rem" }}>{issuer}</code>
            </td>
          </tr>
          <tr>
            <td style={{ padding: "0.4rem 1rem 0.4rem 0", fontWeight: 600, whiteSpace: "nowrap" }}>OIDC Discovery</td>
            <td>
              <a
                href={`${issuer}/.well-known/openid-configuration`}
                target="_blank"
                rel="noreferrer"
                style={{ color: "var(--accent, #3b82f6)" }}
              >
                {issuer}/.well-known/openid-configuration
              </a>
            </td>
          </tr>
          <tr>
            <td style={{ padding: "0.4rem 1rem 0.4rem 0", fontWeight: 600, whiteSpace: "nowrap" }}>Redirect URI</td>
            <td>
              <code style={{ fontFamily: "monospace", fontSize: "0.85rem" }}>
                {typeof window !== "undefined" ? window.location.origin : ""}/auth/callback
              </code>
            </td>
          </tr>
          <tr>
            <td style={{ padding: "0.4rem 1rem 0.4rem 0", fontWeight: 600, whiteSpace: "nowrap" }}>SCIM 2.0 (planned)</td>
            <td>
              <code style={{ fontFamily: "monospace", fontSize: "0.85rem" }}>
                {issuer}/scim/v2/&lt;org-id&gt;/
              </code>
              <span style={{ marginLeft: "0.5rem", color: "var(--text-muted, #888)", fontSize: "0.8rem" }}>
                — enable per-org in Zitadel Admin Console
              </span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}
