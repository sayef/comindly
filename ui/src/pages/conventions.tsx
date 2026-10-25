import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowDownUp, ChevronDown, ChevronRight, Code2, Pencil, Plus, RotateCcw, Search, Trash2 } from "lucide-react";
import { Fragment, useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { RepoDot } from "@/components/symbol";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip } from "@/components/ui/tooltip";
import { api, type Convention, type ConventionChange } from "@/lib/api";
import { cn, sentence } from "@/lib/utils";

const ORG = "*";
const scopeName = (r: string) => (r === ORG ? "Org-wide" : r);
const originName = { measured: "Measured", edited: "Edited", manual: "Team" } as const;
const textarea =
  "min-h-20 w-full rounded-md border bg-card px-3 py-2 text-sm shadow-xs outline-none focus-visible:ring-2 focus-visible:ring-ring";
const select = "h-9 rounded-md border bg-card px-2 text-sm";

function useChange() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (c: ConventionChange) => api.changeConvention(c),
    meta: { inline: true },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["conventions"] }),
  });
}

/** `path:line` in `repo`, or `repo:path:line` for org-wide conventions. */
function place(repo: string, at: string): [string, string] {
  if (repo !== ORG) return [repo, at];
  const i = at.indexOf(":");
  return i > 0 ? [at.slice(0, i), at.slice(i + 1)] : [repo, at];
}

/** One code location, with its lines on demand. */
function Where({ repo, at, kind }: { repo: string; at: string; kind: "follows" | "deviates" }) {
  const [open, setOpen] = useState(false);
  const [r, loc] = place(repo, at);
  const src = useQuery({ queryKey: ["source", r, loc], queryFn: () => api.source(r, loc), enabled: open, retry: false });
  return (
    <li className="flex flex-col gap-1">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <Badge variant={kind === "follows" ? "brand" : "warning"}>{kind === "follows" ? "Follows" : "Deviates"}</Badge>
        {r !== ORG && <RepoDot repo={r} />}
        <span className="font-mono [overflow-wrap:anywhere]">{repo === ORG ? `${r}: ${loc}` : loc}</span>
        <Button size="sm" variant="ghost" className="h-6 px-2 text-xs" loading={open && src.isLoading} onClick={() => setOpen(!open)}>
          {!(open && src.isLoading) && <Code2 />} {open ? "Hide code" : "Show code"}
        </Button>
      </div>
      {open && src.error && <p className="text-xs text-muted-foreground">{sentence(src.error.message)}</p>}
      {open && src.data && (
        <pre className="overflow-x-auto rounded-md border bg-muted/40 py-2 font-mono text-[11px] leading-relaxed">
          {src.data.lines.map((l) => (
            <div key={l.n} className={cn("flex gap-3 px-3", l.n === src.data.line && "bg-brand/15")}>
              <span className="w-8 shrink-0 select-none text-right text-muted-foreground">{l.n}</span>
              <span className="whitespace-pre">{l.text}</span>
            </div>
          ))}
        </pre>
      )}
    </li>
  );
}

