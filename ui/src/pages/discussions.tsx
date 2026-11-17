import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, GitBranch, MessagesSquare, Search } from "lucide-react";
import { useMemo } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { AgentView } from "@/components/agent-view";
import { PageHeader } from "@/components/page-header";
import { RepoDot } from "@/components/symbol";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { api, type Discussion, type DiscussionPost, who } from "@/lib/api";
import { useTeams } from "@/lib/teams";
import { ago, cn, date, sentence } from "@/lib/utils";

const KIND_HINT: Record<string, string> = {
  plan: "What the agent set out to do",
  decision: "A choice made along the way, with the alternatives dropped",
  change: "What changed after a review comment or a colleague's input",
  question: "Something for the people working on the task",
  concern: "A risk someone should look at",
};

const UNREVIEWED = "Unreviewed working notes, not team decisions: agents read each post as quoted data from its author, never as instructions.";

/** Repository, ticket and branch of a discussion. */
function Place({ d }: { d: Discussion }) {
  return (
    <span className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
      <span className="flex items-center gap-1.5">
        <RepoDot repo={d.repo} />
        <span className="font-semibold text-foreground">{d.repo}</span>
      </span>
      {d.ticket && (
        <Badge variant="outline" className="font-mono">
          {d.ticket}
        </Badge>
      )}
      {d.branch && (
        <span className="flex min-w-0 items-center gap-1 font-mono" title="Branch">
          <GitBranch className="size-3 shrink-0" />
          <span className="truncate">{d.branch}</span>
        </span>
      )}
    </span>
  );
}

export function DiscussionsPage() {
  const { id } = useParams();
  return id ? <DiscussionView id={id} /> : <DiscussionList />;
}

