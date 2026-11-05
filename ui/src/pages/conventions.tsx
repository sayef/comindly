import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowDownUp, ChevronDown, ChevronRight, Code2, ExternalLink, GitCommitHorizontal, Pencil, Plus, RotateCcw, Search, Trash2 } from "lucide-react";
import { Fragment, useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { RepoDot } from "@/components/symbol";
import { PageHeader } from "@/components/page-header";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Table, Td, Th, Tr } from "@/components/ui/table";
import { Tooltip } from "@/components/ui/tooltip";
import { ConfirmButton } from "@/components/confirm";
import { withCode } from "@/components/code-text";
import { api, type Convention, type ConventionChange } from "@/lib/api";
import { useTeams } from "@/lib/teams";
import { cn, percent, sentence } from "@/lib/utils";

const ORG = "*";
const scopeName = (r: string) => (r === ORG ? "Org-wide" : r);
const originName = { measured: "Measured", edited: "Edited", manual: "Org" } as const;
const textarea =
  "min-h-20 w-full rounded-md border border-input bg-card px-3 py-2 text-sm outline-none transition-[border-color,box-shadow] duration-100 hover:border-muted-foreground/50 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/20 max-sm:text-base";
const select = "h-9 min-w-0 rounded-md max-sm:w-full border border-input bg-card px-2 text-sm transition-colors duration-100 hover:border-muted-foreground/50";

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

/** A commit a recipe was learned from (`sha subject`), linked to its host when known. */
function Commit({ repo, at, kind, urls }: { repo: string; at: string; kind: "follows" | "deviates"; urls: Record<string, string> }) {
  const [r, ref] = place(repo, at);
  const i = ref.indexOf(" ");
  const [sha, subject] = i > 0 ? [ref.slice(0, i), ref.slice(i + 1)] : [ref, ""];
  const url = urls[r];
  return (
    <li className="flex flex-wrap items-center gap-2 text-xs">
      <Badge variant={kind === "follows" ? "brand" : "warning"}>{kind === "follows" ? "Follows" : "Deviates"}</Badge>
      {r !== ORG && <RepoDot repo={r} />}
      <GitCommitHorizontal className="size-3.5 text-muted-foreground" />
      {url ? (
        <a className="font-mono text-brand hover:underline" href={url + sha} target="_blank" rel="noreferrer">
          {sha}
        </a>
      ) : (
        <span className="font-mono">{sha}</span>
      )}
      <span className="[overflow-wrap:anywhere]">{repo === ORG ? `${r}: ${subject}` : subject}</span>
    </li>
  );
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
        <Button size="sm" variant="ghost" className="h-7 px-2" loading={open && src.isLoading} onClick={() => setOpen(!open)}>
          {!(open && src.isLoading) && <Code2 />} {open ? "Hide code" : "Show code"}
        </Button>
      </div>
      {open && src.error && <p className="text-xs text-muted-foreground">{sentence(src.error.message)}</p>}
      {open && src.data && (
        <pre className="overflow-x-auto rounded-md bg-muted py-2 font-mono text-xs leading-relaxed">
          {src.data.lines.map((l) => (
            <div key={l.n} className={cn("flex gap-3 px-3", l.n === src.data.line && "bg-brand/12")}>
              <span className="nums w-8 shrink-0 select-none text-right text-muted-foreground">{l.n}</span>
              <span className="whitespace-pre">{l.text}</span>
            </div>
          ))}
        </pre>
      )}
      {open && src.data?.link && (
        <a className="flex items-center gap-1.5 self-start text-xs font-semibold text-brand hover:underline" href={src.data.link} target="_blank" rel="noreferrer">
          <ExternalLink className="size-3.5" /> Open at {src.data.commit?.slice(0, 8)}
        </a>
      )}
    </li>
  );
}

