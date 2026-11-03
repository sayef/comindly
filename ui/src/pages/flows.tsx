import { useQuery } from "@tanstack/react-query";
import { ArrowRight, ChevronDown, ChevronLeft, ChevronRight, Search, Sparkles } from "lucide-react";
import { useMemo, useState } from "react";
import { withCode } from "@/components/code-text";
import { Link, useSearchParams } from "react-router";
import { RepoDot, SymbolLink } from "@/components/symbol";
import { PageHeader } from "@/components/page-header";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { api, type FlowStep, type Node } from "@/lib/api";
import { cn } from "@/lib/utils";

/** Repositories in the order a flow first reaches them. */
function journey(entry: Node, steps: FlowStep[]) {
  const seen = [entry.repo];
  for (const s of steps) if (s.node.kind !== "Contract" && !seen.includes(s.node.repo)) seen.push(s.node.repo);
  return seen;
}

function Journey({ repos }: { repos: string[] }) {
  return (
    <div className="flex flex-wrap items-center gap-1 text-xs">
      {repos.map((r, i) => (
        <span key={r} className="flex items-center gap-1">
          {i > 0 && <ArrowRight className="size-3 text-muted-foreground" />}
          <span className="flex min-w-0 items-center gap-1.5 rounded-sm bg-muted px-2 py-0.5 [overflow-wrap:anywhere]">
            <RepoDot repo={r} /> {r}
          </span>
        </span>
      ))}
    </div>
  );
}


/** A contract name with its kind word in the text face, as Contracts shows it. */
function ContractName({ name }: { name: string }) {
  const label = contractLabel(name);
  const at = label.indexOf(" ");
  if (!name.includes(":") || name.startsWith("http:") || at < 0) return <>{label}</>;
  return (
    <>
      <span className="font-sans font-semibold">{label.slice(0, at)}</span> {label.slice(at + 1)}
    </>
  );
}

/** A contract as Contracts writes it: `POST /path`, `Table name`, `SQS name`. */
function contractLabel(name: string) {
  const i = name.indexOf(":");
  if (i < 0) return name;
  const [kind, rest] = [name.slice(0, i), name.slice(i + 1)];
  return kind === "http" ? rest : `${kind === "table" ? "Table" : kind.toUpperCase()} ${rest}`;
}

const MAX_DEPTH = 12;
const indent = "ml-6 border-l pl-2 max-sm:ml-1 max-sm:pl-1.5";

type Tree = { children: Map<string, FlowStep[]>; size: Map<string, number>; crosses: Set<string> };

