import { useQuery } from "@tanstack/react-query";
import {
  BookText,
  Boxes,
  Compass,
  History,
  LayoutGrid,
  LogOut,
  type LucideIcon,
  Moon,
  Ruler,
  Search,
  Settings,
  Sun,
  Waypoints,
  Workflow,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Link, NavLink, Outlet, useLocation } from "react-router";
import { Activity } from "@/components/activity";
import { CommandPalette } from "@/components/command-palette";
import { useTheme } from "@/components/theme";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { ago, cn } from "@/lib/utils";

const NAV: { to: string; label: string; icon: LucideIcon }[] = [
  { to: "/", label: "Overview", icon: LayoutGrid },
  { to: "/contracts", label: "Contracts", icon: Waypoints },
  { to: "/flows", label: "Flows", icon: Workflow },
  { to: "/ledger", label: "Ledger", icon: BookText },
  { to: "/conventions", label: "Conventions", icon: Ruler },
  { to: "/packages", label: "Packages", icon: Boxes },
  { to: "/activity", label: "Activity", icon: History },
  { to: "/setup", label: "Get started", icon: Compass },
];

const navItem = (isActive: boolean) =>
  cn(
    "group flex min-h-9 items-center gap-2.5 rounded-md px-2.5 text-sm text-muted-foreground transition-colors duration-100 hover:bg-muted hover:text-foreground",
    isActive && "bg-card font-semibold text-foreground shadow-[0_1px_2px_var(--shadow-ink)] hover:bg-card",
  );

function Logo({ className }: { className?: string }) {
  return (
    <>
      <img src="/logo.svg" alt="" className={cn("dark:hidden", className)} />
      <img src="/logo-dark.svg" alt="" className={cn("hidden dark:block", className)} />
    </>
  );
}

function NavItem({ to, label, icon: Icon, count, busy }: { to: string; label: string; icon: LucideIcon; count?: number; busy?: number }) {
  return (
    <NavLink to={to} end={to === "/"} className={({ isActive }) => navItem(isActive)}>
      {({ isActive }) => (
        <>
          <Icon className={cn("size-4 shrink-0", isActive && "text-brand")} strokeWidth={1.75} />
          <span className="flex-1">{label}</span>
          {!!count && <Badge variant="warning">{count}</Badge>}
          {!!busy && <Badge variant="brand">{busy}</Badge>}
        </>
      )}
    </NavLink>
  );
}

