import { Link, Outlet } from "@tanstack/react-router";
import { useTheme, type ThemePreference } from "../lib/theme";
import { useTimeDisplay, TIME_FORMAT_OPTIONS } from "../lib/timeDisplay";

const navItems = [
  { label: "Setup", to: "/setup" },
  { label: "Ask (NLQ)", to: "/nlq" },
  { label: "Services", to: "/services" },
  { label: "Traces", to: "/traces" },
  { label: "Logs", to: "/logs" },
  { label: "Infrastructure", to: "/infrastructure" },
  { label: "Service Overview", to: "/service-overview" },
  { label: "Dashboards", to: "/dashboards" },
  { label: "Alerts & SLOs", to: "/alerts" },
  { label: "Admin / Fleet / Billing", to: "/admin" },
] as const;

const themeOptions: { label: string; value: ThemePreference }[] = [
  { label: "Light", value: "light" },
  { label: "Dark", value: "dark" },
  { label: "Sys", value: "system" },
];

export function AppShell() {
  const { preference, setPreference } = useTheme();
  const { format, setFormat } = useTimeDisplay();

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="Primary navigation">
        <div className="brand-lockup">
          <div className="brand-mark" aria-label="Observable">OBSERVABLE</div>
        </div>

        <nav className="nav-list">
          {navItems.map((item) => (
            <Link
              key={item.to}
              to={item.to}
              className="nav-link"
              activeProps={{ className: "nav-link active" }}
            >
              {item.label}
            </Link>
          ))}
        </nav>

        <div className="sidebar-footer">
          <div className="field-label">Theme</div>
          <div className="segmented-control" role="radiogroup" aria-label="Theme preference">
            {themeOptions.map((option) => (
              <button
                key={option.value}
                type="button"
                role="radio"
                aria-checked={preference === option.value}
                className={preference === option.value ? "segment active" : "segment"}
                onClick={() => setPreference(option.value)}
              >
                {option.label}
              </button>
            ))}
          </div>
        </div>
      </aside>

      <div className="workspace">
        <header className="topbar">
          <div className="topbar-title">Platform — dev</div>
          <div className="topbar-controls" aria-label="Global context">
            <select
              aria-label="Time display format"
              className="context-pill"
              value={format}
              onChange={(e) => setFormat(e.target.value as typeof format)}
              style={{ cursor: "pointer", background: "var(--surface)", color: "var(--text)", border: "1px solid var(--border)", borderRadius: "var(--radius, 4px)", padding: "2px 6px", fontSize: "inherit" }}
            >
              {TIME_FORMAT_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
            <span className="context-pill">Last 1h</span>
            <Link to="/traces" className="secondary-link">Traces</Link>
            <Link to="/logs" className="secondary-link">Logs</Link>
          </div>
        </header>

        <main className="content-shell">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
