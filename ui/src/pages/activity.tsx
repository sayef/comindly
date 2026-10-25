import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, Check, CircleSlash, Clock, Copy, Loader2, X } from "lucide-react";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { duration, IndexLog, shortStatus } from "@/components/index-log";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { api, type JobRow, type JobStatus } from "@/lib/api";
import { useAccess } from "@/lib/me";
import { ago, cn } from "@/lib/utils";

const KIND: Record<string, string> = { index: "Indexing" };
const kind = (k: string) => KIND[k] ?? k[0].toUpperCase() + k.slice(1);
/** "Today, 15:53", "Yesterday, 09:10" or "23 Oct, 18:02". */
function when(iso: string): string {
  const d = new Date(iso);
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  const day = (n: number) => new Date(Date.now() - n * 86_400_000).toDateString();
  if (d.toDateString() === day(0)) return `Today, ${time}`;
  if (d.toDateString() === day(1)) return `Yesterday, ${time}`;
  return `${d.toLocaleDateString([], { day: "numeric", month: "short" })}, ${time}`;
}

const active = (s: JobStatus) => s === "queued" || s === "running";

const STATUS: Record<JobStatus, { label: string; badge: "brand" | "warning" | "danger" | "outline" | "default" }> = {
  queued: { label: "Queued", badge: "outline" },
  running: { label: "Running", badge: "brand" },
  succeeded: { label: "Succeeded", badge: "brand" },
  failed: { label: "Failed", badge: "danger" },
  cancelled: { label: "Cancelled", badge: "default" },
  interrupted: { label: "Interrupted", badge: "warning" },
};

function StatusIcon({ status, className }: { status: JobStatus; className?: string }) {
  const c = cn("size-4 shrink-0", className);
  if (status === "running") return <Loader2 className={cn(c, "animate-spin text-brand")} />;
  if (status === "queued") return <Clock className={cn(c, "text-muted-foreground")} />;
  if (status === "succeeded") return <Check className={cn(c, "text-brand")} />;
  if (status === "failed") return <X className={cn(c, "text-danger")} />;
  if (status === "interrupted") return <AlertTriangle className={cn(c, "text-warning")} />;
  return <CircleSlash className={cn(c, "text-muted-foreground")} />;
}

function took(j: JobRow): string {
  if (!j.started) return "";
  const end = j.finished ? new Date(j.finished).getTime() : Date.now();
  return duration(end - new Date(j.started).getTime());
}

type Filter = "all" | "running" | "queued" | "succeeded" | "failed";
const failedLike = (s: JobStatus) => s === "failed" || s === "interrupted";
const matches = (f: Filter, s: JobStatus) => f === "all" || (f === "failed" ? failedLike(s) : s === f);

/** The list's sections: what is under way first, then one per day. */
function section(j: JobRow): string {
  if (active(j.status)) return "Now";
  const d = new Date(j.created).toDateString();
  if (d === new Date().toDateString()) return "Today";
  if (d === new Date(Date.now() - 86_400_000).toDateString()) return "Yesterday";
  return new Date(j.created).toLocaleDateString([], { weekday: "long", day: "numeric", month: "short" });
}

function Tile({
  label,
  value,
  note,
  icon,
  on,
  tone,
  onClick,
}: {
  label: string;
  value: number;
  note: string;
  icon: React.ReactNode;
  on: boolean;
  tone?: "brand" | "danger";
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      aria-pressed={on}
      className={cn(
        "flex min-w-0 flex-col gap-2 rounded-[10px] border bg-card px-4 py-3.5 text-left transition-colors hover:border-input",
        on && "border-brand/60 bg-accent/40 hover:border-brand/60",
      )}
    >
      <span className="flex items-center justify-between gap-2">
        <span className="eyebrow">{label}</span>
        {icon}
      </span>
      <span className={cn("nums text-2xl font-semibold tracking-[-0.03em]", value > 0 && tone === "danger" && "text-danger", value > 0 && tone === "brand" && "text-brand")}>{value}</span>
      <span className="truncate text-xs text-muted-foreground">{note}</span>
    </button>
  );
}