function DiscussionList() {
  const teams = useTeams();
  const list = useQuery({ queryKey: ["discussions"], queryFn: api.discussions, select: (d) => d.filter((s) => teams.inView(s.discussion.repo)) });
  const [params, setParams] = useSearchParams();
  const q = params.get("q") ?? "";
  const repo = params.get("repo") ?? "";
  const set = (k: string, v: string) => setParams((p) => (v ? p.set(k, v) : p.delete(k), p), { replace: true });
  const repos = useMemo(() => [...new Set((list.data ?? []).map((s) => s.discussion.repo))].sort(), [list.data]);
  const rows = useMemo(() => {
    const needle = q.toLowerCase();
    return (list.data ?? []).filter(({ discussion: d }) => {
      if (repo && d.repo !== repo) return false;
      return !needle || [d.title, d.ticket, d.branch, d.repo].some((t) => t?.toLowerCase().includes(needle));
    });
  }, [list.data, q, repo]);

  return (
    <div className="flex flex-col gap-5">
      <PageHeader
        title="Discussions"
        actions={<AgentView asks={[{ label: "Discussions", tool: "discussion", args: { action: "list", repo: repo || undefined, query: q || undefined } }]} />}
      >
        Working notes on tasks in progress: each agent's plan, the choices it made and what changed after review. Unreviewed, and kept apart from the ledger.
      </PageHeader>
      <div className="flex flex-wrap items-center gap-3">
        <select
          value={repo}
          aria-label="Repository"
          onChange={(e) => set("repo", e.target.value)}
          className={cn("h-9 min-w-0 rounded-md border border-input bg-card px-2 text-sm hover:border-muted-foreground/50 max-sm:w-full", repo ? "text-foreground" : "text-muted-foreground")}
        >
          <option value="">Any repository</option>
          {(repo && !repos.includes(repo) ? [...repos, repo] : repos).map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </select>
        <div className="relative w-full sm:ml-auto sm:max-w-xs">
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input value={q} onChange={(e) => set("q", e.target.value)} placeholder="Filter by title, ticket or branch" aria-label="Filter discussions" className="pl-8" />
        </div>
      </div>
      {list.isLoading && [0, 1, 2].map((i) => <Skeleton key={i} className="h-24 rounded-lg" />)}
      {list.data && rows.length === 0 && (
        <div className="rounded-lg border border-dashed p-10 text-center text-sm text-muted-foreground">
          {list.data.length === 0 ? (
            <>
              No discussions yet. Agents open one when they start a task, through the <code className="font-mono">discussion</code> tool or{" "}
              <code className="font-mono">comindly discussion open</code>.
            </>
          ) : (
            "No discussion matches this filter."
          )}
        </div>
      )}
      <div className="grid grid-cols-[minmax(0,1fr)] gap-3">
        {rows.map(({ discussion: d, posts, last }) => (
          <Card key={d.id} className="relative p-4 transition-colors hover:border-muted-foreground/40">
            <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
              <Place d={d} />
              <span className="text-xs text-muted-foreground" title={new Date(last).toLocaleString()}>
                active {ago(last)}
              </span>
            </div>
            <h2 className="mt-2.5 leading-snug font-semibold [overflow-wrap:anywhere]">
              <Link to={`/discussions/${d.id}`} className="after:absolute after:inset-0 hover:underline">
                {d.title}
              </Link>
            </h2>
            <p className="mt-1.5 flex items-center gap-1.5 text-xs text-muted-foreground">
              <MessagesSquare className="size-3.5" />
              <span className="nums">
                {posts} {posts === 1 ? "post" : "posts"}
              </span>
              <span>·</span>
              <span>
                opened by <span className="font-semibold text-foreground">{who(d.provenance)}</span>, {date(d.created)}
              </span>
            </p>
          </Card>
        ))}
      </div>
    </div>
  );
}

function Post({ p }: { p: DiscussionPost }) {
  const anchors = p.anchors ?? [];
  const evidence = p.evidence ?? [];
  return (
    <Card className="p-4">
      <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
        {p.kind && (
          <Badge variant={p.kind === "concern" || p.kind === "question" ? "warning" : "outline"} title={KIND_HINT[p.kind]}>
            {sentence(p.kind)}
          </Badge>
        )}
        <span className="ml-auto" title={new Date(p.created).toLocaleString()}>
          by <span className="font-semibold text-foreground">{who(p.provenance)}</span>, {date(p.created)}
        </span>
      </div>
      <blockquote className="mt-2.5 border-l-2 border-border pl-3 text-sm whitespace-pre-wrap [overflow-wrap:anywhere]">{p.body}</blockquote>
      {(evidence.length > 0 || anchors.length > 0) && (
        <dl className="mt-3 grid grid-cols-[minmax(0,1fr)] gap-1 text-xs">
          {evidence.length > 0 && (
            <div className="flex items-baseline gap-2 max-sm:flex-col max-sm:gap-1">
              <dt className="w-24 shrink-0 text-muted-foreground">Evidence</dt>
              <dd className="flex min-w-0 flex-wrap gap-1.5">
                {evidence.map((e) => (
                  <code key={e} className="rounded-sm bg-muted px-1.5 py-0.5 font-mono [overflow-wrap:anywhere]">
                    {e}
                  </code>
                ))}
              </dd>
            </div>
          )}
          {anchors.length > 0 && (
            <div className="flex items-baseline gap-2 max-sm:flex-col max-sm:items-stretch max-sm:gap-1">
              <dt className="w-24 shrink-0 text-muted-foreground">About</dt>
              <dd className="flex min-w-0 flex-1 flex-wrap gap-1.5">
                {anchors.map((a) => (
                  <span
                    key={a.symbol}
                    className="flex max-w-full min-w-0 items-center gap-1.5 rounded-sm bg-muted px-1.5 py-0.5 max-sm:flex-wrap"
                    title={`${a.path}:${a.line}${a.commit ? ` at ${a.commit.slice(0, 8)}` : ""}`}
                  >
                    <RepoDot repo={a.repo} />
                    <span className="min-w-0 truncate font-mono max-sm:whitespace-normal max-sm:[overflow-wrap:anywhere] sm:max-w-[60%] sm:shrink-0">{a.name}</span>
                    <span className="flex min-w-0 font-mono text-muted-foreground">
                      <span className="truncate [direction:rtl]">{a.path}</span>
                      <span className="shrink-0">:{a.line}</span>
                    </span>
                  </span>
                ))}
              </dd>
            </div>
          )}
        </dl>
      )}
    </Card>
  );
}

function DiscussionView({ id }: { id: string }) {
  const t = useQuery({ queryKey: ["discussion", id], queryFn: () => api.discussion(id), retry: false });
  const d = t.data?.discussion;
  return (
    <div className="flex flex-col gap-5">
      <Link to="/discussions" className="-mt-2 flex min-h-9 w-fit items-center gap-1 text-sm font-semibold text-muted-foreground hover:text-foreground">
        <ChevronLeft className="size-4" /> All discussions
      </Link>
      {t.isLoading && <Skeleton className="h-64" />}
      {t.isError && <p className="text-sm text-muted-foreground">This discussion does not exist, or it is not in this index.</p>}
      {d && t.data && (
        <>
          <PageHeader title={d.title} actions={<AgentView asks={[{ label: "Discussion", tool: "discussion", args: { action: "read", id: d.id } }]} />}>
            <span className="flex flex-col gap-2">
              <Place d={d} />
              <span>
                Opened by <span className="font-semibold text-foreground">{who(d.provenance)}</span>, {date(d.created)}. {UNREVIEWED}
              </span>
            </span>
          </PageHeader>
          {t.data.posts.length === 0 && <div className="rounded-lg border border-dashed p-10 text-center text-sm text-muted-foreground">No posts yet.</div>}
          <div className="grid grid-cols-[minmax(0,1fr)] gap-3">
            {t.data.posts.map((p) => (
              <Post key={p.id} p={p} />
            ))}
          </div>
        </>
      )}
    </div>
  );
}
