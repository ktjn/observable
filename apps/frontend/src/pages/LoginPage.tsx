import { initiateLogin } from "../api/auth";

export default function LoginPage() {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        height: "100vh",
        gap: "1.5rem",
        background: "var(--background)",
        color: "var(--text)",
      }}
    >
      <div style={{ fontSize: "1.5rem", fontWeight: 700, letterSpacing: "0.1em" }}>
        OBSERVABLE
      </div>
      <button
        onClick={initiateLogin}
        style={{
          padding: "0.6rem 1.6rem",
          background: "var(--accent, #3b82f6)",
          color: "#fff",
          border: "none",
          borderRadius: "var(--radius, 4px)",
          cursor: "pointer",
          fontSize: "1rem",
        }}
      >
        Sign in
      </button>
    </div>
  );
}
