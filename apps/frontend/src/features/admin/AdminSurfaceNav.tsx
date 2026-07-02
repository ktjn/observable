import { Link, useLocation } from "@tanstack/react-router";

type AdminSection = {
  to: string;
  label: string;
};

const sections: AdminSection[] = [
  { to: "/admin", label: "Overview" },
  { to: "/admin/members", label: "Members" },
  { to: "/admin/fleet", label: "Fleet management" },
];

function isActive(pathname: string, to: string): boolean {
  return to === "/admin" ? pathname === to : pathname === to || pathname.startsWith(`${to}/`);
}

export function AdminSurfaceNav() {
  const location = useLocation();

  return (
    <nav aria-label="Admin console sections" className="flex flex-wrap gap-2 border-b border-[var(--border-strong)] pb-1">
      {sections.map((section) => {
        const active = isActive(location.pathname, section.to);

        return (
          <Link
            key={section.to}
            to={section.to}
            aria-current={active ? "page" : undefined}
            className={[
              "inline-flex items-center border-b-2 px-3 py-2 text-[11px] font-semibold uppercase tracking-wide transition-colors",
              active
                ? "border-[var(--brand)] text-[var(--text)]"
                : "border-transparent text-[var(--muted)] hover:border-[var(--border-strong)] hover:text-[var(--text)]",
            ].join(" ")}
          >
            {section.label}
          </Link>
        );
      })}
    </nav>
  );
}