function Row({ c, writable }: { c: Convention; writable: boolean }) {
  const change = useChange();
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(c.rule);
  const pct = c.total ? Math.round((c.support / c.total) * 100) : 0;
  const where = c.exemplars.length + c.counterexamples.length;
  const busy = (action: string) => change.isPending && change.variables?.action === action;

  return (
    <Fragment>
      <tr className="group border-b align-top last:border-0">
        <td className="w-8 py-2.5 pl-3">
          {where > 0 && (
            <button className="text-muted-foreground hover:text-foreground" onClick={() => setOpen(!open)} aria-label={open ? "Hide where it comes from" : "Show where it comes from"} title="Where it comes from">
              {open ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
            </button>
          )}
        </td>
        <td className="py-2.5 pr-4">
          {editing ? (
            <div className="flex flex-col gap-2">
              <textarea className={textarea} value={text} onChange={(e) => setText(e.target.value)} autoFocus />
              {change.error && <p className="text-xs text-danger [overflow-wrap:anywhere]">{sentence(change.error.message)}</p>}
              <div className="flex gap-2">
                <Button size="sm" variant="brand" disabled={!text.trim()} loading={busy("edit")} onClick={() => change.mutate({ action: "edit", id: c.id, rule: text }, { onSuccess: () => setEditing(false) })}>
                  Save
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setEditing(false)}>
                  Cancel
                </Button>
              </div>
            </div>
          ) : (
            <span className="text-sm leading-snug">{c.rule}</span>
          )}
        </td>
        <td className="whitespace-nowrap py-2.5 pr-4 text-xs">
          <span className="flex items-center gap-1.5">
            {c.repo !== ORG && <RepoDot repo={c.repo} />}
            {scopeName(c.repo)}
          </span>
        </td>
        <td className="py-2.5 pr-4 text-xs text-muted-foreground">{c.dimension}</td>
        <td className="py-2.5 pr-4">
          <Badge variant={c.origin === "manual" ? "brand" : "outline"}>{originName[c.origin]}</Badge>
        </td>
        <td className="w-36 py-2.5 pr-4">
          {c.origin !== "manual" && c.total > 0 && (
            <div className="flex items-center gap-2" title={`${c.support} of ${c.total} follow it`}>
              <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
                <div className={cn("h-full rounded-full", pct >= 80 ? "bg-brand" : "bg-warning")} style={{ width: `${pct}%` }} />
              </div>
              <span className="font-mono text-[11px] tabular-nums text-muted-foreground">{pct}%</span>
            </div>
          )}
        </td>
        <td className="w-24 py-2 pr-3">
          {writable && !editing && (
            <div className="flex justify-end gap-0.5 opacity-60 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
              <Tooltip content="Edit">
                <Button size="icon" variant="ghost" className="size-7" onClick={() => (setText(c.rule), setEditing(true))} aria-label="Edit">
                  <Pencil />
                </Button>
              </Tooltip>
              {c.origin === "edited" && (
                <Tooltip content="Back to the measured rule">
                  <Button size="icon" variant="ghost" className="size-7" loading={busy("restore")} onClick={() => change.mutate({ action: "restore", id: c.id })} aria-label="Restore">
                    {!busy("restore") && <RotateCcw />}
                  </Button>
                </Tooltip>
              )}
              <Tooltip content={c.origin === "manual" ? "Delete" : "Hide (can be restored)"}>
                <Button size="icon" variant="ghost" className="size-7" loading={busy("hide")} onClick={() => change.mutate({ action: "hide", id: c.id })} aria-label="Remove">
                  {!busy("hide") && <Trash2 />}
                </Button>
              </Tooltip>
            </div>
          )}
        </td>
      </tr>
      {open && (
        <tr className="border-b bg-muted/20">
          <td />
          <td colSpan={6} className="py-3 pr-4">
            <p className="mb-2 text-xs text-muted-foreground">
              {c.origin === "manual" ? "Set by the team." : `Measured from the code: ${c.support} of ${c.total} follow it.`} Where it comes from:
            </p>
            <ul className="flex flex-col gap-2">
              {c.exemplars.map((x) => (
                <Where key={`f-${x}`} repo={c.repo} at={x} kind="follows" />
              ))}
              {c.counterexamples.map((x) => (
                <Where key={`d-${x}`} repo={c.repo} at={x} kind="deviates" />
              ))}
            </ul>
          </td>
        </tr>
      )}
    </Fragment>
  );
}

