import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2, X } from "lucide-react";
import { useState } from "react";
import { Chip } from "@/components/team-chips";
import { useUnsavedWarning } from "@/components/team-settings";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { api, type TeamDef } from "@/lib/api";
import { useAccess } from "@/lib/me";

const INCOMING: Record<TeamDef["incoming"], string> = {
  open: "Count right away",
  review: "Marked until we confirm",
  closed: "Not accepted",
};

/** The org's teams: which repositories each labels, who belongs, and how notes from outside count. */
export function TeamsCard() {
  const qc = useQueryClient();
  const { server, admin } = useAccess();
  const team = useQuery({ queryKey: ["team"], queryFn: api.team, retry: false });
  const overview = useQuery({ queryKey: ["overview"], queryFn: api.overview });
  const stored = team.data?.settings.teams ?? [];
  // Null until an admin edits, so a refetch never overwrites unsaved edits.
  const [draft, setDraft] = useState<TeamDef[] | null>(null);
  const [fresh, setFresh] = useState("");
  const list = draft ?? stored;
  const dirty = draft !== null && JSON.stringify(draft) !== JSON.stringify(stored);
  useUnsavedWarning(dirty);
  const save = useMutation({
    mutationFn: () => api.saveTeams(list, team.data?.version ?? null),
    meta: { inline: true },
    onSuccess: async () => (await qc.invalidateQueries({ queryKey: ["team"] }), setDraft(null)),
  });
  const indexed = new Set((overview.data?.repos ?? []).map((r) => r.repo));
  const edit = (i: number, change: Partial<TeamDef>) => setDraft(list.map((t, j) => (j === i ? { ...t, ...change } : t)));
  const add = () => {
    const name = fresh.trim();
    if (!name || list.some((t) => t.name.toLowerCase() === name.toLowerCase())) return;
    setDraft([...list, { name, repos: [], members: [], incoming: "review" }]);
    setFresh("");
  };

  if (!team.data) return null;
  return (
    <Card id="teams">
      <CardHeader>
        <CardTitle>Teams</CardTitle>
        <CardDescription>
          Teams label repositories, so everyone can focus on theirs; nobody loses access to the rest. A repository can belong to several
          teams. Label repositories here, or in the list above.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {!list.length && <p className="text-sm text-muted-foreground">No teams yet.</p>}
        {list.map((t, i) => {
          const missing = t.repos.filter((r) => overview.data && !indexed.has(r));
          return (
            <div key={i} className="flex flex-col gap-2.5 rounded-md border px-4 py-3">
              <div className="flex items-center gap-2">
                {admin ? (
                  <Input value={t.name} onChange={(e) => edit(i, { name: e.target.value })} aria-label="Team name" className="h-8 max-w-56 font-semibold" />
                ) : (
                  <span className="font-semibold">{t.name}</span>
                )}
                <span className="nums text-xs text-muted-foreground">
                  {t.repos.length} {t.repos.length === 1 ? "repository" : "repositories"}
                  {server && `, ${t.members.length} ${t.members.length === 1 ? "member" : "members"}`}
                </span>
                {admin && (
                  <Button
                    variant="ghost"
                    size="icon"
                    className="ml-auto size-8"
                    aria-label={`Delete ${t.name || "this team"}`}
                    title="Delete the team (its repositories stay)"
                    onClick={() => setDraft(list.filter((_, j) => j !== i))}
                  >
                    <Trash2 />
                  </Button>
                )}
              </div>
              <Labelled label="Repositories">
                {t.repos.length ? (
                  t.repos.map((r) => (
                    <Removable key={r} onRemove={admin ? () => edit(i, { repos: t.repos.filter((x) => x !== r) }) : undefined}>
                      {r}
                    </Removable>
                  ))
                ) : (
                  <span className="text-xs text-muted-foreground">None yet</span>
                )}
              </Labelled>
              {missing.length > 0 && (
                <p className="text-xs text-warning">
                  Not in the index: {missing.join(", ")}.{" "}
                  {admin && (
                    <button className="underline underline-offset-2" onClick={() => edit(i, { repos: t.repos.filter((r) => !missing.includes(r)) })}>
                      Remove {missing.length === 1 ? "it" : "them"}
                    </button>
                  )}
                </p>
              )}
              {server && (
                <Labelled label="Members">
                  {t.members.length ? (
                    t.members.map((m) => (
                      <Removable key={m} onRemove={admin ? () => edit(i, { members: t.members.filter((x) => x !== m) }) : undefined}>
                        {m}
                      </Removable>
                    ))
                  ) : (
                    <span className="text-xs text-muted-foreground">Nobody yet; people join under the team menu at the top</span>
                  )}
                </Labelled>
              )}
              <label className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                Notes from other teams on its code
                {admin ? (
                  <select
                    value={t.incoming}
                    onChange={(e) => edit(i, { incoming: e.target.value as TeamDef["incoming"] })}
                    className="h-8 rounded-md border border-input bg-card px-2 text-xs text-foreground"
                  >
                    {Object.entries(INCOMING).map(([k, v]) => (
                      <option key={k} value={k}>
                        {v}
                      </option>
                    ))}
                  </select>
                ) : (
                  <span className="text-foreground">{INCOMING[t.incoming]}</span>
                )}
              </label>
            </div>
          );
        })}
        {admin && (
          <div className="flex flex-wrap items-center gap-2">
            <Input
              value={fresh}
              onChange={(e) => setFresh(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && (e.preventDefault(), add())}
              placeholder="New team name"
              aria-label="New team name"
              className="h-9 max-w-56"
            />
            <Button variant="outline" onClick={add} disabled={!fresh.trim()}>
              <Plus /> Add team
            </Button>
            <div className="ml-auto flex items-center gap-2">
              {dirty && (
                <Button variant="ghost" onClick={() => setDraft(null)}>
                  Discard
                </Button>
              )}
              <Button variant="brand" disabled={!dirty} loading={save.isPending} onClick={() => save.mutate()}>
                Save teams
              </Button>
            </div>
          </div>
        )}
        {save.error && <p className="text-sm text-danger">{save.error.message}</p>}
      </CardContent>
    </Card>
  );
}

function Labelled({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      <span className="w-24 shrink-0 text-xs text-muted-foreground">{label}</span>
      {children}
    </div>
  );
}

function Removable({ onRemove, children }: { onRemove?: () => void; children: React.ReactNode }) {
  return (
    <Chip>
      <span className="flex items-center gap-1">
        {children}
        {onRemove && (
          <button onClick={onRemove} aria-label={`Remove ${children}`} className="relative after:absolute after:-inset-1.5 after:content-[''] -mr-0.5 text-muted-foreground hover:text-foreground">
            <X className="size-3" />
          </button>
        )}
      </span>
    </Chip>
  );
}
