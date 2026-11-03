import { useQuery } from "@tanstack/react-query";
import {
  BookText,
  ChevronsUpDown,
  Boxes,
  Compass,
  History,
  LayoutGrid,
  LogOut,
  Menu,
  type LucideIcon,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  Ruler,
  Search,
  Settings,
  Sun,
  Users,
  Waypoints,
  Workflow,
  X,
} from "lucide-react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { useEffect, useRef, useState } from "react";
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

const navItem = (isActive: boolean, narrow = false) =>
  cn(
    "group flex min-h-9 items-center gap-2.5 rounded-md px-2.5 text-sm text-muted-foreground transition-colors duration-100 hover:bg-muted hover:text-foreground",
    narrow && "justify-center px-0",
    isActive && "bg-accent font-semibold text-accent-foreground hover:bg-accent hover:text-accent-foreground",
  );

const RAIL_KEY = "comindly.rail";
function savedNarrow() {
  try {
    return localStorage.getItem(RAIL_KEY) === "narrow";
  } catch {
    return false;
  }
}

function Logo({ className }: { className?: string }) {
  return (
    <>
      <img src="/logo.svg" alt="" className={cn("dark:hidden", className)} />
      <img src="/logo-dark.svg" alt="" className={cn("hidden dark:block", className)} />
    </>
  );
}

function NavItem({
  to,
  label,
  icon: Icon,
  count,
  busy,
  narrow,
}: { to: string; label: string; icon: LucideIcon; count?: number; busy?: number; narrow?: boolean }) {
  return (
    <NavLink to={to} end={to === "/"} className={({ isActive }) => navItem(isActive, narrow)} title={narrow ? label : undefined}>
      {({ isActive }) => (
        <>
          <span className="relative">
            <Icon className={cn("size-4 shrink-0", isActive ? "text-brand" : "opacity-80")} strokeWidth={1.75} />
            {narrow && (!!count || !!busy) && <span className={cn("absolute -top-1 -right-1.5 size-2 rounded-full", count ? "bg-warning" : "bg-brand")} />}
          </span>
          <span className={cn("flex-1", narrow && "sr-only")}>{label}</span>
          {!narrow && !!count && <Badge variant="warning">{count}</Badge>}
          {!narrow && !!busy && <Badge variant="brand">{busy}</Badge>}
        </>
      )}
    </NavLink>
  );
}