/** Background work, waiting, running or done, each run with its log. */
export function ActivityPage() {
  const { id } = useParams();
  const { me } = useAccess();
  const who = (j: JobRow) => j.started_by ?? (me?.mode === "server" ? "automatic" : "this machine");
  const navigate = useNavigate();
  const [filter, setFilter] = useState<Filter>("all");
  const jobs = useQuery({
    queryKey: ["jobs"],
    queryFn: api.jobs,
    refetchInterval: (q) => (q.state.data?.some((j) => active(j.status)) ? 1500 : 15_000),
  });
  const current = useQuery({ queryKey: ["index-job"], queryFn: api.indexJob });
  const all = jobs.data ?? [];
  const shown = all.filter((j) => matches(filter, j.status));
  const selected = id ?? (all.find((j) => j.status === "running") ?? all[0])?.id;
  const count = (f: Filter) => all.filter((j) => matches(f, j.status)).length;
  const lastOk = all.find((j) => j.status === "succeeded");
  const lastBad = all.find((j) => failedLike(j.status));
  const toggle = (f: Filter) => setFilter((was) => (was === f ? "all" : f));
  const sections: [string, JobRow[]][] = [];
  for (const j of shown) {
    const name = section(j);
    const last = sections[sections.length - 1];
    if (last?.[0] === name) last[1].push(j);
    else sections.push([name, [j]]);
  }

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h1 className="text-[28px] font-semibold tracking-[-0.035em]">Activity</h1>
        <p className="mt-1.5 text-sm text-muted-foreground">
          What comindly works on in the background: what waits in the queue, what runs now and how each run went, step by step.
        </p>
      </div>

      {jobs.isLoading ? (
        <Skeleton className="h-96" />
      ) : all.length === 0 ? (
        <Card className="flex flex-col items-center gap-3 px-6 py-14 text-center">
          <Clock className="size-6 text-muted-foreground" />
          <p className="text-sm font-medium">No activity yet</p>
          <p className="max-w-sm text-sm text-muted-foreground">
            Each indexing run shows up here with its steps, warnings and cost. Start one from{" "}
            <Link to="/settings/repositories" className="underline underline-offset-2">
              Settings → Repositories
            </Link>
            .
          </p>
        </Card>
      ) : (
        <>
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Tile
              label="Running"
              value={count("running")}
              note={current.data?.running ? shortStatus(current.data.events) || "Starting" : "Nothing running"}
              icon={<Loader2 className={cn("size-4 text-muted-foreground", count("running") > 0 && "animate-spin text-brand")} />}
              tone="brand"
              on={filter === "running"}
              onClick={() => toggle("running")}
            />
            <Tile
              label="Queued"
              value={count("queued")}
              note={count("queued") ? "Starts when the running one ends" : "Queue empty"}
              icon={<Clock className="size-4 text-muted-foreground" />}
              on={filter === "queued"}
              onClick={() => toggle("queued")}
            />
            <Tile
              label="Succeeded"
              value={count("succeeded")}
              note={lastOk ? `Last ${ago(lastOk.finished ?? lastOk.created)}` : "None yet"}
              icon={<Check className="size-4 text-brand" />}
              on={filter === "succeeded"}
              onClick={() => toggle("succeeded")}
            />
            <Tile
              label="Failed"
              value={count("failed")}
              note={lastBad ? `Last ${ago(lastBad.finished ?? lastBad.created)}` : "None kept"}
              icon={<X className={cn("size-4 text-muted-foreground", lastBad && "text-danger")} />}
              tone="danger"
              on={filter === "failed"}
              onClick={() => toggle("failed")}
            />
          </div>

          <div className="grid min-w-0 gap-6 lg:grid-cols-[minmax(0,340px)_minmax(0,1fr)]">
            <Card className="h-fit overflow-hidden">
              <div className="max-h-[70vh] overflow-y-auto">
                {sections.map(([name, rows]) => (
                  <section key={name}>
                    <h3 className="eyebrow sticky top-0 z-10 flex items-center justify-between border-b bg-card/95 px-4 py-2 backdrop-blur">
                      {name}
                      <span className="nums">{rows.length}</span>
                    </h3>
                    <ul>
                      {rows.map((j) => (
                        <li key={j.id}>
                          <button
                            onClick={() => navigate(`/activity/${j.id}`)}
                            className={cn(
                              "flex w-full items-start gap-3 border-b px-4 py-3 text-left transition-colors hover:bg-muted/50",
                              j.id === selected && "bg-accent/50 hover:bg-accent/60",
                            )}
                          >
                            <StatusIcon status={j.status} className="mt-0.5" />
                            <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                              <span className="flex items-center gap-2 text-[13.5px] font-medium">
                                {kind(j.kind)}
                                {j.status === "queued" && j.position != null && <span className="font-mono text-[10.5px] text-muted-foreground">#{j.position} in queue</span>}
                              </span>
                              <span className="truncate text-xs text-muted-foreground">
                                {new Date(j.created).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })} · {who(j)}
                              </span>
                            </span>
                            <span className="nums shrink-0 pt-0.5 font-mono text-[11px] text-muted-foreground">{took(j)}</span>
                          </button>
                        </li>
                      ))}
                    </ul>
                  </section>
                ))}
                {shown.length === 0 && (
                  <p className="px-4 py-6 text-center text-sm text-muted-foreground">
                    Nothing here.{" "}
                    <button className="underline underline-offset-2" onClick={() => setFilter("all")}>
                      Show everything
                    </button>
                  </p>
                )}
              </div>
              <p className="border-t px-4 py-2 text-[11px] text-muted-foreground">The last 50 runs are kept.</p>
            </Card>
            {selected && <Detail id={selected} who={who} />}
          </div>
        </>
      )}
    </div>
  );
}