function AddDialog({ open, onOpenChange, scopes, scope, dimensions }: { open: boolean; onOpenChange: (o: boolean) => void; scopes: string[]; scope: string; dimensions: string[] }) {
  const change = useChange();
  const [repo, setRepo] = useState(scope);
  const [dimension, setDimension] = useState("");
  const [rule, setRule] = useState("");
  const submit = () =>
    change.mutate(
      { action: "add", repo, dimension: dimension.trim() || "team", rule },
      { onSuccess: () => (setRule(""), setDimension(""), onOpenChange(false)) },
    );
  return (
    <Dialog open={open} onOpenChange={(o) => (o && setRepo(scope), onOpenChange(o))}>
      <DialogContent className="p-5">
        <DialogTitle className="text-base font-semibold">Add a convention</DialogTitle>
        <p className="mt-1 text-sm text-muted-foreground">Agents see it through the MCP guide, marked as set by the team.</p>
        <div className="mt-4 flex flex-col gap-3">
          <label className="flex flex-col gap-1 text-xs font-medium">
            Applies to
            <select className="h-9 rounded-md border bg-card px-2 text-sm" value={repo} onChange={(e) => setRepo(e.target.value)}>
              {scopes.map((r) => (
                <option key={r} value={r}>
                  {scopeName(r)}
                </option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium">
            Topic
            <Input list="dimensions" value={dimension} onChange={(e) => setDimension(e.target.value)} placeholder="e.g. errors, libraries, testing" />
            <datalist id="dimensions">
              {dimensions.map((d) => (
                <option key={d} value={d} />
              ))}
            </datalist>
          </label>
          <label className="flex flex-col gap-1 text-xs font-medium">
            Rule
            <textarea className={textarea} value={rule} onChange={(e) => setRule(e.target.value)} placeholder="One sentence an agent can follow." />
          </label>
          {change.error && <p className="text-xs text-danger [overflow-wrap:anywhere]">{sentence(change.error.message)}</p>}
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button variant="brand" disabled={!rule.trim()} loading={change.isPending} onClick={submit}>
              Add
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

type Sort = "scope" | "topic" | "strength";

export function ConventionsPage() {
  const all = useQuery({ queryKey: ["conventions"], queryFn: api.conventions });
  const overview = useQuery({ queryKey: ["overview"], queryFn: api.overview });
  const writable = overview.data?.writable ?? false;
  const change = useChange();
  const items = all.data?.conventions ?? [];
  const scopes = useMemo(() => {
    const names = new Set([ORG, ...items.map((c) => c.repo), ...(overview.data?.repos.map((r) => r.repo) ?? [])]);
    return [...names].sort((a, b) => (a === ORG ? -1 : b === ORG ? 1 : a.localeCompare(b)));
  }, [items, overview.data]);
  const dimensions = useMemo(() => [...new Set(items.map((c) => c.dimension))].sort(), [items]);
  const [params, setParams] = useSearchParams();
  const scope = params.get("repo") ?? "";
  const setScope = (r: string) => setParams(r ? { repo: r } : {}, { replace: true });
  const [topic, setTopic] = useState("");
  const [origin, setOrigin] = useState("");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>("scope");
  const [adding, setAdding] = useState(false);

  const rows = useMemo(() => {
    const q = query.trim().toLowerCase();
    const strength = (c: Convention) => (c.total ? c.support / c.total : 1);
    return items
      .filter((c) => (!scope || c.repo === scope) && (!topic || c.dimension === topic) && (!origin || c.origin === origin))
      .filter((c) => !q || `${c.rule} ${c.dimension} ${c.repo}`.toLowerCase().includes(q))
      .sort((a, b) =>
        sort === "strength"
          ? strength(b) - strength(a)
          : sort === "topic"
            ? a.dimension.localeCompare(b.dimension) || a.repo.localeCompare(b.repo)
            : (a.repo === ORG ? -1 : b.repo === ORG ? 1 : a.repo.localeCompare(b.repo)) || a.dimension.localeCompare(b.dimension),
      );
  }, [items, scope, topic, origin, query, sort]);
  const hidden = (all.data?.hidden ?? []).filter((h) => !scope || h.repo === scope);

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-[28px] font-semibold tracking-[-0.035em]">Conventions</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Measured from the code, plus the team's own: how each repository names, structures, logs, configures and tests things.
          </p>
        </div>
        {writable && (
          <Button variant="brand" onClick={() => setAdding(true)}>
            <Plus /> Add convention
          </Button>
        )}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative w-full max-w-xs">
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Filter rules" className="pl-8" />
        </div>
        <select className={select} value={scope} onChange={(e) => setScope(e.target.value)} aria-label="Scope">
          <option value="">All scopes</option>
          {scopes.map((r) => (
            <option key={r} value={r}>
              {scopeName(r)}
            </option>
          ))}
        </select>
        <select className={select} value={topic} onChange={(e) => setTopic(e.target.value)} aria-label="Topic">
          <option value="">All topics</option>
          {dimensions.map((d) => (
            <option key={d} value={d}>
              {d}
            </option>
          ))}
        </select>
        <select className={select} value={origin} onChange={(e) => setOrigin(e.target.value)} aria-label="Origin">
          <option value="">Any origin</option>
          <option value="measured">Measured</option>
          <option value="edited">Edited</option>
          <option value="manual">Team</option>
        </select>
        <label className="flex items-center gap-1.5 text-sm text-muted-foreground">
          <ArrowDownUp className="size-4" />
          <select className={select} value={sort} onChange={(e) => setSort(e.target.value as Sort)} aria-label="Sort">
            <option value="scope">By scope</option>
            <option value="topic">By topic</option>
            <option value="strength">By strength</option>
          </select>
        </label>
        <span className="text-xs text-muted-foreground">
          {rows.length} of {items.length}
        </span>
      </div>
      <Card className="overflow-x-auto">
        {all.isLoading ? (
          <Skeleton className="h-64" />
        ) : rows.length === 0 ? (
          <p className="p-6 text-center text-sm text-muted-foreground">{items.length === 0 ? "No conventions yet." : "No conventions match."}</p>
        ) : (
          <table className="w-full text-left">
            <thead className="border-b">
              <tr>
                <th className="w-8" />
                <th className="eyebrow h-9 pr-4 font-normal">Rule</th>
                <th className="eyebrow h-9 pr-4 font-normal">Scope</th>
                <th className="eyebrow h-9 pr-4 font-normal">Topic</th>
                <th className="eyebrow h-9 pr-4 font-normal">Origin</th>
                <th className="eyebrow h-9 pr-4 font-normal">Followed</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {rows.map((c) => (
                <Row key={`${c.id}-${c.rule}`} c={c} writable={writable} />
              ))}
            </tbody>
          </table>
        )}
      </Card>
      {hidden.length > 0 && (
        <details className="rounded-xl border bg-card p-4">
          <summary className="cursor-pointer text-sm font-medium">Hidden ({hidden.length})</summary>
          <div className="mt-3 flex flex-col gap-2">
            {hidden.map((h) => (
              <div key={h.id} className="flex items-center gap-3 text-sm text-muted-foreground">
                <Badge variant="outline">{h.dimension}</Badge>
                <span className="text-xs">{scopeName(h.repo)}</span>
                <span className="flex-1 line-through decoration-muted-foreground/50">{h.rule}</span>
                {writable && (
                  <Button size="sm" variant="outline" loading={change.isPending && (change.variables as { id?: string } | undefined)?.id === h.id} onClick={() => change.mutate({ action: "restore", id: h.id })}>
                    <RotateCcw /> Restore
                  </Button>
                )}
              </div>
            ))}
          </div>
        </details>
      )}
      <AddDialog open={adding} onOpenChange={setAdding} scopes={scopes} scope={scope || ORG} dimensions={dimensions} />
    </div>
  );
}
