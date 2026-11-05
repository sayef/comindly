import { useQuery } from "@tanstack/react-query";
import { Inbox, ListTree, Plus, Search } from "lucide-react";
import { useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { EntryCard } from "@/components/entry-card";
import { RememberDialog } from "@/components/remember-dialog";
import { PageHeader } from "@/components/page-header";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Toggle, ToggleGroup } from "@/components/ui/tabs";
import { api, attention, when } from "@/lib/api";
import { useTeams } from "@/lib/teams";

export function LedgerPage() {
  const teams = useTeams();
  const ledger = useQuery({ queryKey: ["ledger"], queryFn: api.ledger, select: (d) => d.filter(teams.entryInView) });
  const overview = useQuery({ queryKey: ["overview"], queryFn: api.overview });
  const [params, setParams] = useSearchParams();
  const pendingCount = (ledger.data ?? []).filter((e) => attention(e) < 3).length;
  const view: "review" | "timeline" = (params.get("view") as "review" | "timeline" | null) ?? (ledger.data && pendingCount === 0 ? "timeline" : "review");
  const setView = (v: "review" | "timeline") => setParams((p) => (p.set("view", v), p), { replace: true });
  const q = params.get("q") ?? "";
  const setQ = (v: string) => setParams((p) => (v ? p.set("q", v) : p.delete("q"), p), { replace: true });
  const [adding, setAdding] = useState(false);
  const text = useMemo(() => new Map((ledger.data ?? []).map((e) => [e.record.id, JSON.stringify(e.record).toLowerCase()])), [ledger.data]);
  const rows = useMemo(() => {
    const needle = q.toLowerCase();
    const all = (ledger.data ?? []).filter((e) => !needle || text.get(e.record.id)?.includes(needle));
    return view === "timeline"
      ? all.slice().sort((a, b) => when(b).localeCompare(when(a)))
      : all.filter((e) => attention(e) < 3).sort((a, b) => attention(a) - attention(b) || b.record.created.localeCompare(a.record.created));
  }, [ledger.data, text, view, q]);
  const pending = (ledger.data ?? []).filter((e) => attention(e) < 3).length;

  let month = "";
  return (
    <div className="flex flex-col gap-5">
      <PageHeader
        title="Ledger"
        actions={
          overview.data?.writable && (
            <Button variant="brand" onClick={() => setAdding(true)}>
              <Plus /> Add record
            </Button>
          )
        }
      >
        What the code does not say: failed attempts, decisions and constraints, anchored to the symbols they are about.
      </PageHeader>
      <RememberDialog open={adding} onOpenChange={setAdding} />
      <div className="flex flex-wrap items-center gap-3">
        <ToggleGroup label="View" value={view} onValueChange={(v) => setView(v as "review" | "timeline")}>
          <Toggle value="review">
            <Inbox className="size-3.5" /> Needs review {pending > 0 && <span className="nums text-xs font-normal text-muted-foreground">{pending}</span>}
          </Toggle>
          <Toggle value="timeline">
            <ListTree className="size-3.5" /> Timeline
          </Toggle>
        </ToggleGroup>
        <div className="relative w-full sm:ml-auto sm:max-w-xs">
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Filter records" aria-label="Filter records" className="pl-8" />
        </div>
      </div>
      {ledger.isLoading && [0, 1, 2].map((i) => <Skeleton key={i} className="h-32 rounded-lg" />)}
      {ledger.data && rows.length === 0 && (
        <div className="rounded-lg border border-dashed p-10 text-center text-sm text-muted-foreground">
          {ledger.data.length === 0 ? (
            <>
              No records yet. Use <span className="font-medium text-foreground">Add record</span>, or{" "}
              <code className="font-mono">comindly remember</code>; agents add them through MCP.
            </>
          ) : (
            view === "review" && !q ? (
              <>
                Nothing needs review. Records that go stale, lose their symbol or get disputed land here; the{" "}
                <button className="font-semibold text-brand hover:underline" onClick={() => setView("timeline")}>
                  timeline
                </button>{" "}
                has all of them.
              </>
            ) : (
              "No record matches this filter."
            )
          )}
        </div>
      )}
      <div className="grid gap-3">
        {rows.map((e) => {
          const m = when(e).slice(0, 7);
          const head = view === "timeline" && m !== month;
          month = m;
          return (
            <div key={e.record.id}>
              {head && (
                <h2 className="mb-2 mt-4 text-sm font-semibold text-muted-foreground">
                  {new Date(`${m}-01`).toLocaleDateString(undefined, { year: "numeric", month: "long" })}
                </h2>
              )}
              <EntryCard entry={e} writable={overview.data?.writable} heading={view === "timeline" ? "h3" : "h2"} />
            </div>
          );
        })}
      </div>
    </div>
  );
}
