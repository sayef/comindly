import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Pencil } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { api } from "@/lib/api";
import { useTeams } from "@/lib/teams";
import { cn } from "@/lib/utils";

/** A repository's teams as chips; admins change them in place. */
export function TeamChips({ repo, admin }: { repo: string; admin: boolean }) {
  const t = useTeams();
  const qc = useQueryClient();
  const has = t.teamsOf(repo);
  const [editing, setEditing] = useState<string[] | null>(null);
  const [fresh, setFresh] = useState("");
  const save = useMutation({
    mutationFn: (teams: string[]) => api.labelRepo(repo, teams),
    onSuccess: () => (qc.invalidateQueries({ queryKey: ["team"] }), setEditing(null), setFresh("")),
  });

  if (editing) {
    const names = [...new Set([...t.teams.map((x) => x.name), ...editing])];
    const flip = (n: string) => setEditing(editing.includes(n) ? editing.filter((x) => x !== n) : [...editing, n]);
    const add = () => {
      const n = fresh.trim();
      if (n && !editing.some((x) => x.toLowerCase() === n.toLowerCase())) setEditing([...editing, n]);
      setFresh("");
    };
    return (
      <div className="mt-2 flex flex-wrap items-center gap-1.5">
        {names.map((n) => (
          <Chip key={n} on={editing.includes(n)} onClick={() => flip(n)}>
            {n}
          </Chip>
        ))}
        <Input
          value={fresh}
          onChange={(e) => setFresh(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), add())}
          onBlur={add}
          placeholder="New team"
          aria-label={`New team for ${repo}`}
          className="h-7 w-32 text-xs"
        />
        <Button size="sm" variant="outline" loading={save.isPending} onClick={() => save.mutate(editing)}>
          Save
        </Button>
        <Button size="sm" variant="ghost" onClick={() => (setEditing(null), setFresh(""))}>
          Cancel
        </Button>
        {save.error && <span className="text-xs text-danger">{save.error.message}</span>}
      </div>
    );
  }

  if (!has.length && !admin) return null;
  return (
    <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
      {has.length ? has.map((n) => <Chip key={n}>{n}</Chip>) : <span className="text-xs text-muted-foreground">No team</span>}
      {admin && (
        <button
          onClick={() => setEditing(has)}
          className="flex size-6 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground"
          aria-label={`Change the teams of ${repo}`}
          title="Change teams"
        >
          <Pencil className="size-3" />
        </button>
      )}
    </div>
  );
}

export function Chip({ on, onClick, children }: { on?: boolean; onClick?: () => void; children: React.ReactNode }) {
  const style = cn(
    "rounded-sm border px-1.5 py-0.5 text-xs",
    on === undefined ? "bg-secondary" : on ? "bg-accent font-semibold text-accent-foreground" : "text-muted-foreground",
    onClick && "transition-colors duration-100 hover:border-input",
  );
  return onClick ? (
    <button type="button" aria-pressed={on} onClick={onClick} className={style}>
      {children}
    </button>
  ) : (
    <span className={style}>{children}</span>
  );
}
