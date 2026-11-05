import { useIsMutating, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Clock, Loader2, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link } from "react-router";
import { toast } from "sonner";
import { shortStatus, statusParts } from "@/components/index-log";
import { api } from "@/lib/api";
import { cn } from "@/lib/utils";

/** What is running right now, in the header's corner, with a way to stop it; and a toast when indexing ends. */
export function Activity() {
  const qc = useQueryClient();
  const job = useQuery({
    queryKey: ["index-job"],
    queryFn: api.indexJob,
    refetchInterval: (q) => (q.state.data?.running || q.state.data?.queued?.length ? 1000 : 10_000),
  });
  const busy = useIsMutating();
  const stopIndex = useMutation({
    mutationFn: api.cancelIndex,
    meta: { quiet: true },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["index-job"] }),
  });
  const dequeue = useMutation({
    mutationFn: api.cancelJob,
    meta: { quiet: true },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["index-job"] }),
  });

  const j = job.data;
  const next = j?.queued?.[0];
  const queued = next && (
    <Pill>
      <Link to={`/activity/${next.id}`} className="flex items-center gap-2" title="Indexing starts when the running job ends">
        <Clock className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="font-medium">Queued</span>
        {j.queued!.length > 1 && <span className="text-muted-foreground">+{j.queued!.length - 1}</span>}
      </Link>
      <Stop
        label="Cancel the queued indexing"
        question="Take it off the queue?"
        confirm="Remove"
        pending={dequeue.isPending}
        onClick={() => dequeue.mutate(next.id)}
      />
    </Pill>
  );
  const was = useRef<{ id?: string; running: boolean } | undefined>(undefined);
  useEffect(() => {
    if (!j) return;
    // A queued job took over from the one that ended: its index is loaded, so refresh the pages.
    if (was.current?.running && j.running && was.current.id !== j.id)
      qc.invalidateQueries({ predicate: (q) => q.queryKey[0] !== "index-job" });
    if (was.current?.running && !j.running) {
      if (j.cancelled) toast.info("Indexing cancelled");
      else if (j.ok) toast.success("Indexing finished; every page shows the new index");
      else
        toast.error("Indexing failed", {
          action: { label: "See the log", onClick: () => window.location.assign(j.id ? `/activity/${j.id}` : "/activity") },
        });
      qc.invalidateQueries({ predicate: (q) => q.queryKey[0] !== "index-job" });
    }
    was.current = { id: j.id, running: j.running };
  }, [j, qc]);

  const step = shortStatus(j?.events ?? []);
  const parts = statusParts(j?.events ?? []);
  if (j?.running)
    return (
      <div className="flex flex-col items-end gap-2">
        <Pill>
          <Link to={j.id ? `/activity/${j.id}` : "/activity"} className="flex min-w-0 items-center gap-2" title={step ? `Indexing: ${step}` : "Indexing"}>
            <Loader2 className="size-3.5 shrink-0 animate-spin text-brand" />
            <span className="shrink-0 font-medium">Indexing</span>
            <span className="min-w-0 truncate text-muted-foreground">{parts.label}</span>
            {parts.count && <span className="nums shrink-0 text-muted-foreground">{parts.count}</span>}
          </Link>
          <Stop
            label="Stop indexing"
            question="Stop indexing? The run so far is discarded."
            confirm="Stop"
            pending={stopIndex.isPending}
            onClick={() => stopIndex.mutate()}
          />
        </Pill>
        {queued}
      </div>
    );
  if (queued) return queued;
  if (busy > 0)
    return (
      <Pill>
        <Loader2 className="size-3.5 shrink-0 animate-spin text-muted-foreground" />
        <span className="text-muted-foreground">Working…</span>
      </Pill>
    );
  return null;
}

function Pill({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-10 max-w-[min(26rem,calc(100vw-2rem))] items-center gap-2.5 rounded-lg border bg-popover px-3.5 text-sm text-popover-foreground shadow-float">
      {children}
    </div>
  );
}

function Stop({ label, question, confirm, pending, onClick }: { label: string; question: string; confirm: string; pending: boolean; onClick: () => void }) {
  const [asking, setAsking] = useState(false);
  const yes = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (asking) yes.current?.focus();
  }, [asking]);
  if (asking)
    return (
      <span role="group" aria-label={question} className="flex shrink-0 items-center gap-1.5" onKeyDown={(e) => e.key === "Escape" && setAsking(false)}>
        <span className="whitespace-nowrap text-muted-foreground">{question}</span>
        <button className="rounded-sm px-1.5 py-0.5 font-medium hover:bg-muted" onClick={() => setAsking(false)}>
          Keep
        </button>
        <button ref={yes} className="rounded-sm px-1.5 py-0.5 font-semibold text-danger hover:bg-danger/10" onClick={() => (setAsking(false), onClick())}>
          {confirm}
        </button>
      </span>
    );
  return (
    <button
      className={cn(
        "-mr-1 flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-muted hover:text-foreground",
        pending && "opacity-50",
      )}
      disabled={pending}
      onClick={() => setAsking(true)}
      aria-label={label}
      title={label}
    >
      <X className="size-3.5" />
    </button>
  );
}
