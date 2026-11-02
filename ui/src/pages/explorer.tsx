import { useQuery } from "@tanstack/react-query";
import { ArrowDownRight, ArrowLeftRight, ArrowUpRight, Check, ClipboardCopy, Layers, Network, Plus, Route } from "lucide-react";
import { useMemo, useState } from "react";
import { withCode } from "@/components/code-text";
import { Navigate, useNavigate, useSearchParams } from "react-router";
import { EntryCard } from "@/components/entry-card";
import { GraphView } from "@/components/graph-view";
import { RememberDialog } from "@/components/remember-dialog";
import { RepoDot, SymbolLink } from "@/components/symbol";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { api, type Node } from "@/lib/api";
import { toast } from "sonner";
import { sentence } from "@/lib/utils";

function NodeList({ title, icon: Icon, nodes }: { title: string; icon: typeof Layers; nodes: Node[] }) {
  if (nodes.length === 0) return null;
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="flex items-center gap-2">
          <Icon className="size-4" /> {title}
          <span className="nums text-xs font-normal text-muted-foreground">{nodes.length}</span>
        </CardTitle>
      </CardHeader>
      <CardContent className="flex max-h-72 flex-col gap-1.5 overflow-y-auto">
        {nodes.map((n) => (
          <SymbolLink key={n.id} node={n} />
        ))}
      </CardContent>
    </Card>
  );
}

/** Copies the context pack (what to read before changing this) as markdown for an agent. */
function CopyContext({ focus }: { focus: string }) {
  const [done, setDone] = useState(false);
  const [busy, setBusy] = useState(false);
  return (
    <Button
      size="sm"
      variant="outline"
      title="What to read before changing this, plus what the team recorded, as markdown"
      loading={busy}
      onClick={async () => {
        setBusy(true);
        try {
          await navigator.clipboard?.writeText(await api.context(focus));
          setDone(true);
          toast.success("Context copied; paste it into your agent");
          setTimeout(() => setDone(false), 1500);
        } catch (e) {
          toast.error(sentence((e as Error).message));
        } finally {
          setBusy(false);
        }
      }}
    >
      {!busy && (done ? <Check /> : <ClipboardCopy />)} {done ? "Copied" : "Copy context for an agent"}
    </Button>
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
  if (detail.isLoading) return <Skeleton className="mt-6 h-[600px] rounded-lg" />;
  if (detail.isError || !detail.data?.zoom.focus)
    return (
      <p className="pt-6 text-muted-foreground">
        This symbol is not in the current index; it may have been renamed or removed. Search for it with ⌘&nbsp;K.
      </p>
    );

  const { zoom, ripple, ledger } = detail.data;
  const f = zoom.focus!;
  const reached = Object.entries(ripple.repos);
  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-3 pt-6">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            <RepoDot repo={f.repo} /> {f.repo === "_contracts" ? "Service contract" : f.repo}
            {f.repo !== "_contracts" && <span className="font-mono [overflow-wrap:anywhere]">{f.location}</span>}
            <Badge variant="outline">{f.kind}</Badge>
          </div>
          <h1 className="mt-1.5 font-mono text-[26px] leading-tight font-semibold tracking-[-0.02em] [overflow-wrap:anywhere]">{f.name}</h1>
          <p className="mt-2 max-w-[68ch] text-[17px] leading-normal text-muted-foreground">
            {ripple.total === 0
              ? "Nothing else in the index depends on it, so a change stays here."
              : `A change here reaches ${ripple.total.toLocaleString()} ${ripple.total === 1 ? "dependent" : "dependents"} in ${reached.length} ${reached.length === 1 ? "repository" : "repositories"}.`}
          </p>
          {reached.length > 0 && (
            <div className="mt-2 flex flex-wrap gap-1.5">
              {reached.map(([r, n]) => (
                <span key={r} className="nums flex items-center gap-1.5 rounded-sm bg-card px-2 py-1 text-xs">
                  <RepoDot repo={r} /> {r === "_contracts" ? "contracts" : r}
                  <span className="text-muted-foreground">{n}</span>
                </span>
              ))}
            </div>
          )}
          {f.signature && <pre className="mt-3 max-w-3xl overflow-x-auto rounded-md bg-card px-3 py-2 font-mono text-xs">{f.signature}</pre>}
          {detail.data.summary && <p className="mt-3 max-w-[68ch] text-muted-foreground">{withCode(detail.data.summary)}</p>}
          {f.repo !== "_contracts" && (
            <div className="mt-3 flex flex-wrap gap-2">
              <CopyContext focus={f.id} />
              {overview.data?.writable && (
                <Button size="sm" variant="outline" onClick={() => setAdding(true)}>
                  <Plus /> Add to the ledger
                </Button>
              )}
            </div>
          )}
          <RememberDialog open={adding} onOpenChange={setAdding} about={[f]} />
        </div>
      </div>

      <Card className="overflow-hidden">
        {graph && (
          <GraphView
            nodes={graph.nodes}
            edges={graph.edges}
            focus={detail.data.graph.focus}
            onPick={(id) => id !== f.id && navigate(`/explorer?focus=${encodeURIComponent(id)}`)}
            className="h-[460px] w-full"
          />
        )}
      </Card>

      {detail.data.flow && (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="flex items-center gap-2">
              <Route className="size-4" /> How this flow works
            </CardTitle>
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
            <CardTitle className="flex items-center gap-2">
              <Network className="size-4" /> Contracts
            </CardTitle>
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
          <h2 className="text-[17px] font-semibold">Recorded reasoning</h2>
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