function Step({ t, s, repo, depth }: { t: Tree; s: FlowStep; repo: string; depth: number }) {
  const contract = s.node.kind === "Contract";
  const into = !contract && s.node.repo !== repo;
  const n = depth < MAX_DEPTH ? (t.size.get(s.node.id) ?? 0) : 0;
  const foldable = n >= 3 && !t.crosses.has(s.node.id);
  const [open, setOpen] = useState(!foldable);
  return (
    <li className="flex flex-col">
      <div className={cn("flex flex-wrap items-center gap-x-2 gap-y-0.5 rounded-md px-2 py-1", contract && "bg-accent")}>
        <span className="w-20 shrink-0 text-right text-xs text-muted-foreground max-sm:w-auto max-sm:text-left">{s.via}</span>
        {contract ? (
          <span className="min-w-0 font-mono text-xs text-accent-foreground [overflow-wrap:anywhere]"><ContractName name={s.node.name} /></span>
        ) : (
          <SymbolLink node={s.node} showRepo={false} />
        )}
        {into && <Badge variant="brand">Into {s.node.repo}</Badge>}
        {foldable && (
          <button
            className="flex items-center gap-0.5 rounded-sm text-xs text-muted-foreground hover:text-foreground"
            onClick={() => setOpen(!open)}
            aria-expanded={open}
          >
            {open ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
            <span className="nums">{n} steps</span>
          </button>
        )}
      </div>
      {open && n > 0 && <Steps t={t} id={s.node.id} repo={contract ? repo : s.node.repo} depth={depth + 1} />}
    </li>
  );
}

function Steps({ t, id, repo, depth }: { t: Tree; id: string; repo: string; depth: number }) {
  return (
    <ul className={indent}>
      {(t.children.get(id) ?? []).map((s) => (
        <Step key={s.node.id} t={t} s={s} repo={repo} depth={depth} />
      ))}
    </ul>
  );
}

/** The flow as a tree: each step under the step that led to it; same-repository subtrees fold. */
function Path({ entry, steps }: { entry: Node; steps: FlowStep[] }) {
  const tree = useMemo(() => {
    const children = new Map<string, FlowStep[]>();
    for (const s of steps) children.set(s.from, [...(children.get(s.from) ?? []), s]);
    const size = new Map<string, number>();
    const crosses = new Set<string>();
    const lines = new Map<string, { from: string; contract?: string; to: string }>();
    // Returns whether the subtree under `id` leaves `repo` or touches a contract.
    const walk = (id: string, repo: string, depth: number, via?: string): boolean => {
      let n = 0;
      let out = false;
      for (const s of children.get(id) ?? []) {
        const contract = s.node.kind === "Contract";
        if (!contract && s.node.repo !== repo) {
          const line = { from: repo, contract: via, to: s.node.repo };
          lines.set(`${line.from}|${line.contract ?? ""}|${line.to}`, line);
        }
        let sub = contract || s.node.repo !== repo;
        if (depth < MAX_DEPTH) sub = walk(s.node.id, contract ? repo : s.node.repo, depth + 1, contract ? contractLabel(s.node.name) : undefined) || sub;
        n += 1 + (size.get(s.node.id) ?? 0);
        out ||= sub;
      }
      size.set(id, n);
      if (out) crosses.add(id);
      return out;
    };
    walk(entry.id, entry.repo, 0);
    return { children, size, crosses, crossings: [...lines.values()] };
  }, [entry, steps]);
  const { crossings } = tree;

  return (
    <div className="flex flex-col gap-4 text-sm">
      {crossings.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <span className="eyebrow">Crossings</span>
          {crossings.map((c) => (
            <div key={`${c.from}|${c.contract}|${c.to}`} className="flex flex-wrap items-center gap-1.5 text-xs">
              <span className="flex items-center gap-1.5">
                <RepoDot repo={c.from} /> {c.from}
              </span>
              <ArrowRight className="size-3 text-muted-foreground" />
              {c.contract && (
                <>
                  <span className="min-w-0 rounded-sm bg-accent px-1.5 py-0.5 font-mono text-accent-foreground [overflow-wrap:anywhere]">{c.contract}</span>
                  <ArrowRight className="size-3 text-muted-foreground" />
                </>
              )}
              <span className="flex items-center gap-1.5">
                <RepoDot repo={c.to} /> {c.to}
              </span>
            </div>
          ))}
        </div>
      )}
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-2 px-2 py-1">
          <span className="w-20 shrink-0 text-right text-xs text-muted-foreground max-sm:w-auto">Start</span>
          <SymbolLink node={entry} />
        </div>
        <Steps t={tree} id={entry.id} repo={entry.repo} depth={0} />
      </div>
    </div>
  );
}

function FlowView({ id }: { id: string }) {
  const flow = useQuery({ queryKey: ["flow", id], queryFn: () => api.flow(id) });
  const f = flow.data;
  return (
    <div className="flex flex-col gap-5">
      <Link to="/flows" className="-mt-2 flex min-h-9 w-fit items-center gap-1 text-sm font-semibold text-muted-foreground hover:text-foreground">
        <ChevronLeft className="size-4" /> All flows
      </Link>
      {!f ? (
        <Skeleton className="h-64" />
      ) : (
        <>
          <div className="flex flex-col gap-2">
            <h1 className="font-mono text-[26px] font-semibold tracking-[-0.02em] [overflow-wrap:anywhere]">{f.entry.name}</h1>
            <Journey repos={journey(f.entry, f.steps)} />
          </div>
          {f.narration && (
            <Card>
              <CardHeader>
                <h2 className="flex items-center gap-2 text-sm font-semibold">
                  <Sparkles className="size-4 text-brand" /> Walkthrough
                </h2>
              </CardHeader>
              <CardContent className="flex flex-col gap-3 text-sm leading-relaxed">
                {f.narration.split("\n\n").map((p, i) => (
                  <p key={i}>{withCode(p)}</p>
                ))}
                {f.questions && f.questions.length > 0 && (
                  <p className="text-xs text-muted-foreground">Answers: {withCode(f.questions.join("; "))}</p>
                )}
              </CardContent>
            </Card>
          )}
          <Card>
            <CardHeader>
              <h2 className="text-sm font-semibold">Path</h2>
              <CardDescription className="max-w-[70ch]">
                Every step from the entry point into other repositories. Highlighted steps are contracts; steps that stay in one
                repository fold.
              </CardDescription>
            </CardHeader>
            <CardContent className="overflow-x-auto">
              <Path entry={f.entry} steps={f.steps} />
            </CardContent>
          </Card>
        </>
      )}
    </div>
  );
}

