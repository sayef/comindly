import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { RefreshCcw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Link, useNavigate } from "react-router";
import { type Direction, TreeView, useOrg } from "@/components/org-views";
import { Backdrop } from "@/components/backdrop";
import { OrgNetwork } from "@/components/org-network";
import { RepoDot } from "@/components/symbol";
import { PageHeader } from "@/components/page-header";
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
import { cn, repoHue } from "@/lib/utils";

const sentenceCase = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);
const short = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(n >= 10_000 ? 0 : 1)}k` : String(n));
const share = (a: number, b: number) => (b ? `${((a / b) * 100).toFixed(a / b < 0.1 ? 1 : 0)} %` : "0 %");

/** A per-browser preference, or `fallback` when storage is unavailable. */
function remembered(key: string, fallback: string) {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}

type Part = { key: string; value: number; color: string; label: string };

/** A figure, the bar that breaks it down, and what the bar shows; links to where the figure comes from. */
function Stat({ label, value, parts, total, hint, to }: { label: string; value: string; parts: Part[]; total?: number; hint: string; to?: string }) {
  const sum = total ?? parts.reduce((n, p) => n + p.value, 0);
  const body = (
    <>
      <div className="text-xs font-semibold text-muted-foreground">{label}</div>
      <div className="nums text-[26px] leading-tight font-semibold tracking-[-0.02em]">{value}</div>
      <div className="mt-1 flex h-1.5 gap-px overflow-hidden rounded-full bg-muted" aria-hidden="true">
        {sum > 0 &&
          parts
            .filter((p) => p.value > 0)
            .map((p) => (
              <span
                key={p.key}
                title={`${p.label}: ${p.value.toLocaleString()}`}
                className="h-full min-w-[3px]"
                style={{ width: `${(p.value / sum) * 100}%`, background: p.color }}
              />
            ))}
      </div>
      <div className="truncate text-xs text-muted-foreground" title={hint}>
        {hint}
      </div>
    </>
  );
  const box = "flex min-w-0 flex-col gap-1 bg-card px-5 py-4";
  return to ? (
    <Link to={to} className={`${box} transition-colors duration-100 hover:bg-muted/60`}>
      {body}
    </Link>
  ) : (
    <div className={box}>{body}</div>
  );
}

const KIND_NAME: Record<string, string> = { http: "HTTP", sqs: "SQS", sns: "SNS", s3: "S3", table: "tables" };
const KIND_TONE: Record<string, number> = { http: 100, table: 70, sqs: 50, sns: 38, s3: 28 };

function Panel({ title, to, link, children }: { title: string; to: string; link: string; children: React.ReactNode }) {
  return (
    <section className="overflow-hidden rounded-lg border bg-card">
      <div className="flex items-center justify-between gap-3 border-b px-5 py-3.5">
        <h2 className="text-sm font-semibold">{title}</h2>
        <Link to={to} className="-my-1 flex min-h-7 items-center px-1 text-xs font-semibold text-brand hover:underline">
          {link}
        </Link>
      </div>
      {children}
    </section>
  );
}

/** What the org map says first: the repository most others depend on, and any cycles. */
function finding(org: ReturnType<typeof useOrg>): string | null {
  if (org.repos.length < 2) return null;
  const hub = [...org.usedBy.entries()].sort((a, b) => b[1].size - a[1].size || a[0].localeCompare(b[0]))[0];
  if (!hub || hub[1].size === 0) return null;
  return `${hub[1].size} of ${org.repos.length} repositories depend on ${hub[0]}`;
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
      <h2 className="font-semibold">Join a team</h2>
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
      <h2 className="font-semibold">Create a team</h2>
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
  const [mode, setMode] = useState<"network" | "tree">(() => (remembered("comindly.org.view", "network") === "tree" ? "tree" : "network"));
  useEffect(() => {
    try {
      localStorage.setItem("comindly.org.view", mode);
    } catch {
      // storage may be unavailable
    }
  }, [mode]);
  const contracts = useQuery({ queryKey: ["contracts"], queryFn: api.contracts });
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
        <h1 className="text-[26px] font-semibold tracking-[-0.02em]">{teamName ? `You're in ${teamName}` : "Welcome to comindly"}</h1>
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
        <h1 className="text-[26px] font-semibold tracking-[-0.02em]">Welcome to comindly</h1>
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
        <h1 className="text-[26px] font-semibold tracking-[-0.02em]">You're in {teamName}</h1>
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
        <h1 className="text-[26px] font-semibold tracking-[-0.02em]">Welcome to comindly</h1>
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

  const said = finding(org);
  const bySize = (o?.repos ?? []).slice().sort((a, b) => b.symbols - a.symbols);
  const kinds = Object.entries(
    (contracts.data ?? []).reduce<Record<string, number>>((m, v) => {
      const k = v.contract.name.split(":")[0];
      m[k] = (m[k] ?? 0) + 1;
      return m;
    }, {}),
  ).sort((a, b) => b[1] - a[1]);

  return (
    <div className="flex flex-col gap-5">
      <PageHeader title="Overview">How the team's repositories depend on each other, in code and through service contracts.</PageHeader>

      {o ? (
        <div className="grid grid-cols-2 gap-px overflow-hidden rounded-lg border bg-border lg:grid-cols-4">
          <Stat
            label="Repositories"
            value={o.repos.length.toLocaleString()}
            to="/settings/repositories"
            parts={bySize.map((r) => ({ key: r.repo, value: r.symbols, label: r.repo, color: `hsl(${repoHue(r.repo)} 62% 58%)` }))}
            hint={`${o.symbols.toLocaleString()} symbols; ${bySize
              .slice(0, 2)
              .map((r) => `${r.repo} ${short(r.symbols)}`)
              .join(", ")}${bySize.length > 2 ? ` and ${bySize.length - 2} more` : ""}`}
          />
          <Stat
            label="Links across repositories"
            value={o.cross_repo_edges.toLocaleString()}
            parts={[{ key: "cross", value: o.cross_repo_edges, label: "Across repositories", color: "var(--color-brand)" }]}
            total={o.edges}
            hint={`${share(o.cross_repo_edges, o.edges)} of ${o.edges.toLocaleString()} dependencies cross a repository`}
          />
          <Stat
            label="Contracts"
            value={o.contracts.toLocaleString()}
            to="/contracts"
            parts={kinds.map(([k, n]) => ({
              key: k,
              value: n,
              label: KIND_NAME[k] ?? k,
              color: `color-mix(in srgb, var(--color-brand) ${KIND_TONE[k] ?? 30}%, var(--color-muted))`,
            }))}
            hint={kinds.length ? kinds.map(([k, n]) => `${KIND_NAME[k] ?? k} ${n}`).join(", ") : `${o.linked_contracts} with both sides indexed`}
          />
          <Stat
            label="Ledger records"
            value={totalRecords.toLocaleString()}
            to="/ledger"
            parts={Object.entries(o.ledger).map(([k, n]) => ({
              key: k,
              value: n,
              label: k,
              color: k === "stale" ? "var(--color-warning)" : k === "orphaned" ? "var(--color-danger)" : k === "active" || k === "moved" ? "var(--color-brand)" : "var(--color-input)",
            }))}
            hint={
              totalRecords === 0
                ? "none yet"
                : Object.entries(o.ledger)
                    .filter(([, n]) => n > 0)
                    .map(([k, n]) => `${n} ${k}`)
                    .join(", ") + (needs.length ? `; ${needs.length} to review` : "")
            }
          />
        </div>
      ) : (
        <Skeleton className="h-[106px] rounded-lg" />
      )}

      <div className="flex flex-col gap-6">
        <Card className="min-w-0 overflow-hidden">
          <div className="flex flex-wrap items-center gap-3 border-b px-5 py-3">
            <div className="flex min-w-0 flex-1 flex-col">
              <h2 className="text-sm font-semibold">Org map</h2>
              {said && <p className="text-xs text-muted-foreground">{said}</p>}
            </div>
            {org.cycles.length > 0 && (
              <div className="relative">
                <button
                  onClick={() => setShowCycles((v) => !v)}
                  className="flex h-8 items-center gap-1.5 rounded-md bg-warning/12 px-2.5 text-xs font-semibold text-warning"
                >
                  <RefreshCcw className="size-3.5" /> {org.cycles.length} {org.cycles.length === 1 ? "cycle" : "cycles"}
                </button>
                {showCycles && (
                  <div className="absolute right-0 z-20 mt-1 flex w-72 flex-col gap-0.5 rounded-lg bg-popover p-1.5 shadow-float">
                    <p className="px-2 py-1.5 text-xs text-muted-foreground">
                      These repositories depend on each other in a circle, so none of them can change or release on its own.
                    </p>
                    {org.cycles.map((c) => (
                      <button
                        key={c.join()}
                        onClick={() => (setMode("tree"), setStart(c[0]), setDir("uses"), setOpen(new Set([c[0]])), setSelected(null), setShowCycles(false))}
                        className="rounded-md px-2 py-1.5 text-left font-mono text-xs hover:bg-muted"
                      >
                        {c.join(", ")}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}
            <div className="flex h-8 rounded-lg bg-secondary p-0.5" role="group" aria-label="View">
              {(
                [
                  ["network", "Network"],
                  ["tree", "Tree"],
                ] as const
              ).map(([m, label]) => (
                <button
                  key={m}
                  aria-pressed={mode === m}
                  onClick={() => setMode(m)}
                  className={`rounded-md px-2.5 text-xs font-semibold transition-colors duration-100 ${mode === m ? "bg-card text-foreground shadow-[0_1px_2px_var(--shadow-ink)]" : "text-muted-foreground hover:text-foreground"}`}
                >
                  {label}
                </button>
              ))}
            </div>
            {mode === "tree" && (
            <>
            <label className="flex items-center gap-2 text-xs text-muted-foreground">
              Start from
              <select
                value={start}
                onChange={(e) => (
                  setStart(e.target.value),
                  setOpen(new Set(e.target.value === "*" ? [] : [e.target.value])),
                  setSelected(null)
                )}
                className="h-8 rounded-md border border-input bg-card px-2 text-xs text-foreground"
              >
                <option value="*">{dir === "uses" ? "All entry points" : "All shared code"}</option>
                {org.repos.map((r) => (
                  <option key={r} value={r}>
                    {r}
                  </option>
                ))}
              </select>
            </label>
            <div className="flex h-8 rounded-lg bg-secondary p-0.5" role="group" aria-label="Direction">
              {(
                [
                  ["uses", "What it uses"],
                  ["usedBy", "What uses it"],
                ] as const
              ).map(([d, label]) => (
                <button
                  key={d}
                  aria-pressed={dir === d}
                  onClick={() => (setDir(d), setOpen(new Set(start === "*" ? [] : [start])), setSelected(null))}
                  className={`rounded-md px-2.5 text-xs font-semibold transition-colors duration-100 ${dir === d ? "bg-card text-foreground shadow-[0_1px_2px_var(--shadow-ink)]" : "text-muted-foreground hover:text-foreground"}`}
                >
                  {label}
                </button>
              ))}
            </div>
            </>
            )}
          </div>
          {!map ? (
            <Skeleton className="h-[420px] rounded-none" />
          ) : mode === "network" && org.repos.length > 1 ? (
            <OrgNetwork org={org} />
          ) : roots.length === 0 ? (
            <p className="px-5 py-8 text-sm text-muted-foreground">No links between the indexed repositories yet.</p>
          ) : (
            <TreeView org={org} roots={roots} dir={dir} open={openKeys} onToggle={toggle} selected={selected} />
          )}
          <div className="border-t px-5 py-3 text-xs text-muted-foreground">
            {mode === "network"
              ? "Circle size is the number of symbols; an arrow points from a repository to what it uses, thicker for more links. Green goes through a contract, dashed amber is a cycle. Hover to see neighbours, click for details, drag to rearrange; pinch or Ctrl-scroll to zoom."
              : "Click a box to open or close it; each line names the contract it goes through. A dashed box is shown elsewhere in the tree; an amber line loops back where repositories depend on each other in a circle."}
          </div>
          {versions.data && versions.data.length > 1 && (
            <div className="border-t px-5 py-3.5">
              <div className="flex items-center justify-between">
                <span className="eyebrow">Index version</span>
                <span className="nums text-xs text-muted-foreground">
                  {(versions.data[at ?? last] ?? "").slice(0, 16).replace("T", " ")}
                  {(at ?? last) === last ? ", the latest" : `, ${(at ?? last) + 1} of ${last + 1}`}
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
                <Link key={e.record.id} to="/ledger" className="flex flex-col gap-1 border-b px-5 py-3 last:border-0 hover:bg-muted/60">
                  <span className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Badge variant={isChallenged(e) ? "warning" : ["stale", "orphaned"].includes(e.status) ? "danger" : "default"}>
                      {isChallenged(e) ? "Disputed" : ["stale", "orphaned"].includes(e.status) ? sentenceCase(e.status) : "Unconfirmed"}
                    </Badge>
                    {sentenceCase(e.record.kind)}
                    {e.record.anchors[0] ? ` in ${e.record.anchors[0].repo}` : ""}
                  </span>
                  <span>{e.record.title}</span>
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
                  className="flex items-center gap-3 border-b px-5 py-3 last:border-0 hover:bg-muted/60"
                >
                  <span className="flex min-w-0 flex-1 items-center gap-2">
                    <RepoDot repo={f.entry.repo} />
                    <span className="truncate font-mono text-xs">{f.entry.name}</span>
                    <span className="truncate text-xs text-muted-foreground">{f.entry.repo}</span>
                  </span>
                  <span className="nums text-xs text-muted-foreground">{f.repos.length} repositories</span>
                  <span className="h-1 w-16 overflow-hidden rounded-full bg-muted">
                    <span className="block h-1 bg-brand" style={{ width: `${(f.repos.length / most) * 100}%` }} />
                  </span>
                </Link>
              );
            })}
            {flows.data?.length === 0 && <p className="px-5 py-4 text-sm text-muted-foreground">No flows yet. They appear once repositories are indexed.</p>}
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
                    <span className="nums w-14 text-right text-xs text-muted-foreground">
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
            <Link to="/packages" className="text-xs font-semibold text-brand hover:underline">
              Packages
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
                        {s.published} <span className="font-sans text-xs">from {s.publisher}</span>
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
