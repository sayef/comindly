import { useIsMutating, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Clock, Loader2, X } from "lucide-react";
import { useEffect, useRef } from "react";
import { Link } from "react-router";
import { toast } from "sonner";
import { shortStatus } from "@/components/index-log";
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
      <Link to="/settings/repositories" className="flex items-center gap-2" title="Indexing starts when the running job ends">
        <Clock className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="font-medium">Queued</span>
        {j.queued!.length > 1 && <span className="text-muted-foreground">+{j.queued!.length - 1}</span>}
      </Link>
      <Stop label="Cancel the queued indexing" pending={dequeue.isPending} onClick={() => dequeue.mutate(next.id)} />
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
          action: { label: "See the log", onClick: () => window.location.assign("/settings/repositories") },
        });
      qc.invalidateQueries({ predicate: (q) => q.queryKey[0] !== "index-job" });
    }
    was.current = { id: j.id, running: j.running };
  }, [j, qc]);

  const step = shortStatus(j?.events ?? []);
  if (j?.running)
    return (
      <div className="flex items-center gap-2">
        <Pill>
          <Link to="/settings/repositories" className="flex min-w-0 items-center gap-2" title={step ? `Indexing: ${step}` : "Indexing"}>
            <Loader2 className="size-3.5 shrink-0 animate-spin text-brand" />
            <span className="font-medium">Indexing</span>
            <span className="hidden max-w-44 truncate text-muted-foreground 2xl:inline">{step}</span>
          </Link>
          <Stop label="Cancel indexing" pending={stopIndex.isPending} onClick={() => stopIndex.mutate()} />
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
  return <div className="flex h-8 max-w-sm items-center gap-2 rounded-full border bg-card px-3 text-xs shadow-sm">{children}</div>;
}

function Stop({ label, pending, onClick }: { label: string; pending: boolean; onClick: () => void }) {
  return (
    <button
      className={cn(
        "-mr-1 flex items-center gap-1 rounded-full px-1.5 py-0.5 text-muted-foreground hover:bg-muted hover:text-foreground",
        pending && "opacity-50",
      )}
      disabled={pending}
      onClick={onClick}
      aria-label={label}
      title={label}
    >
      <X className="size-3.5" />
    </button>
  );
}
