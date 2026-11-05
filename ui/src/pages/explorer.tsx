import { useQuery } from "@tanstack/react-query";
import { ArrowDownRight, ArrowLeftRight, ArrowUpRight, Layers, Network, Plus, Radar, Route } from "lucide-react";
import { useMemo, useState } from "react";
import { withCode } from "@/components/code-text";
import { Navigate, useNavigate, useSearchParams } from "react-router";
import { EntryCard } from "@/components/entry-card";
import { GraphView } from "@/components/graph-view";
import { AgentView } from "@/components/agent-view";
import { RememberDialog } from "@/components/remember-dialog";
import { RepoDot, SymbolLink } from "@/components/symbol";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { api, type Node } from "@/lib/api";
import { sentence } from "@/lib/utils";

function NodeList({ title, icon: Icon, nodes }: { title: string; icon: typeof Layers; nodes: Node[] }) {
  if (nodes.length === 0) return null;
  return (
    <Card>
      <CardHeader className="pb-2">
        <h2 className="flex items-center gap-2 text-sm font-semibold">
          <Icon className="size-4" /> {title}
          <span className="nums text-xs font-normal text-muted-foreground">{nodes.length}</span>
        </h2>
      </CardHeader>
      <CardContent className="flex max-h-72 flex-col gap-1.5 overflow-y-auto">
        {nodes.map((n) => (
          <SymbolLink key={n.id} node={n} />
        ))}
      </CardContent>
    </Card>
  );
}

export function ExplorerPage() {
  const [params] = useSearchParams();
  const navigate = useNavigate();
  const focus = params.get("focus");
  const detail = useQuery({ queryKey: ["node", focus], queryFn: () => api.node(focus!), enabled: !!focus });
  const overview = useQuery({ queryKey: ["overview"], queryFn: api.overview });
  const [adding, setAdding] = useState(false);
  const graph = useMemo(
    () =>
      detail.data
        ? {
            nodes: detail.data.graph.nodes.map((n) => ({ id: n.id, label: n.name, repo: n.repo })),
            edges: detail.data.graph.edges,
          }
        : null,
    [detail.data],
  );

  if (!focus) return <Navigate to="/" replace />;
  if (detail.isLoading) return <Skeleton className="h-[600px] rounded-lg" />;
  if (detail.isError || !detail.data?.zoom.focus)
    return (
      <p className="text-muted-foreground">
        This symbol is not in the current index; it may have been renamed or removed. Search for it with ⌘&nbsp;K.
      </p>
    );

  const { zoom, ripple, ledger } = detail.data;
  const f = zoom.focus!;
  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            <RepoDot repo={f.repo} /> {f.repo === "_contracts" ? "Service contract" : f.repo}
            {f.repo !== "_contracts" && <span className="font-mono [overflow-wrap:anywhere]">{f.location}</span>}
          </div>
          <h1 className="mt-1 flex flex-wrap items-center gap-2 text-[26px] font-semibold tracking-[-0.02em] [overflow-wrap:anywhere]">
            <span className="font-mono">{f.name}</span>
            <Badge variant="outline">{f.kind}</Badge>
          </h1>
          {f.signature && <pre className="mt-2 max-w-3xl overflow-x-auto rounded-md bg-muted px-3 py-2 font-mono text-xs">{f.signature}</pre>}
          {detail.data.summary && <p className="mt-2 max-w-3xl text-sm text-muted-foreground">{withCode(detail.data.summary)}</p>}
          {f.repo !== "_contracts" && (
            <div className="mt-3 flex flex-wrap gap-2">
              <AgentView
                asks={[
                  { label: "Before a change", tool: "context_pack", args: { focus: f.id } },
                  { label: "Neighbourhood", tool: "zoom", args: { focus: f.id } },
                  { label: "Blast radius", tool: "ripple", args: { focus: f.id } },
                  { label: "Records", tool: "why", args: { focus: f.id } },
                ]}
              />
              {overview.data?.writable && (
                <Button size="sm" variant="outline" onClick={() => setAdding(true)}>
                  <Plus /> Add to the ledger
                </Button>
              )}
            </div>
          )}
          <RememberDialog open={adding} onOpenChange={setAdding} about={[f]} />
        </div>
        <Card className="flex items-center gap-4 px-4 py-3">
          <Radar className="size-5 shrink-0 text-muted-foreground" />
          <div className="shrink-0" title="Blast radius: everything that depends on this">
            <div className="nums text-[26px] leading-tight font-semibold">{ripple.total}</div>
            <div className="text-xs whitespace-nowrap text-muted-foreground">Dependents</div>
          </div>
          <div className="flex max-w-xs flex-wrap gap-1">
            {Object.entries(ripple.repos).map(([r, n]) => (
              <Badge key={r} variant="outline">
                <RepoDot repo={r} /> {r === "_contracts" ? "contracts" : r} <span className="nums font-normal">{n}</span>
              </Badge>
            ))}
          </div>
        </Card>
      </div>

      <Card className="overflow-hidden">
        {graph && (
          <GraphView
            nodes={graph.nodes}
            edges={graph.edges}
            focus={detail.data.graph.focus}
            onPick={(id) => id !== f.id && navigate(`/explorer?focus=${encodeURIComponent(id)}`)}
            className="w-full"
          />
        )}
      </Card>

      {detail.data.flow && (
        <Card>
          <CardHeader className="pb-2">
            <h2 className="flex items-center gap-2 text-sm font-semibold">
              <Route className="size-4" /> How this flow works
            </h2>
          </CardHeader>
          <CardContent className="flex flex-col gap-2 text-sm">
            <p className="leading-relaxed">{withCode(detail.data.flow.narration)}</p>
            {detail.data.flow.questions.length > 0 && (
              <p className="text-xs text-muted-foreground">Answers: {detail.data.flow.questions.join("; ")}</p>
            )}
          </CardContent>
        </Card>
      )}

      {zoom.contracts.length > 0 && (
        <Card>
          <CardHeader className="pb-2">
            <h2 className="flex items-center gap-2 text-sm font-semibold">
              <Network className="size-4" /> Contracts
            </h2>
          </CardHeader>
          <CardContent className="flex flex-col gap-2">
            {zoom.contracts.map((c) => (
              <div key={c.contract.id + c.role} className="flex flex-wrap items-center gap-2 text-sm">
                <Badge variant="brand">{sentence(c.role)}</Badge>
                <SymbolLink node={c.contract} />
                <ArrowLeftRight className="size-3.5 text-muted-foreground" aria-label="with" />
                {c.counterparts.length ? (
                  c.counterparts.map((n) => <SymbolLink key={n.id} node={n} />)
                ) : (
                  <span className="text-xs text-muted-foreground">no counterpart in the index</span>
                )}
              </div>
            ))}
          </CardContent>
        </Card>
      )}

      {ledger.length > 0 && (
        <div className="flex flex-col gap-3">
          <h2 className="text-sm font-semibold">Recorded reasoning</h2>
          {ledger.map((e) => (
            <EntryCard key={e.record.id} entry={e} writable={overview.data?.writable} />
          ))}
        </div>
      )}

      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        <NodeList title="Callers" icon={ArrowDownRight} nodes={zoom.callers} />
        <NodeList title="Callees" icon={ArrowUpRight} nodes={zoom.callees} />
        <NodeList title="Imported by" icon={ArrowDownRight} nodes={zoom.importers} />
        <NodeList title="Providers" icon={Network} nodes={zoom.providers} />
        <NodeList title="Consumers" icon={Network} nodes={zoom.consumers} />
        <NodeList title="Members" icon={Layers} nodes={zoom.members} />
      </div>
    </div>
  );
}
