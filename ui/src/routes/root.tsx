import { Link, Outlet } from "@tanstack/react-router";

const NAV: { to: string; label: string }[] = [
  { to: "/", label: "Đọc (TTS)" },
  { to: "/flows", label: "Flows" },
  { to: "/runs", label: "Runs" },
  { to: "/settings", label: "Cài đặt" },
  { to: "/api", label: "API" },
];

export function RootLayout() {
  return (
    <div className="flex h-screen flex-col bg-canvas text-ink">
      <header className="flex items-center gap-5 border-b border-border px-5 py-3">
        <div className="flex items-center gap-2.5">
          <span className="text-xl text-brand">◐</span>
          <div className="leading-tight">
            <div className="text-sm font-semibold">VieNeu Studio</div>
            <div className="text-[11px] text-muted">v3-Turbo · Rust · on-device</div>
          </div>
        </div>
        <nav className="flex gap-1">
          {NAV.map((n) => (
            <Link
              key={n.to}
              to={n.to}
              // exact match only for the index route so it isn't always active
              activeOptions={{ exact: n.to === "/" }}
              className="rounded-md px-3 py-1.5 text-sm text-muted transition hover:text-ink"
              activeProps={{ className: "rounded-md px-3 py-1.5 text-sm text-ink bg-surface-2" }}
            >
              {n.label}
            </Link>
          ))}
        </nav>
      </header>

      <main className="flex-1 overflow-auto p-5">
        <Outlet />
      </main>
    </div>
  );
}
