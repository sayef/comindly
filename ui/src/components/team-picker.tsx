import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Check, ChevronDown, Users } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link } from "react-router";
import { api } from "@/lib/api";
import { UNASSIGNED, useTeams } from "@/lib/teams";
import { cn } from "@/lib/utils";

/** Which teams' repositories every page shows; signed-in members also join and leave teams here. */
export function TeamPicker({ admin }: { admin: boolean }) {
  const t = useTeams();
  const qc = useQueryClient();
  const [open, setOpen] = useState<{ top: number; right: number } | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const join = useMutation({
    mutationFn: api.saveMyTeams,
    meta: { quiet: true },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["team"] }),
  });

  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => root.current?.contains(e.target as Node) || setOpen(null);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(null);
    const close = () => setOpen(null);
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", esc);
    window.addEventListener("resize", close);
    return () => (
      document.removeEventListener("pointerdown", away), document.removeEventListener("keydown", esc), window.removeEventListener("resize", close)
    );
  }, [open]);

  if (!t.teams.length) {
    if (!admin) return null;
    return (
      <Link to="/settings#teams" className="hidden shrink-0 text-xs text-muted-foreground underline-offset-2 hover:text-foreground hover:underline lg:inline">
        Set up teams
      </Link>
    );
  }

  const isOn = (n: string) => t.selected.some((s) => s.toLowerCase() === n.toLowerCase());
  const toggle = (n: string) => t.select(isOn(n) ? t.selected.filter((s) => s.toLowerCase() !== n.toLowerCase()) : [...t.selected, n]);
  const label = !t.selected.length
    ? "Everyone"
    : t.selected.length === 1
      ? t.selected[0] === UNASSIGNED
        ? "Unassigned"
        : t.selected[0]
      : `${t.selected.length} teams`;

  return (
    <div ref={root} className="relative shrink-0">
      <button
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          setOpen((o) => (o ? null : { top: r.bottom + 6, right: window.innerWidth - r.right }));
        }}
        aria-haspopup="true"
        aria-expanded={!!open}
        title="Which teams' repositories to show"
        className="flex h-9 max-w-44 items-center gap-1.5 rounded-md border bg-card px-2.5 text-sm transition-colors duration-100 hover:border-input"
      >
        <Users className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="truncate">{label}</span>
        <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
      </button>
      {open && (
        <div role="group" aria-label="Teams in view" style={open} className="fixed z-50 w-64 rounded-lg border bg-popover p-1.5 text-sm shadow-float">
          <Option on={!t.selected.length} onClick={() => t.select([])}>
            Everyone
          </Option>
          {t.teams.map((team) => (
            <Option key={team.name} on={isOn(team.name)} onClick={() => toggle(team.name)}>
              <span className="flex-1 truncate">{team.name}</span>
              <span className="nums text-xs text-muted-foreground">{team.repos.length}</span>
            </Option>
          ))}
          <Option on={isOn(UNASSIGNED)} onClick={() => toggle(UNASSIGNED)}>
            <span className="flex-1 text-muted-foreground">Unassigned</span>
          </Option>
          {t.server && (
            <div className="mt-1.5 border-t px-2 pt-2 pb-1">
              <p className="text-xs text-muted-foreground">Your teams</p>
              <div className="mt-1.5 flex flex-wrap gap-1">
                {t.teams.map((team) => {
                  const member = t.mine.includes(team.name);
                  return (
                    <button
                      key={team.name}
                      disabled={join.isPending}
                      onClick={() => join.mutate(member ? t.mine.filter((m) => m !== team.name) : [...t.mine, team.name])}
                      aria-pressed={member}
                      className={cn(
                        "rounded-sm border px-1.5 py-0.5 text-xs transition-colors duration-100 hover:border-input",
                        member ? "bg-accent font-semibold text-accent-foreground" : "text-muted-foreground",
                      )}
                    >
                      {team.name}
                    </button>
                  );
                })}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function Option({ on, onClick, children }: { on: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button role="menuitemcheckbox" aria-checked={on} onClick={onClick} className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left hover:bg-muted">
      <Check className={cn("size-3.5 shrink-0", on ? "text-brand" : "invisible")} />
      {children}
    </button>
  );
}
