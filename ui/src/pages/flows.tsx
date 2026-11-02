import { useQuery } from "@tanstack/react-query";
import { ArrowRight, ChevronLeft, Search, Sparkles } from "lucide-react";
import { useMemo, useState } from "react";
import { withCode } from "@/components/code-text";
import { Link, useSearchParams } from "react-router";
import { RepoDot, SymbolLink } from "@/components/symbol";
import { PageHeader } from "@/components/page-header";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
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
          <span className="flex items-center gap-1.5 rounded-sm bg-muted px-2 py-0.5">
            <RepoDot repo={r} /> {r}
          </span>
        </span>
      ))}
    </div>
  );
}


/** The flow as a tree: each step under the step that led to it. */
function Path({ entry, steps }: { entry: Node; steps: FlowStep[] }) {
  const children = useMemo(() => {
    const m = new Map<string, FlowStep[]>();
    for (const s of steps) m.set(s.from, [...(m.get(s.from) ?? []), s]);
    return m;
  }, [steps]);
  const render = (id: string, repo: string, depth: number): React.ReactNode =>
    (children.get(id) ?? []).map((s) => {
      const contract = s.node.kind === "Contract";
      const crosses = !contract && s.node.repo !== repo;
      return (
        <li key={s.node.id} className="flex flex-col">
          <div className={cn("flex flex-wrap items-center gap-2 rounded-md px-2 py-1", contract && "bg-accent")}>
            <span className="w-20 shrink-0 text-right text-xs text-muted-foreground">{s.via}</span>
            {contract ? (
              <span className="font-mono text-xs text-accent-foreground">{s.node.name}</span>
            ) : (
              <SymbolLink node={s.node} />
            )}
            {crosses && <Badge variant="brand">Into {s.node.repo}</Badge>}
          </div>
          {depth < 12 && <ul className="ml-6 border-l pl-2">{render(s.node.id, contract ? repo : s.node.repo, depth + 1)}</ul>}
        </li>
      );
    });
  return (
    <div className="flex flex-col gap-1 text-sm">
      <div className="flex items-center gap-2 px-2 py-1">
        <span className="w-20 shrink-0 text-right text-xs text-muted-foreground">Start</span>
        <SymbolLink node={entry} />
      </div>
      <ul className="ml-6 border-l pl-2">{render(entry.id, entry.repo, 0)}</ul>
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
                <CardTitle className="flex items-center gap-2">
                  <Sparkles className="size-4 text-brand" /> Walkthrough
                </CardTitle>
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
              <CardTitle>Path</CardTitle>
              <CardDescription>
                Every step from the entry point, across HTTP routes, topics and queues into other repositories. Highlighted steps are
                contracts.
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
  const [params] = useSearchParams();
  const id = params.get("id");
  const flows = useQuery({ queryKey: ["flows"], queryFn: api.flows, enabled: !id });
  const [filter, setFilter] = useState("");
  const [across, setAcross] = useState(true);
  if (id) return <FlowView id={id} />;
  const rows = (flows.data ?? [])
    .filter((f) => !across || f.repos.length > 1)
    .filter((f) => {
      const q = filter.trim().toLowerCase();
      return !q || [f.entry.name, ...f.repos, ...f.contracts].some((x) => x.toLowerCase().includes(q));
    });
  const total = flows.data?.length ?? 0;

  return (
    <div className="flex flex-col gap-5">
      <PageHeader title="Flows">
        Where work starts and where it travels: calls, then across HTTP routes, topics and queues into the services that handle them.
        With LLM summaries on when indexing, flows also get a written walkthrough.
      </PageHeader>
      <div className="flex flex-wrap items-center gap-3">
        <div className="relative w-full max-w-sm">
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filter by name, repository or contract" className="pl-8" />
        </div>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={across} onChange={(e) => setAcross(e.target.checked)} />
          Only flows across repositories
        </label>
        <span className="nums text-xs text-muted-foreground">
          {rows.length} of {total}
        </span>
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
            className="flex flex-col gap-2 border-b px-5 py-3.5 last:border-0 hover:bg-muted/60"
          >
            <div className="flex flex-wrap items-center gap-2">
              <RepoDot repo={f.entry.repo} />
              <span className="font-mono font-semibold">{f.entry.name}</span>
              <span className="font-mono text-xs text-muted-foreground">{f.entry.location}</span>
              {f.narrated && <Badge variant="brand">Walkthrough</Badge>}
              <span className="nums ml-auto text-xs text-muted-foreground">{f.steps} steps</span>
            </div>
            <Journey repos={f.repos} />
            {f.contracts.length > 0 && (
              <div className="flex flex-wrap gap-1">
                {f.contracts.map((c) => (
                  <span key={c} className="rounded-sm bg-accent px-1.5 py-0.5 font-mono text-xs text-accent-foreground">
                    {c}
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