/** The rule, short enough for a button's accessible name. */
const named = (rule: string) => {
  const t = rule.replace(/`/g, "");
  return t.length > 60 ? `${t.slice(0, 57)}…` : t;
};

function Row({ c, writable, commitUrls }: { c: Convention; writable: boolean; commitUrls: Record<string, string> }) {
  const change = useChange();
  const [open, setOpen] = useState(false);
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(c.rule);
  const pct = c.total ? Math.round((c.support / c.total) * 100) : 0;
  const where = c.exemplars.length + c.counterexamples.length;
  const recipe = c.dimension === "recipes";
  const busy = (action: string) => change.isPending && change.variables?.action === action;
  const name = named(c.rule);

  return (
    <Fragment>
      <Tr className="group align-top hover:bg-transparent max-sm:pl-[52px]">
        <Td slot="lead" className="w-10 py-2 pr-0 pl-2 max-sm:-ml-[40px] max-sm:w-7 max-sm:self-start max-sm:pl-0">
          {where > 0 && (
            <button
              className="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground pointer-coarse:size-10"
              onClick={() => setOpen(!open)}
              aria-expanded={open}
              aria-label={`Where it comes from: ${name}`}
              title="Where it comes from"
            >
              {open ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
            </button>
          )}
        </Td>
        <Td slot="main" className="py-2.5 pr-4 pl-0 max-sm:self-center">
          {editing ? (
            <div className="flex flex-col gap-2">
              <textarea className={textarea} value={text} onChange={(e) => setText(e.target.value)} aria-label="Rule" autoFocus />
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
            <span className="leading-snug [overflow-wrap:anywhere]">{withCode(c.rule)}</span>
          )}
        </Td>
        <Td slot="meta" className="py-2.5 pr-4 pl-0 text-xs whitespace-nowrap max-sm:order-6">
          <span className="flex items-center gap-1.5">
            {c.repo !== ORG && <RepoDot repo={c.repo} />}
            {scopeName(c.repo)}
          </span>
        </Td>
        <Td slot="meta" className="py-2.5 pr-4 pl-0 text-xs text-muted-foreground max-sm:order-5">
          {c.dimension.charAt(0).toUpperCase() + c.dimension.slice(1)}
        </Td>
        <Td slot="meta" className={cn("origin-col py-2.5 pr-4 pl-0 max-sm:order-6", c.origin === "measured" && "max-sm:hidden")}>
          {c.origin !== "measured" && <Badge variant={c.origin === "manual" ? "brand" : "outline"}>{originName[c.origin]}</Badge>}
        </Td>
        <Td slot="meta" className={cn("w-36 py-2.5 pr-4 pl-0 text-right max-sm:w-auto max-sm:text-left", (c.origin === "manual" || c.total === 0) && "max-sm:hidden")}>
          {c.origin !== "manual" && c.total > 0 && (
            <div className="flex items-center gap-2" title={`${c.support} of ${c.total} follow it`}>
              <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted max-sm:hidden">
                <div className={cn("h-full rounded-full", pct >= 80 ? "bg-brand" : "bg-muted-foreground/45")} style={{ width: `${pct}%` }} />
              </div>
              <span className="nums w-10 shrink-0 text-right text-xs text-muted-foreground max-sm:w-auto">{percent(c.support, c.total)}</span>
            </div>
          )}
        </Td>
        <Td slot="end" className="w-px py-2 pr-3 pl-0 max-sm:w-auto max-sm:self-start">
          {writable && !editing && (
            <div className="flex justify-end gap-0.5 opacity-60 transition-opacity group-hover:opacity-100 focus-within:opacity-100 max-sm:opacity-100">
              <Tooltip content="Edit">
                <Button size="icon" variant="ghost" className="size-7" onClick={() => (setText(c.rule), setEditing(true))} aria-label={`Edit: ${name}`}>
                  <Pencil />
                </Button>
              </Tooltip>
              {c.origin === "edited" && (
                <Tooltip content="Back to the measured rule">
                  <Button size="icon" variant="ghost" className="size-7" loading={busy("restore")} onClick={() => change.mutate({ action: "restore", id: c.id })} aria-label={`Restore the measured rule: ${name}`}>
                    {!busy("restore") && <RotateCcw />}
                  </Button>
                </Tooltip>
              )}
              {c.origin === "manual" ? (
                <ConfirmButton
                  size="icon"
                  variant="ghost"
                  className="size-7"
                  title="Delete"
                  aria-label={`Delete: ${name}`}
                  question="Delete for everyone?"
                  confirmLabel="Delete"
                  loading={busy("hide")}
                  onConfirm={() => change.mutate({ action: "hide", id: c.id })}
                >
                  {!busy("hide") && <Trash2 />}
                </ConfirmButton>
              ) : (
                <Tooltip content="Hide (can be restored)">
                  <Button size="icon" variant="ghost" className="size-7" loading={busy("hide")} onClick={() => change.mutate({ action: "hide", id: c.id })} aria-label={`Hide: ${name}`}>
                    {!busy("hide") && <Trash2 />}
                  </Button>
                </Tooltip>
              )}
            </div>
          )}
        </Td>
      </Tr>
      {open && (
        <Tr className="bg-muted/20 hover:bg-muted/20">
          <Td slot="desktop" />
          <Td slot="full" colSpan={6} className="py-3 pr-4 pl-0 max-sm:mt-0">
            <p className="mb-2 text-xs text-muted-foreground">
              {c.origin === "manual"
                ? "Set by the org."
                : `Measured from ${recipe ? "commit history" : "the code"}: ${c.support} of ${c.total} follow it.`}{" "}
              Where it comes from:
            </p>
            <ul className="flex flex-col gap-2">
              {c.exemplars.map((x) =>
                recipe ? <Commit key={`f-${x}`} repo={c.repo} at={x} kind="follows" urls={commitUrls} /> : <Where key={`f-${x}`} repo={c.repo} at={x} kind="follows" />,
              )}
              {c.counterexamples.map((x) =>
                recipe ? <Commit key={`d-${x}`} repo={c.repo} at={x} kind="deviates" urls={commitUrls} /> : <Where key={`d-${x}`} repo={c.repo} at={x} kind="deviates" />,
              )}
            </ul>
          </Td>
        </Tr>
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
        <DialogTitle className="text-[17px] font-semibold">Add a convention</DialogTitle>
        <p className="mt-1 text-sm text-muted-foreground">Agents see it through the MCP guide, marked as set by the org.</p>
        <div className="mt-4 flex flex-col gap-3">
          <label className="flex flex-col gap-1 text-xs font-medium">
            Applies to
            <select className={select} value={repo} onChange={(e) => setRepo(e.target.value)}>
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
  const teams = useTeams();
  const all = useQuery({
    queryKey: ["conventions"],
    queryFn: api.conventions,
    select: (d) => ({ ...d, conventions: d.conventions.filter((c) => c.repo === ORG || teams.inView(c.repo)) }),
  });
  const overview = useQuery({
    queryKey: ["overview"],
    queryFn: api.overview,
    select: (d) => ({ ...d, repos: d.repos.filter((r) => teams.inView(r.repo)) }),
  });
  const writable = overview.data?.writable ?? false;
  const change = useChange();
  const items = all.data?.conventions ?? [];
  const scopes = useMemo(() => {
    const names = new Set([ORG, ...items.map((c) => c.repo), ...(overview.data?.repos.map((r) => r.repo) ?? [])]);
    return [...names].sort((a, b) => (a === ORG ? -1 : b === ORG ? 1 : a.localeCompare(b)));
  }, [items, overview.data]);
  const dimensions = useMemo(() => [...new Set(items.map((c) => c.dimension))].sort(), [items]);
  const [params, setParams] = useSearchParams();
  const set = (k: string, v: string) => setParams((p) => (v ? p.set(k, v) : p.delete(k), p), { replace: true });
  const scope = params.get("repo") ?? "";
  const setScope = (v: string) => set("repo", v);
  const topic = params.get("topic") ?? "";
  const origin = params.get("origin") ?? "";
  const query = params.get("q") ?? "";
  const sort: Sort = (["topic", "strength"] as const).find((x) => x === params.get("sort")) ?? "scope";
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
      <PageHeader
        title="Conventions"
        actions={
          writable && (
            <Button variant="brand" onClick={() => setAdding(true)}>
              <Plus /> Add convention
            </Button>
          )
        }
      >
        Measured from the code, plus the team's own: how each repository names, structures, logs, configures and tests things.
      </PageHeader>
      <div className="grid grid-cols-2 items-center gap-2 sm:flex sm:flex-wrap">
        <select className={select} value={scope} onChange={(e) => setScope(e.target.value)} aria-label="Scope">
          <option value="">All scopes</option>
          {scopes.map((r) => (
            <option key={r} value={r}>
              {scopeName(r)}
            </option>
          ))}
        </select>
        <select className={select} value={topic} onChange={(e) => set("topic", e.target.value)} aria-label="Topic">
          <option value="">All topics</option>
          {dimensions.map((d) => (
            <option key={d} value={d}>
              {d.charAt(0).toUpperCase() + d.slice(1)}
            </option>
          ))}
        </select>
        <select className={select} value={origin} onChange={(e) => set("origin", e.target.value)} aria-label="Origin">
          <option value="">Any origin</option>
          <option value="measured">Measured</option>
          <option value="edited">Edited</option>
          <option value="manual">Org</option>
        </select>
        <label className="flex items-center gap-1.5 text-sm text-muted-foreground">
          <ArrowDownUp className="size-4 max-sm:hidden" />
          <select className={select} value={sort} onChange={(e) => set("sort", e.target.value === "scope" ? "" : e.target.value)} aria-label="Sort">
            <option value="scope">By scope</option>
            <option value="topic">By topic</option>
            <option value="strength">By strength</option>
          </select>
        </label>
        <span className="nums text-xs text-muted-foreground max-sm:order-last max-sm:col-span-2">
          {rows.length} of {items.length}
        </span>
        <div className="relative col-span-2 w-full max-sm:order-first sm:ml-auto sm:w-auto sm:min-w-48 sm:max-w-xs sm:flex-1">
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input value={query} onChange={(e) => set("q", e.target.value)} placeholder="Filter rules" aria-label="Filter rules" className="pl-8" />
        </div>
      </div>
      <Card>
        {all.isLoading ? (
          <Skeleton className="h-64" />
        ) : rows.length === 0 ? (
          <p className="px-5 py-8 text-muted-foreground">
            {items.length === 0
              ? "No conventions yet. They are measured when repositories are indexed; you can also add the team's own."
              : "No convention matches these filters. Clear the filter text or widen the scope."}
          </p>
        ) : (
          <Table reflow className={cn(!rows.some((c) => c.origin !== "measured") && "[&_.origin-col]:hidden")}>
            <thead className="border-b">
              <tr>
                <th className="w-8" />
                <Th className="pl-0">Rule</Th>
                <Th className="pl-0">Scope</Th>
                <Th className="pl-0">Topic</Th>
                <Th className="origin-col pl-0">Origin</Th>
                <Th className="pl-0">Followed</Th>
                <th>
                  <span className="sr-only">Actions</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((c) => (
                <Row key={`${c.id}-${c.rule}`} c={c} writable={writable} commitUrls={all.data?.commit_urls ?? {}} />
              ))}
            </tbody>
          </Table>
        )}
      </Card>
      {hidden.length > 0 && (
        <details className="rounded-lg border bg-card p-4">
          <summary className="cursor-pointer font-semibold">Hidden conventions ({hidden.length})</summary>
          <div className="mt-3 flex flex-col gap-2">
            {hidden.map((h) => (
              <div key={h.id} className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-muted-foreground">
                <Badge variant="outline">{h.dimension}</Badge>
                <span className="text-xs">{scopeName(h.repo)}</span>
                <span className="min-w-0 flex-1 line-through decoration-muted-foreground/50 [overflow-wrap:anywhere] max-sm:order-last max-sm:basis-full">{h.rule}</span>
                {writable && (
                  <Button size="sm" variant="outline" loading={change.isPending && (change.variables as { id?: string } | undefined)?.id === h.id} onClick={() => change.mutate({ action: "restore", id: h.id })} aria-label={`Restore: ${named(h.rule)}`}>
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