export function FlowsPage() {
  const [params, setParams] = useSearchParams();
  const id = params.get("id");
  const flows = useQuery({ queryKey: ["flows"], queryFn: api.flows, enabled: !id });
  const filter = params.get("q") ?? "";
  const across = params.get("within") !== "1";
  const set = (k: string, v: string) => setParams((p) => (v ? p.set(k, v) : p.delete(k), p), { replace: true });
  const setAcross = (on: boolean) => set("within", on ? "" : "1");
  if (id) return <FlowView id={id} />;
  const rows = (flows.data ?? [])
    .filter((f) => !across || f.repos.length > 1)
    .filter((f) => {
      const q = filter.trim().toLowerCase();
      return !q || [f.entry.name, ...f.repos, ...f.contracts, ...f.contracts.map(contractLabel)].some((x) => x.toLowerCase().includes(q));
    });
  const total = flows.data?.length ?? 0;

  return (
    <div className="flex flex-col gap-5">
      <PageHeader title="Flows">
        Where work starts and where it travels: calls, then across HTTP routes, topics and queues into the services that handle them.
        With LLM summaries on when indexing, flows also get a written walkthrough.
      </PageHeader>
      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={across} onChange={(e) => setAcross(e.target.checked)} />
          Only flows across repositories
        </label>
        <span className="nums text-xs text-muted-foreground">
          {rows.length} of {total}
        </span>
        <div className="relative w-full sm:ml-auto sm:max-w-xs">
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input
            value={filter}
            onChange={(e) => set("q", e.target.value)}
            placeholder="Filter by name, repository or contract"
            aria-label="Filter flows"
            spellCheck={false}
            className="pl-8"
          />
        </div>
      </div>
      <Card className="overflow-hidden">
        {flows.isLoading && <Skeleton className="h-40" />}
        {flows.data && rows.length === 0 && (
          <p className="px-5 py-8 text-muted-foreground">
            {total === 0 ? "No flows in this index yet. They appear once repositories are indexed." : "No flow matches."}{" "}
            {across && total > 0 && (
              <button className="font-semibold text-brand hover:underline" onClick={() => setAcross(false)}>
                Show flows within one repository too
              </button>
            )}
          </p>
        )}
        {rows.map((f) => (
          <Link
            key={f.entry.id}
            to={`/flows?id=${encodeURIComponent(f.entry.id)}`}
            className="flex min-w-0 flex-col gap-2 border-b px-5 py-3.5 last:border-0 hover:bg-muted/60 max-sm:px-4"
          >
            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5">
              <span className="flex min-w-0 max-w-full items-center gap-2">
                <RepoDot repo={f.entry.repo} />
                <span className="truncate font-mono font-semibold" title={f.entry.name}>
                  {f.entry.name}
                </span>
              </span>
              <span className="min-w-0 max-w-full truncate font-mono text-xs text-muted-foreground max-sm:order-last max-sm:basis-full" title={f.entry.location}>
                {f.entry.location}
              </span>
              {f.narrated && <Badge variant="brand">Walkthrough</Badge>}
              <span className="nums ml-auto text-xs text-muted-foreground">{f.steps} steps</span>
            </div>
            <Journey repos={f.repos} />
            {f.contracts.length > 0 && (
              <div className="flex min-w-0 flex-wrap gap-1">
                {f.contracts.map((c) => (
                  <span key={c} className="max-w-full rounded-sm bg-accent px-1.5 py-0.5 font-mono text-xs text-accent-foreground [overflow-wrap:anywhere]">
                    <ContractName name={c} />
                  </span>
                ))}
              </div>
            )}
            {f.summary && <p className="line-clamp-2 max-w-[80ch] text-muted-foreground">{f.summary.replace(/`/g, "")}</p>}
          </Link>
        ))}
      </Card>
    </div>
  );
}
