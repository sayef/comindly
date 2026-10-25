import { useQuery } from "@tanstack/react-query";
import { ChevronsUpDown, LogOut, Moon, Search, Settings, Sun } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, NavLink, Outlet } from "react-router";
import { Activity } from "@/components/activity";
import { CommandPalette } from "@/components/command-palette";
import { useTheme } from "@/components/theme";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import { ago, cn } from "@/lib/utils";

const NAV = [
  { to: "/", label: "Overview" },
  { to: "/contracts", label: "Contracts" },
  { to: "/flows", label: "Flows" },
  { to: "/ledger", label: "Ledger" },
  { to: "/conventions", label: "Conventions" },
  { to: "/packages", label: "Packages" },
  { to: "/activity", label: "Activity" },
  { to: "/setup", label: "Get started" },
];

function Logo({ className }: { className?: string }) {
  return (
    <>
      <img src="/logo.svg" alt="" className={cn("dark:hidden", className)} />
      <img src="/logo-dark.svg" alt="" className={cn("hidden dark:block", className)} />
    </>
  );
}

function NavItem({ to, label, count, busy }: { to: string; label: string; count?: number; busy?: number }) {
  return (
    <NavLink
      to={to}
      end={to === "/"}
      className={({ isActive }) =>
        cn(
          "group flex min-h-9 items-center gap-2.5 rounded-[7px] px-2.5 text-[13.5px] text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground",
          isActive && "bg-accent font-medium text-accent-foreground hover:bg-accent hover:text-accent-foreground",
        )
      }
    >
      {({ isActive }) => (
        <>
          <span className={cn("size-1.5 rounded-[2px] bg-input transition-colors", isActive && "bg-brand")} />
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
          <h1 className="mt-5 text-lg font-semibold tracking-tight">Open comindly from its link</h1>
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
      <aside className="hidden w-[248px] shrink-0 flex-col gap-5 border-r bg-sidebar px-3.5 py-4 md:flex">
        <Link to="/" className="flex items-center gap-2.5 px-2 pt-1">
          <Logo className="size-6" />
          <span className="text-[15px] font-semibold tracking-tight">comindly</span>
        </Link>

        <Link
          to="/settings"
          className="flex min-h-11 items-center justify-between gap-2 rounded-lg border bg-card px-2.5 py-1.5 transition-colors hover:border-input"
          title="Team settings"
        >
          <span className="flex min-w-0 flex-col gap-0.5">
            <span className="eyebrow">Team</span>
            <span className="truncate text-[13.5px] font-medium">{teamName ?? "No team yet"}</span>
          </span>
          <ChevronsUpDown className="size-3.5 shrink-0 text-muted-foreground" />
        </Link>

        <nav className="flex flex-col gap-0.5" aria-label="Main">
          {NAV.map((n) => (
            <NavItem key={n.to} to={n.to} label={n.label} count={n.label === "Ledger" ? inbox : undefined} busy={n.to === "/activity" ? busy : undefined} />
          ))}
        </nav>

        <div className="mt-auto flex flex-col gap-0.5 border-t pt-3">
          <NavLink
            to="/settings"
            className={({ isActive }) =>
              cn(
                "flex min-h-9 items-center gap-2.5 rounded-[7px] px-2.5 text-[13.5px] text-muted-foreground transition-colors hover:bg-muted/60 hover:text-foreground",
                isActive && "bg-accent font-medium text-accent-foreground hover:bg-accent hover:text-accent-foreground",
              )
            }
          >
            <Settings className="size-3.5" /> Settings
          </NavLink>
          {me.data?.mode === "server" && me.data.email && (
            <div className="flex items-center gap-2.5 px-2.5 py-2">
              <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-[10px] font-semibold uppercase text-foreground">
                {me.data.email.slice(0, 2)}
              </span>
              <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground" title={me.data.email}>
                {me.data.email}
              </span>
              <button
                onClick={signOut}
                className="rounded p-1 text-muted-foreground hover:bg-muted hover:text-foreground"
                aria-label="Sign out"
                title="Sign out"
              >
                <LogOut className="size-3.5" />
              </button>
            </div>
          )}
          <div className="px-2.5 pt-1 font-mono text-[11px] text-muted-foreground">
            {overview.data ? `${overview.data.repos.length} repos · ${overview.data.symbols.toLocaleString()} symbols` : "Loading index…"}
          </div>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="grid h-[60px] shrink-0 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 border-b px-4 md:grid-cols-[1fr_minmax(0,560px)_1fr] md:px-7">
          <div>
            <Link to="/" className="md:hidden">
              <Logo className="size-6" />
            </Link>
          </div>
          <button
            onClick={() => setPalette(true)}
            className="flex h-10 w-full min-w-0 items-center gap-2.5 rounded-lg border bg-sidebar px-3 text-sm text-muted-foreground transition-colors hover:border-input"
          >
            <Search className="size-4 shrink-0" />
            <span className="flex-1 truncate text-left">Search symbols, files, contracts</span>
            <kbd className="hidden rounded border border-input px-1.5 py-0.5 font-mono text-[10.5px] sm:inline">⌘K</kbd>
          </button>
          <div className="flex shrink-0 items-center justify-end gap-3">
            <Activity />
            {overview.data && !overview.data.writable && <Badge variant="outline">Read-only</Badge>}
            {version > 0 && (
              <span
                className="hidden items-center gap-2 whitespace-nowrap font-mono text-xs text-muted-foreground lg:flex"
                title={latest ? `index v${version}, ${ago(latest)} (${latest})` : undefined}
              >
                <span className="size-[7px] shrink-0 rounded-full bg-brand shadow-[0_0_0_3px_color-mix(in_srgb,var(--color-brand)_18%,transparent)]" />
                index v{version}
                <span className="hidden 2xl:inline">· {ago(latest)}</span>
              </span>
            )}
            <Button variant="ghost" size="icon" onClick={toggle} aria-label="Toggle theme">
              {theme === "dark" ? <Sun /> : <Moon />}
            </Button>
          </div>
        </header>
        <nav className="flex gap-1 overflow-x-auto border-b px-4 py-2 md:hidden" aria-label="Main">
          {[...NAV, { to: "/settings", label: "Settings" }].map(({ to, label }) => (
            <NavLink
              key={to}
              to={to}
              end={to === "/"}
              className={({ isActive }) =>
                cn(
                  "rounded-md px-3 py-1.5 text-xs whitespace-nowrap",
                  isActive ? "bg-accent font-medium text-accent-foreground" : "text-muted-foreground",
                )
              }
            >
              {label}
            </NavLink>
          ))}
        </nav>
        <main className="relative min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto w-full min-w-0 max-w-[1400px] px-4 py-7 md:px-7">
            <Outlet />
          </div>
        </main>
      </div>
      <CommandPalette open={palette} onOpenChange={setPalette} />
    </div>
  );
}