export function Layout() {
  const { theme, toggle } = useTheme();
  const [palette, setPalette] = useState(false);
  const [narrow, setNarrow] = useState(savedNarrow);
  const toggleRail = () =>
    setNarrow((n) => {
      try {
        localStorage.setItem(RAIL_KEY, n ? "wide" : "narrow");
      } catch {
        // storage may be unavailable
      }
      return !n;
    });
  const overview = useQuery({ queryKey: ["overview"], queryFn: api.overview });
  const versions = useQuery({ queryKey: ["versions"], queryFn: api.versions });
  const team = useQuery({ queryKey: ["team"], queryFn: api.team, retry: false });
  const me = useQuery({ queryKey: ["me"], queryFn: api.me, staleTime: Infinity });
  const teamName = team.data?.version != null ? (team.data.settings.name ?? null) : null;
  const signOut = () => api.signOut().finally(() => window.location.replace("/"));
  const { pathname } = useLocation();
  const [menu, setMenu] = useState(false);
  const menuButton = useRef<HTMLButtonElement>(null);
  useEffect(() => setMenu(false), [pathname]);
  useEffect(() => {
    const page = [...NAV, { to: "/settings", label: "Settings" }].filter((n) => (n.to === "/" ? pathname === "/" : pathname.startsWith(n.to))).at(-1);
    document.title = page && page.to !== "/" ? `${page.label} · comindly` : "comindly";
  }, [pathname]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette((p) => !p);
      }
      if ((e.metaKey || e.ctrlKey) && e.key === "\\") {
        e.preventDefault();
        toggleRail();
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

  // The rail's contents: in the side column on desktop (wide or narrow), in a drawer on phones.
  const railBody = (narrow: boolean, drawer = false) => (
    <>
          <div className={cn("flex items-center", narrow ? "flex-col gap-3" : "justify-between")}>
            <Link to="/" className="flex items-center gap-2.5 rounded-md px-2 pt-1" aria-label={narrow ? "comindly overview" : undefined}>
              <Logo className="size-6" />
              {!narrow && (
                <span className="text-[17px] font-semibold tracking-[-0.01em]" translate="no">
                  comindly
                </span>
              )}
            </Link>
            {drawer ? (
              <DialogPrimitive.Close asChild>
                <Button variant="ghost" size="icon" className="size-8" aria-label="Close menu">
                  <X />
                </Button>
              </DialogPrimitive.Close>
            ) : (
              <Button
                variant="ghost"
                size="icon"
                className="size-8"
                onClick={toggleRail}
                aria-label={narrow ? "Expand sidebar" : "Collapse sidebar"}
                aria-expanded={!narrow}
                title={`${narrow ? "Expand" : "Collapse"} sidebar (⌘ \\)`}
              >
                {narrow ? <PanelLeftOpen /> : <PanelLeftClose />}
              </Button>
            )}
          </div>

          {narrow ? (
            <Link
              to="/settings"
              className="flex size-11 items-center justify-center self-center rounded-lg border bg-card text-sm font-semibold transition-colors duration-100 hover:border-input"
              title={`Team: ${teamName ?? "no team yet"}`}
              aria-label={`Team settings: ${teamName ?? "no team yet"}`}
            >
              {teamName ? teamName.slice(0, 1).toUpperCase() : <Users className="size-4 text-muted-foreground" />}
            </Link>
          ) : (
            <Link
              to="/settings"
              className="flex min-h-11 items-center justify-between gap-2 rounded-lg border bg-card px-2.5 py-1.5 transition-colors duration-100 hover:border-input"
              title="Team settings"
            >
              <span className="flex min-w-0 flex-col">
                <span className="eyebrow">Team</span>
                <span className="truncate text-sm font-semibold">{teamName ?? "No team yet"}</span>
              </span>
              <ChevronsUpDown className="size-3.5 shrink-0 text-muted-foreground" />
            </Link>
          )}

          <nav className="flex flex-col gap-0.5" aria-label="Main">
            {NAV.map((n) => (
              <NavItem
                key={n.to}
                to={n.to}
                label={n.label}
                icon={n.icon}
                narrow={narrow}
                count={n.label === "Ledger" ? inbox : undefined}
                busy={n.to === "/activity" ? busy : undefined}
              />
            ))}
          </nav>

          <div className="mt-auto flex flex-col gap-0.5 border-t pt-3">
            <NavItem to="/settings" label="Settings" icon={Settings} narrow={narrow} />
            {me.data?.mode === "server" && me.data.email && (
              <div className={cn("flex items-center gap-2.5 py-2", narrow ? "justify-center" : "px-2.5")}>
                {!narrow && (
                  <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground" title={me.data.email}>
                    {me.data.email}
                  </span>
                )}
                <button
                  onClick={signOut}
                  className="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground"
                  aria-label="Sign out"
                  title={narrow ? `Sign out ${me.data.email}` : "Sign out"}
                >
                  <LogOut className="size-3.5" />
                </button>
              </div>
            )}
            {!narrow && (
              <p className="nums px-2.5 pt-2 text-xs text-muted-foreground">
                {overview.data
                  ? `${overview.data.repos.length} repositories, ${overview.data.symbols.toLocaleString()} symbols`
                  : "Loading the index…"}
              </p>
            )}
          </div>
    </>
  );

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
      <aside className={cn("hidden shrink-0 flex-col gap-5 border-r bg-sidebar py-4 md:flex", narrow ? "w-[64px] px-2.5" : "w-[248px] px-3.5")}>
        {railBody(narrow)}
      </aside>
      <DialogPrimitive.Root open={menu} onOpenChange={setMenu}>
        <DialogPrimitive.Portal>
          <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-[rgb(6_14_12/0.55)] md:hidden" />
          <DialogPrimitive.Content
            aria-describedby={undefined}
            onCloseAutoFocus={(e) => (e.preventDefault(), menuButton.current?.focus())}
            className="fixed inset-y-0 left-0 z-50 flex w-[min(288px,85vw)] flex-col gap-5 overflow-y-auto overscroll-contain border-r bg-sidebar px-3.5 pt-[max(1rem,env(safe-area-inset-top))] pb-[max(1rem,env(safe-area-inset-bottom))] shadow-float md:hidden"
          >
            <DialogPrimitive.Title className="sr-only">Menu</DialogPrimitive.Title>
            {railBody(false, true)}
          </DialogPrimitive.Content>
        </DialogPrimitive.Portal>
      </DialogPrimitive.Root>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="grid h-[60px] shrink-0 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 border-b px-4 md:grid-cols-[minmax(0,1fr)_minmax(0,560px)_minmax(0,1fr)] md:px-7">
          <div className="md:hidden">
            <Button ref={menuButton} variant="ghost" size="icon" className="-ml-2" onClick={() => setMenu(true)} aria-label="Open menu" aria-expanded={menu}>
              <Menu />
            </Button>
          </div>
          <div className="hidden md:block" />
          <button
            onClick={() => setPalette(true)}
            className="flex h-10 w-full min-w-0 items-center gap-2.5 rounded-lg border bg-card pointer-coarse:h-11 px-3 text-sm text-muted-foreground transition-colors duration-100 hover:border-input hover:text-foreground"
          >
            <Search className="size-4 shrink-0" />
            <span className="flex-1 truncate text-left">Search symbols, files and contracts</span>
            <kbd className="hidden rounded-sm bg-muted px-1.5 py-0.5 text-xs font-semibold sm:inline">⌘&nbsp;K</kbd>
          </button>
          {/* Equal side columns keep the search centred whatever shows up here. */}
          <div className="flex min-w-0 items-center justify-end gap-3 overflow-hidden">
            <Activity />
            {overview.data && !overview.data.writable && <Badge variant="outline">Read-only</Badge>}
            {version > 0 && (
              <span className="nums hidden whitespace-nowrap text-xs text-muted-foreground xl:inline" title={latest ?? undefined}>
                Index version {version}, {ago(latest)}
              </span>
            )}
            <Button variant="ghost" size="icon" className="shrink-0" onClick={toggle} aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}>
              {theme === "dark" ? <Sun /> : <Moon />}
            </Button>
          </div>
        </header>
        <main id="main" tabIndex={-1} className="relative min-h-0 flex-1 overflow-y-auto focus-visible:outline-none">
          <div className="mx-auto w-full min-w-0 max-w-[1400px] px-4 py-7 md:px-7">
            <Outlet />
          </div>
        </main>
      </div>
      <CommandPalette open={palette} onOpenChange={setPalette} />
    </div>
  );
}
