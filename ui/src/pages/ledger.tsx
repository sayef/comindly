import { useQuery } from "@tanstack/react-query";
import { Inbox, ListTree, Plus, Search, Users } from "lucide-react";
import { useMemo, useState } from "react";
import { useSearchParams } from "react-router";
import { EntryCard, statusLabel, trustLabel } from "@/components/entry-card";
import { RememberDialog } from "@/components/remember-dialog";
import { PageHeader } from "@/components/page-header";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Toggle, ToggleGroup } from "@/components/ui/tabs";
import { api, attention, type Entry, when } from "@/lib/api";
import { cn, sentence } from "@/lib/utils";

type View = "review" | "timeline" | "outside";

const FILTERS = [
  { key: "status", label: "Status", any: "Any status", of: (e: Entry) => statusLabel(e).toLowerCase(), name: sentence },
  { key: "kind", label: "Kind", any: "Any kind", of: (e: Entry) => e.record.kind, name: sentence },
  { key: "trust", label: "Trust", any: "Any trust", of: (e: Entry) => e.trust, name: trustLabel },
];
import { useTeams } from "@/lib/teams";

export function LedgerPage() {
  const teams = useTeams();
  const ledger = useQuery({ queryKey: ["ledger"], queryFn: api.ledger, select: (d) => d.filter(teams.entryInView) });
  const overview = useQuery({ queryKey: ["overview"], queryFn: api.overview });
  const [params, setParams] = useSearchParams();
  const pendingCount = (ledger.data ?? []).filter((e) => attention(e) < 3).length;
  // Notes from other teams about code of the teams you belong to (any team's, if you belong to none).
  const ours = (e: Entry) => !teams.mine.length || e.record.anchors.some((a) => teams.teamsOf(a.repo).some((t) => teams.mine.includes(t)));
  const fromOthers = (ledger.data ?? []).filter((e) => e.outside?.length && ours(e) && !["superseded", "retracted"].includes(e.status.toLowerCase()));
  const view: View = (params.get("view") as View | null) ?? (ledger.data && pendingCount === 0 ? "timeline" : "review");
  const setView = (v: View) => setParams((p) => (p.set("view", v), p), { replace: true });
  const q = params.get("q") ?? "";
  const setQ = (v: string) => setParams((p) => (v ? p.set("q", v) : p.delete("q"), p), { replace: true });
  const [adding, setAdding] = useState(false);
  const picks = FILTERS.map((f) => [f, params.get(f.key) ?? ""] as const);
  const setPick = (key: string, v: string) => setParams((p) => (v ? p.set(key, v) : p.delete(key), p), { replace: true });
  const picked = (e: Entry) => picks.every(([f, v]) => !v || f.of(e) === v);
  const text = useMemo(() => new Map((ledger.data ?? []).map((e) => [e.record.id, JSON.stringify(e.record).toLowerCase()])), [ledger.data]);
  const rows = useMemo(() => {
    const needle = q.toLowerCase();
    const all = (ledger.data ?? []).filter((e) => picked(e) && (!needle || text.get(e.record.id)?.includes(needle)));
    if (view === "outside") return fromOthers.filter((e) => picked(e) && (!needle || text.get(e.record.id)?.includes(needle)));
    return view === "timeline"
      ? all.slice().sort((a, b) => when(b).localeCompare(when(a)))
      : all.filter((e) => attention(e) < 3).sort((a, b) => attention(a) - attention(b) || b.record.created.localeCompare(a.record.created));
  }, [ledger.data, text, view, q, fromOthers, params]);
  const pending = (ledger.data ?? []).filter((e) => attention(e) < 3).length;

  let month = "";
  return (
    <div className="flex flex-col gap-5">
      <PageHeader
        title="Ledger"
      >
        What the code does not say: failed attempts, decisions and constraints, anchored to the symbols they are about.
      </PageHeader>
      <RememberDialog open={adding} onOpenChange={setAdding} />
      <div className="flex flex-wrap items-center gap-3">
        <ToggleGroup label="View" value={view} onValueChange={(v) => setView(v as View)}>
          <Toggle value="review">
            <Inbox className="size-3.5" /> Needs review {pending > 0 && <span className="nums text-xs font-normal text-muted-foreground">{pending}</span>}
          </Toggle>
          <Toggle value="timeline">
            <ListTree className="size-3.5" /> Timeline
          </Toggle>
          {fromOthers.length > 0 && (
            <Toggle value="outside">
              <Users className="size-3.5" /> From other teams <span className="nums text-xs font-normal text-muted-foreground">{fromOthers.length}</span>
            </Toggle>
          )}
        </ToggleGroup>
        <div className="grid w-full grid-cols-3 gap-2 sm:flex sm:w-auto">
          {picks.map(([f, v]) => {
            const seen = new Map<string, number>();
            for (const e of ledger.data ?? []) seen.set(f.of(e), (seen.get(f.of(e)) ?? 0) + 1);
            if (v && !seen.has(v)) seen.set(v, 0);
            return (
              <select
                key={f.key}
                value={v}
                aria-label={f.label}
                onChange={(e) => setPick(f.key, e.target.value)}
                className={cn("h-9 min-w-0 rounded-md border border-input bg-card px-2 text-sm hover:border-muted-foreground/50", v ? "text-foreground" : "text-muted-foreground")}
              >
                <option value="">{f.any}</option>
                {[...seen].sort((a, b) => b[1] - a[1]).map(([k]) => (
                  <option key={k} value={k}>
                    {f.name(k)}
                  </option>
                ))}
              </select>
            );
          })}
        </div>
        <div className="relative w-full sm:ml-auto sm:max-w-xs">
          <Search className="absolute left-2.5 top-2.5 size-4 text-muted-foreground" />
          <Input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Filter records" aria-label="Filter records" className="pl-8" />
        </div>
        {overview.data?.writable && (
          <Button variant="brand" onClick={() => setAdding(true)} className="max-sm:w-full">
            <Plus /> Add record
          </Button>
        )}
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
            view === "outside" && !q ? (
              "No notes from other teams are waiting for your teams."
            ) : view === "review" && !q ? (
              <>
                Nothing needs review. Records that lose their symbol, get disputed or repeat another land here; the{" "}
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
      <div className="grid grid-cols-[minmax(0,1fr)] gap-3">
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