export function Layout() {
  const { theme, toggle } = useTheme();
  const [palette, setPalette] = useState(false);
  const overview = useQuery({ queryKey: ["overview"], queryFn: api.overview });
  const versions = useQuery({ queryKey: ["versions"], queryFn: api.versions });
  const team = useQuery({ queryKey: ["team"], queryFn: api.team, retry: false });
  const me = useQuery({ queryKey: ["me"], queryFn: api.me, staleTime: Infinity });
  const teamName = team.data?.version != null ? (team.data.settings.name ?? null) : null;
  const signOut = () => api.signOut().finally(() => window.location.replace("/"));
  const { pathname } = useLocation();
  // On a phone the page list scrolls sideways; keep the current page in view.
  useEffect(() => {
    document.querySelector('[data-pages] [aria-current="page"]')?.scrollIntoView({ inline: "center", block: "nearest" });
  }, [pathname]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette((p) => !p);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const inbox = overview.data
    ? Object.entries(overview.data.ledger)
        .filter(([k]) => k === "stale" || k === "orphaned")
        .reduce((n, [, v]) => n + v, 0)
    : 0;
  const job = useQuery({ queryKey: ["index-job"], queryFn: api.indexJob });
  const busy = (job.data?.running ? 1 : 0) + (job.data?.queued?.length ?? 0);
  const version = versions.data?.length ?? 0;
  const latest = versions.data?.at(-1);

  if (overview.error?.message.includes("access token")) {
    return (
      <div className="flex h-full items-center justify-center p-6">
        <div className="max-w-md text-center">
          <Logo className="mx-auto size-10" />
          <h1 className="mt-5 text-[17px] font-semibold tracking-tight">Open comindly from its link</h1>
          <p className="mt-2 text-sm text-muted-foreground">
            comindly only answers browsers that hold its access token. Run{" "}
            <code className="font-mono text-foreground">comindly ui --web --open</code>, or open the link it prints once; after that, this
            address works as a bookmark.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex h-full">
      <a
        href="#main"
        className="sr-only rounded-md bg-card px-3 py-2 text-sm font-semibold shadow-float focus-visible:not-sr-only focus-visible:fixed focus-visible:top-3 focus-visible:left-3 focus-visible:z-50"
      >
        Skip to content
      </a>
      <aside className="hidden w-[232px] shrink-0 flex-col gap-6 bg-sidebar px-3 py-4 md:flex">
        <div className="flex flex-col gap-3 px-2 pt-1">
          <Link to="/" className="flex items-center gap-2.5 self-start rounded-md">
            <Logo className="size-6" />
            <span className="text-[17px] font-semibold tracking-[-0.01em]" translate="no">
              comindly
            </span>
          </Link>
          <Link to="/settings" className="group flex min-w-0 flex-col self-start rounded-sm" title="Team settings">
            <span className="eyebrow">Team</span>
            <span className="truncate text-sm font-semibold group-hover:text-brand">{teamName ?? "No team yet"}</span>
          </Link>
        </div>

        <nav className="flex flex-col gap-0.5" aria-label="Main">
          {NAV.map((n) => (
            <NavItem
              key={n.to}
              to={n.to}
              label={n.label}
              icon={n.icon}
              count={n.label === "Ledger" ? inbox : undefined}
              busy={n.to === "/activity" ? busy : undefined}
            />
          ))}
        </nav>

        <div className="mt-auto flex flex-col gap-0.5">
          <NavLink to="/settings" className={({ isActive }) => navItem(isActive)}>
            {({ isActive }) => (
              <>
                <Settings className={cn("size-4", isActive && "text-brand")} strokeWidth={1.75} /> Settings
              </>
            )}
          </NavLink>
          {me.data?.mode === "server" && me.data.email && (
            <div className="flex items-center gap-2.5 px-2.5 py-2">
              <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground" title={me.data.email}>
                {me.data.email}
              </span>
              <button
                onClick={signOut}
                className="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
                aria-label="Sign out"
                title="Sign out"
              >
                <LogOut className="size-3.5" />
              </button>
            </div>
          )}
          <p className="nums px-2.5 pt-2 text-xs text-muted-foreground">
            {overview.data
              ? `${overview.data.repos.length} repositories, ${overview.data.symbols.toLocaleString()} symbols`
              : "Loading the index…"}
          </p>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="mx-auto grid h-[60px] w-full max-w-[1320px] shrink-0 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 px-4 md:grid-cols-[minmax(0,560px)_1fr] md:px-8">
          <Link to="/" className="md:hidden" aria-label="Overview">
            <Logo className="size-6" />
          </Link>
          <button
            onClick={() => setPalette(true)}
            className="flex h-10 w-full min-w-0 items-center gap-2.5 rounded-lg bg-card px-3 text-sm text-muted-foreground shadow-[0_1px_2px_var(--shadow-ink)] transition-colors duration-100 hover:text-foreground"
          >
            <Search className="size-4 shrink-0" />
            <span className="flex-1 truncate text-left">Search symbols, files and contracts</span>
            <kbd className="hidden rounded-sm bg-muted px-1.5 py-0.5 text-xs font-semibold sm:inline">⌘&nbsp;K</kbd>
          </button>
          <div className="flex shrink-0 items-center justify-end gap-3">
            <Activity />
            {overview.data && !overview.data.writable && <Badge variant="outline">Read-only</Badge>}
            {version > 0 && (
              <span className="nums hidden whitespace-nowrap text-xs text-muted-foreground lg:inline" title={latest ?? undefined}>
                Index version {version}, {ago(latest)}
              </span>
            )}
            <Button variant="ghost" size="icon" onClick={toggle} aria-label="Toggle theme">
              {theme === "dark" ? <Sun /> : <Moon />}
            </Button>
          </div>
        </header>
        <nav data-pages className="flex gap-1 overflow-x-auto px-4 pb-2 md:hidden" aria-label="Main">
          {[...NAV, { to: "/settings", label: "Settings" }].map(({ to, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === "/"}
              className={({ isActive }) =>
                cn(
                  "flex min-h-11 items-center rounded-md px-3 text-sm whitespace-nowrap",
                  isActive ? "bg-card font-semibold text-foreground" : "text-muted-foreground",
                )
              }
            >
              {label}
            </NavLink>
          ))}
        </nav>
        <main id="main" tabIndex={-1} className="relative min-h-0 flex-1 overflow-y-auto focus-visible:outline-none">
          <div className="mx-auto w-full min-w-0 max-w-[1320px] px-4 pt-4 pb-16 md:px-8">
            <Outlet />
          </div>
        </main>
      </div>
      <CommandPalette open={palette} onOpenChange={setPalette} />
    </div>
  );
}
