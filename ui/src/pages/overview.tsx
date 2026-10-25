import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowUpRight } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "react-router";
import { type Direction, TreeView, useOrg } from "@/components/org-views";
import { Backdrop } from "@/components/backdrop";
import { RepoDot } from "@/components/symbol";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Slider } from "@/components/ui/slider";
import { Table, Td, Th, Tr } from "@/components/ui/table";
import { Outcome } from "@/components/notice";
import { api, attention, isChallenged, type OrgMap } from "@/lib/api";
import { useAccess } from "@/lib/me";
import { cn } from "@/lib/utils";

/** A per-browser preference, or `fallback` when storage is unavailable. */
function remembered(key: string, fallback: string) {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}

function Stat({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="flex flex-col gap-1.5 bg-card px-[18px] py-4">
      <span className="eyebrow">{label}</span>
      <span className="nums text-[30px] font-medium leading-none tracking-[-0.02em]">{value}</span>
      {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
    </div>
  );
}

function Panel({ title, to, link, children }: { title: string; to: string; link: string; children: React.ReactNode }) {
  return (
    <section className="overflow-hidden rounded-[10px] border bg-card">
      <div className="flex items-center justify-between border-b px-5 py-3.5">
        <h2 className="text-sm font-semibold">{title}</h2>
        <Link to={to} className="text-xs text-muted-foreground hover:text-foreground">
          {link} →
        </Link>
      </div>
      {children}
    </section>
  );
}


function linkDiff(then: OrgMap, now: OrgMap) {
  const key = (e: OrgMap["edges"][number]) => `${e.src} → ${e.dst} (${e.kind})`;
  const a = new Set(then.edges.map(key));
  const b = new Set(now.edges.map(key));
  return { added: [...b].filter((k) => !a.has(k)), gone: [...a].filter((k) => !b.has(k)) };
}

/** Where a team's index lives: a folder or a bucket location. */
function Location({ index, setIndex }: { index: string; setIndex: (v: string) => void }) {
  return (
    <Input
      value={index}
      onChange={(e) => setIndex(e.target.value)}
      placeholder="s3://, gs://, az:// or a folder"
      className="font-mono text-xs"
    />
  );
}

/** First run: use the team's index. */
function JoinTeam() {
  const qc = useQueryClient();
  const [index, setIndex] = useState("");
  const join = useMutation({ mutationFn: () => api.join(index), meta: { inline: true }, onSuccess: () => qc.invalidateQueries() });
  return (
    <Card className="flex flex-col p-4 text-left">
      <div className="text-sm font-medium">Join a team</div>
      <p className="mt-1 text-xs text-muted-foreground">
        Where your team's index lives. Bucket access uses the credentials this machine already has.
      </p>
      <div className="mt-3 flex flex-1 flex-col gap-2">
        <Location index={index} setIndex={setIndex} />
        <div className="mt-auto">
          <Button variant="brand" disabled={!index.trim()} loading={join.isPending} onClick={() => join.mutate()}>
            Join
          </Button>
        </div>
        <Outcome error={join.error} />
      </div>
    </Card>
  );
}

/** First run: start a team at an empty folder or bucket location; colleagues then join it. */
/** `at`: the location a server's deployment set; then only the name is asked. */
function CreateTeam({ at }: { at?: string }) {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [index, setIndex] = useState(at ?? "");
  const create = useMutation({
    mutationFn: () => api.createTeam(name, index),
    meta: { inline: true },
    onSuccess: () => (qc.invalidateQueries(), navigate("/settings/repositories")),
  });
  return (
    <Card className="flex flex-col p-4 text-left">
      <div className="text-sm font-medium">Create a team</div>
      <p className="mt-1 text-xs text-muted-foreground">
        {at
          ? "The index lives where the deployment set it. You add repositories next."
          : "Pick an empty folder or bucket location everyone can reach. You add repositories next; colleagues join with the same location."}
      </p>
      <div className="mt-3 flex flex-1 flex-col gap-2">
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="Team name, e.g. Platform" />
        {!at && <Location index={index} setIndex={setIndex} />}
        <div className="mt-auto">
          <Button variant="brand" disabled={!name.trim() || !index.trim()} loading={create.isPending} onClick={() => create.mutate()}>
            Create team
          </Button>
        </div>
        <Outcome error={create.error} />
      </div>
    </Card>
  );
}

export function OverviewPage() {
  const { server, admin } = useAccess();
  const overview = useQuery({ queryKey: ["overview"], queryFn: api.overview });
  const ledger = useQuery({ queryKey: ["ledger"], queryFn: api.ledger });
  const flows = useQuery({ queryKey: ["flows"], queryFn: api.flows });
  const latest = useQuery({ queryKey: ["orgmap"], queryFn: () => api.orgmap() });
  const versions = useQuery({ queryKey: ["versions"], queryFn: api.versions });
  const last = (versions.data?.length ?? 1) - 1;
  const [at, setAt] = useState<number | null>(null);
  const past = useQuery({
    queryKey: ["orgmap", at],
    queryFn: () => api.orgmap(at!),
    enabled: at !== null && at !== last,
  });
  const map = at !== null && at !== last ? past.data : latest.data;
  const [selected, setSelected] = useState<string | null>(null);
  const org = useOrg(map);
  const [start, setStart] = useState(() => remembered("comindly.org.start", "*"));
  const [dir, setDir] = useState<Direction>(() => (remembered("comindly.org.dir", "uses") === "usedBy" ? "usedBy" : "uses"));
  const [open, setOpen] = useState<Set<string> | null>(null);
  const [showCycles, setShowCycles] = useState(false);
  useEffect(() => {
    try {
      localStorage.setItem("comindly.org.start", start);
      localStorage.setItem("comindly.org.dir", dir);
    } catch {
      // storage may be unavailable
    }
  }, [start, dir]);
  const roots = useMemo(() => {
    if (start !== "*" && org.repos.includes(start)) return [start];
    const all = dir === "uses" ? org.entries : org.foundations;
    return all.length ? all : org.repos.slice(0, 1);
  }, [org, start, dir]);
  // Until someone opens or closes a box, each starting repository shows what it uses directly.
  const openKeys = useMemo(() => open ?? new Set(roots), [open, roots]);
  const toggle = (key: string) => {
    const next = new Set(openKeys);
    if (next.has(key)) for (const k of [...next]) k === key || k.startsWith(`${key}/`) ? next.delete(k) : null;
    else next.add(key);
    setOpen(next);
    setSelected(key);
  };
  const diff = at !== null && at !== last && past.data && latest.data ? linkDiff(past.data, latest.data) : null;
  const o = overview.data;
  const team = useQuery({ queryKey: ["team"], queryFn: api.team, retry: false });
  const teamName = team.data?.version != null ? (team.data.settings.name ?? "your team") : null;
  const [switching, setSwitching] = useState(false);

  if (o?.problem && server && !(admin && !teamName)) {
    return (
      <Welcome>
        <h1 className="text-2xl font-semibold tracking-[-0.03em]">{teamName ? `You're in ${teamName}` : "Welcome to comindly"}</h1>
        <p className="text-sm text-muted-foreground">
          {admin
            ? "Nothing is indexed yet. Add the team's repositories and index them; everyone signed in here then sees the same graph."
            : "Nothing is indexed yet. An admin adds the team's repositories under Settings → Repositories; this page fills in once they are indexed."}
        </p>
        {admin && (
          <Button variant="brand" asChild>
            <Link to="/settings/repositories">Add repositories</Link>
          </Button>
        )}
      </Welcome>
    );
  }

  if (o?.problem && server) {
    return (
      <Welcome>
        <h1 className="text-2xl font-semibold tracking-[-0.03em]">Welcome to comindly</h1>
        <p className="text-sm text-muted-foreground">Name the team; then add its repositories and index them.</p>
        <div className="w-full max-w-sm">
          <CreateTeam at={o.root ?? undefined} />
        </div>
      </Welcome>
    );
  }

  if (o?.problem && teamName && !switching) {
    return (
      <Welcome>
        <h1 className="text-2xl font-semibold tracking-[-0.03em]">You're in {teamName}</h1>
        <p className="text-sm text-muted-foreground">
          Nothing is indexed yet. Add the team's repositories and index them; everyone in {teamName} then sees the same graph.
        </p>
        <div className="flex flex-wrap justify-center gap-2">
          <Button variant="brand" asChild>
            <Link to="/settings/repositories">Add repositories</Link>
          </Button>
          <Button variant="outline" asChild>
            <Link to="/settings">Team settings</Link>
          </Button>
        </div>
        <button className="text-xs text-muted-foreground underline underline-offset-2" onClick={() => setSwitching(true)}>
          Join or create a different team
        </button>
      </Welcome>
    );
  }

  if (o?.problem) {
    return (
      <Welcome wide>
        <h1 className="text-2xl font-semibold tracking-[-0.03em]">Welcome to comindly</h1>
        <p className="text-sm text-muted-foreground">Join your team, start one, or index just for yourself.</p>
        <div className="grid w-full gap-3 sm:grid-cols-2">
          <JoinTeam />
          <CreateTeam />
        </div>
        <p className="text-xs text-muted-foreground">
          Just for yourself?{" "}
          <Link to="/settings/repositories" className="underline underline-offset-2">
            Add repositories
          </Link>{" "}
          and index them here, or change the location in{" "}
          <Link to="/settings" className="underline underline-offset-2">
            Settings
          </Link>
          .
        </p>
        <details className="text-left text-xs text-muted-foreground">
          <summary className="cursor-pointer">Details</summary>
          <p className="mt-2">
            Looked in <span className="font-mono [overflow-wrap:anywhere]">{o.root}</span>
          </p>
          <pre className="mt-1 whitespace-pre-wrap [overflow-wrap:anywhere]">{o.problem}</pre>
        </details>
      </Welcome>
    );
  }

  const totalRecords = o ? Object.values(o.ledger).reduce((a, b) => a + b, 0) : 0;
  const needs = (ledger.data ?? []).filter((e) => attention(e) < 3).sort((a, b) => attention(a) - attention(b));

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-[28px] font-semibold tracking-[-0.035em]">Overview</h1>
          <p className="mt-1.5 text-sm text-muted-foreground">
            How the team's repositories depend on each other, in code and through service contracts.
          </p>
        </div>
      </div>

      {o ? (
        <div className="grid grid-cols-2 gap-px overflow-hidden rounded-[10px] border bg-border lg:grid-cols-4">
          <Stat label="Repositories" value={String(o.repos.length)} hint={`${o.symbols.toLocaleString()} symbols indexed`} />
          <Stat
            label="Cross-repo edges"
            value={o.cross_repo_edges.toLocaleString()}
            hint={`of ${o.edges.toLocaleString()} dependency edges`}
          />
          <Stat label="Contracts" value={String(o.contracts)} hint={`${o.linked_contracts} with both sides indexed`} />
          <Stat
            label="Ledger records"
            value={String(totalRecords)}
            hint={needs.length ? `${needs.length} need review` : "none need review"}
          />
        </div>
      ) : (
        <Skeleton className="h-[106px] rounded-[10px]" />
      )}

      <div className="flex flex-col gap-6">
        <Card className="min-w-0 overflow-hidden">
          <div className="flex flex-wrap items-center gap-3 border-b px-5 py-3">
            <h2 className="flex-1 text-sm font-semibold">Org map</h2>
            {org.cycles.length > 0 && (
              <div className="relative">
                <button
                  onClick={() => setShowCycles((v) => !v)}
                  className="flex h-8 items-center gap-1.5 rounded-md border border-warning/40 bg-warning/10 px-2.5 text-xs font-medium text-warning"
                >
                  ↺ {org.cycles.length} {org.cycles.length === 1 ? "cycle" : "cycles"}
                </button>
                {showCycles && (
                  <div className="absolute right-0 z-20 mt-1 flex w-72 flex-col gap-0.5 rounded-lg border bg-popover p-1.5 shadow-lg">
                    <p className="px-2 py-1.5 text-xs text-muted-foreground">
                      These repositories depend on each other in a circle, so none of them can change or release on its own.
                    </p>
                    {org.cycles.map((c) => (
                      <button
                        key={c.join()}
                        onClick={() => (setStart(c[0]), setDir("uses"), setOpen(new Set([c[0]])), setSelected(null), setShowCycles(false))}
                        className="rounded-md px-2 py-1.5 text-left font-mono text-xs hover:bg-muted"
                      >
                        {c.join(" ⇄ ")}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}
            <label className="flex items-center gap-2 text-xs text-muted-foreground">
              Start from
              <select
                value={start}
                onChange={(e) => (
                  setStart(e.target.value),
                  setOpen(new Set(e.target.value === "*" ? [] : [e.target.value])),
                  setSelected(null)
                )}
                className="h-8 rounded-md border border-input bg-card px-2 text-[13px] text-foreground"
              >
                <option value="*">{dir === "uses" ? "All entry points" : "All shared code"}</option>
                {org.repos.map((r) => (
                  <option key={r} value={r}>
                    {r}
                  </option>
                ))}
              </select>
            </label>
            <div className="flex rounded-lg border bg-card p-0.5" role="group" aria-label="Direction">
              {(
                [
                  ["uses", "What it uses →"],
                  ["usedBy", "← What uses it"],
                ] as const
              ).map(([d, label]) => (
                <button
                  key={d}
                  aria-pressed={dir === d}
                  onClick={() => (setDir(d), setOpen(new Set(start === "*" ? [] : [start])), setSelected(null))}
                  className={`rounded-md px-2.5 py-1 text-xs font-medium transition-colors ${dir === d ? "bg-muted text-foreground" : "text-muted-foreground hover:text-foreground"}`}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
          {!map ? (
            <Skeleton className="h-[420px] rounded-none" />
          ) : roots.length === 0 ? (
            <p className="px-5 py-8 text-sm text-muted-foreground">No links between the indexed repositories yet.</p>
          ) : (
            <TreeView org={org} roots={roots} dir={dir} open={openKeys} onToggle={toggle} selected={selected} />
          )}
          <div className="border-t px-5 py-3 font-mono text-xs text-muted-foreground">
            Click a box to open or close it; each line names the contract it goes through. A dashed box is shown elsewhere in the tree; an
            amber line loops back where repositories depend on each other in a circle.
          </div>
          {versions.data && versions.data.length > 1 && (
            <div className="border-t px-5 py-3.5">
              <div className="flex items-center justify-between">
                <span className="eyebrow">Index version</span>
                <span className="font-mono text-xs text-muted-foreground">
                  {(versions.data[at ?? last] ?? "").slice(0, 16).replace("T", " ")}
                  {(at ?? last) === last ? " · latest" : ` · ${(at ?? last) + 1} of ${last + 1}`}
                </span>
              </div>
              <Slider className="mt-3" min={0} max={last} step={1} value={[at ?? last]} onValueChange={([v]) => setAt(v)} />
              {diff && (
                <div className="mt-3 flex flex-wrap gap-1.5 text-xs">
                  {diff.added.length + diff.gone.length === 0 && (
                    <span className="text-muted-foreground">No repository links changed since then.</span>
                  )}
                  {diff.added.map((k) => (
                    <Badge key={k} variant="brand">
                      + {k}
                    </Badge>
                  ))}
                  {diff.gone.map((k) => (
                    <Badge key={k} variant="danger">
                      − {k}
                    </Badge>
                  ))}
                </div>
              )}
            </div>
          )}
        </Card>

        <div className="grid min-w-0 gap-6 lg:grid-cols-2">
          <Panel title="Needs attention" to="/ledger" link="Ledger">
            {needs.length === 0 ? (
              <p className="px-5 py-4 text-sm text-muted-foreground">
                Nothing to review. Records that go stale or get disputed show up here.
              </p>
            ) : (
              needs.slice(0, 4).map((e) => (
                <Link key={e.record.id} to="/ledger" className="flex flex-col gap-1.5 border-b px-5 py-3 last:border-0 hover:bg-muted/40">
                  <span className="eyebrow flex items-center gap-2">
                    <span
                      className={
                        isChallenged(e) ? "text-warning" : ["stale", "orphaned"].includes(e.status) ? "text-danger" : "text-[#8fb8ff]"
                      }
                    >
                      {isChallenged(e) ? "disputed" : ["stale", "orphaned"].includes(e.status) ? e.status : "unconfirmed"}
                    </span>
                    {e.record.kind} · {e.record.anchors[0]?.repo ?? "—"}
                  </span>
                  <span className="text-[13.5px]">{e.record.title}</span>
                </Link>
              ))
            )}
          </Panel>
          <Panel title="Flows that reach furthest" to="/flows" link="Flows">
            {(flows.data ?? []).slice(0, 4).map((f) => {
              const most = Math.max(...(flows.data ?? []).map((x) => x.repos.length), 1);
              return (
                <Link
                  key={f.entry.id}
                  to={`/flows?id=${encodeURIComponent(f.entry.id)}`}
                  className="flex items-center gap-3 border-b px-5 py-3 last:border-0 hover:bg-muted/40"
                >
                  <span className="flex min-w-0 flex-1 items-center gap-2">
                    <RepoDot repo={f.entry.repo} />
                    <span className="truncate font-mono text-[13px]">{f.entry.name}</span>
                    <span className="truncate text-xs text-muted-foreground">{f.entry.repo}</span>
                  </span>
                  <span className="text-xs text-muted-foreground">{f.repos.length} repos</span>
                  <span className="h-1 w-16 overflow-hidden rounded-full bg-muted">
                    <span className="block h-1 bg-brand" style={{ width: `${(f.repos.length / most) * 100}%` }} />
                  </span>
                </Link>
              );
            })}
            {flows.data?.length === 0 && <p className="px-5 py-4 text-sm text-muted-foreground">No flows yet.</p>}
          </Panel>
        </div>
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Repositories</CardTitle>
            <CardDescription>Indexed symbols per repository.</CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-2.5">
            {o?.repos
              .slice()
              .sort((a, b) => b.symbols - a.symbols)
              .map((r) => {
                const max = Math.max(...o.repos.map((x) => x.symbols));
                return (
                  <div key={r.repo} className="flex items-center gap-3 text-sm">
                    <RepoDot repo={r.repo} />
                    <span className="w-40 truncate">{r.repo}</span>
                    <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
                      <div className="h-full rounded-full bg-brand/70" style={{ width: `${(r.symbols / max) * 100}%` }} />
                    </div>
                    <span className="w-14 text-right font-mono text-xs tabular-nums text-muted-foreground">
                      {r.symbols.toLocaleString()}
                    </span>
                  </div>
                );
              })}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex-row items-start justify-between">
            <div>
              <CardTitle>Version skew</CardTitle>
              <CardDescription>Internal packages pinned to a different version than their publisher.</CardDescription>
            </div>
            <Link to="/packages" className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
              Packages <ArrowUpRight className="size-3" />
            </Link>
          </CardHeader>
          <CardContent>
            {o && o.skew.length === 0 && <p className="text-sm text-muted-foreground">Every consumer pins its publisher's version.</p>}
            {o && o.skew.length > 0 && (
              <Table>
                <thead>
                  <tr className="border-b">
                    <Th>Package</Th>
                    <Th>Consumer</Th>
                    <Th>Pinned</Th>
                    <Th>Published</Th>
                  </tr>
                </thead>
                <tbody>
                  {o.skew.map((s) => (
                    <Tr key={`${s.package}-${s.consumer}`}>
                      <Td className="font-medium">{s.package}</Td>
                      <Td>{s.consumer}</Td>
                      <Td className="font-mono text-xs">{s.locked}</Td>
                      <Td className="font-mono text-xs text-muted-foreground">
                        {s.published} <span className="text-[10px]">({s.publisher})</span>
                      </Td>
                    </Tr>
                  ))}
                </tbody>
              </Table>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

/** A first visit: the logo over the repository graph, which spans the whole main area, then what to do. */
function Welcome({ wide, children }: { wide?: boolean; children: React.ReactNode }) {
  return (
    <div className="flex min-h-[calc(100vh-60px-3.5rem)] items-center">
      <Backdrop />
      <div className={cn("relative mx-auto flex flex-col items-center gap-4 py-16 text-center", wide ? "max-w-3xl" : "max-w-xl")}>
        <img src="/logo.svg" alt="" className="size-12 dark:hidden" />
        <img src="/logo-dark.svg" alt="" className="hidden size-12 dark:block" />
        {children}
      </div>
    </div>
  );
}