function Detail({ id, who }: { id: string; who: (j: JobRow) => string }) {
  const qc = useQueryClient();
  const { admin } = useAccess();
  const job = useQuery({
    queryKey: ["job", id],
    queryFn: () => api.job(id),
    refetchInterval: (q) => (q.state.data && active(q.state.data.job.status) ? 1000 : false),
  });
  const cancel = useMutation({
    mutationFn: () => api.cancelJob(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["job", id] });
      qc.invalidateQueries({ queryKey: ["jobs"] });
      qc.invalidateQueries({ queryKey: ["index-job"] });
    },
  });
  if (job.isLoading) return <Skeleton className="h-96" />;
  if (!job.data) return <Card className="p-6 text-sm text-muted-foreground">This job is no longer kept.</Card>;
  const { job: j, events, position } = job.data;

  return (
    <Card className="min-w-0">
      <div className="flex flex-wrap items-start justify-between gap-3 border-b px-5 py-4">
        <div className="flex min-w-0 flex-col gap-1.5">
          <div className="flex items-center gap-2.5">
            <h2 className="text-lg font-semibold tracking-[-0.02em]">{kind(j.kind)}</h2>
            <Badge variant={STATUS[j.status].badge}>{STATUS[j.status].label}</Badge>
            {j.status === "queued" && position != null && <span className="text-xs text-muted-foreground">position {position}</span>}
          </div>
          <dl className="flex flex-wrap gap-x-5 gap-y-1 text-xs text-muted-foreground">
            <Meta label="By">{who(j)}</Meta>
            <Meta label={j.started ? "Started" : "Queued at"}>{when(j.started ?? j.created)}</Meta>
            {j.finished && <Meta label="Took">{took(j)}</Meta>}
            {j.request_id && (
              <Meta label="Request">
                <button className="flex items-center gap-1 font-mono hover:text-foreground" title="Copy the request id, to find this run in the server's logs" onClick={() => navigator.clipboard?.writeText(j.request_id!)}>
                  {j.request_id.slice(0, 12)}
                  <Copy className="size-3" />
                </button>
              </Meta>
            )}
          </dl>
        </div>
        {admin && active(j.status) && (
          <Button variant="outline" size="sm" loading={cancel.isPending} onClick={() => cancel.mutate()}>
            {j.status === "queued" ? "Remove from queue" : "Cancel"}
          </Button>
        )}
      </div>
      <div className="flex flex-col gap-4 p-5">
        {j.error && (
          <div className="flex items-start gap-2.5 rounded-lg border border-danger/30 bg-danger/8 px-3.5 py-2.5 text-sm text-danger">
            <X className="mt-0.5 size-4 shrink-0" />
            <span className="[overflow-wrap:anywhere]">{j.error}</span>
          </div>
        )}
        {j.status === "queued" ? (
          <p className="text-sm text-muted-foreground">Waiting for the running job to end; its steps show here once it starts.</p>
        ) : events.length ? (
          <IndexLog events={events} running={j.status === "running"} status={j.status} started={j.started} finished={j.finished} />
        ) : (
          <p className="text-sm text-muted-foreground">This run left no log.</p>
        )}
      </div>
    </Card>
  );
}

function Meta({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-1.5">
      <dt className="eyebrow">{label}</dt>
      <dd className="text-foreground/80">{children}</dd>
    </div>
  );
}
