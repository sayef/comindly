import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Archive, Ban, Check, ShieldAlert, ShieldCheck, Trash2 } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useAccess } from "@/lib/me";
import { api, type Entry, isChallenged, type ReviewAction, when } from "@/lib/api";
import { cn, sentence } from "@/lib/utils";

const DONE: Record<string, string> = { confirm: "Confirmed", archive: "Archived", invalidate: "Invalidated", dispute: "Disputed", supersede: "Superseded", retract: "Retracted" };

const STATUS: Record<string, "brand" | "warning" | "danger" | "outline"> = {
  active: "brand",
  moved: "brand",
  stale: "warning",
  orphaned: "danger",
  superseded: "outline",
  retracted: "outline",
};

const TRUST: Record<string, [string, string]> = {
  T4: ["enforced", "A constraint whose check passes on the current code"],
  T3: ["confirmed", "Written or confirmed by a person"],
  T2: ["with evidence", "Written by an agent, citing evidence (a commit, test or MR)"],
  T1: ["unverified", "Written by an agent without evidence; a person should confirm it"],
  T0: ["untrusted", "Failed signature or unsafe content: agents never see it"],
};

const PROMPT: Record<"archive" | "invalidate", [string, string]> = {
  archive: ["Archive", "What changed? It was true, but no longer applies."],
  invalidate: ["Invalidate", "Why is it wrong?"],
};

export function TrustBadge({ trust }: { trust: string }) {
  const [label, hint] = TRUST[trust] ?? [trust, ""];
  const high = trust === "T3" || trust === "T4";
  return (
    <Badge variant={high ? "brand" : trust === "T0" ? "danger" : "outline"} title={hint}>
      {sentence(label)}
    </Badge>
  );
}

function statusLabel(e: Entry) {
  if (e.retired === "outdated") return "archived";
  if (e.retired === "wrong") return "invalid";
  return e.status;
}

export function EntryCard({ entry, writable, compact = false }: { entry: Entry; writable?: boolean; compact?: boolean }) {
  const r = entry.record;
  const qc = useQueryClient();
  const { admin } = useAccess();
  const [asking, setAsking] = useState<"archive" | "invalidate" | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [reason, setReason] = useState("");
  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["ledger"] });
    qc.invalidateQueries({ queryKey: ["overview"] });
    qc.invalidateQueries({ queryKey: ["node"] });
  };
  const review = useMutation({
    mutationFn: ({ action, why }: { action: ReviewAction; why?: string }) => api.review(r.id, action, why),
    onSuccess: () => {
      setAsking(null);
      setReason("");
      refresh();
    },
  });
  const erase = useMutation({ mutationFn: () => api.erase(r.id), onSuccess: () => (setDeleting(false), refresh()) });
  const retired = entry.status === "retracted" || entry.status === "superseded";
  const challenged = isChallenged(entry);
  const fields = Object.entries(r.fields).filter(([k]) => k !== "at");

  return (
    <Card className={cn("p-4", retired && "opacity-70")}>
      <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
        <Badge variant="outline" className="capitalize">
          {r.kind}
        </Badge>
        <Badge variant={STATUS[entry.status] ?? "outline"}>{sentence(statusLabel(entry))}</Badge>
        <TrustBadge trust={entry.trust} />
        {challenged && (
          <Badge variant="warning" title="An agent proposed retiring this; a person decides">
            challenged
          </Badge>
        )}
        {entry.verified === true && <ShieldCheck className="size-3.5 text-brand" aria-label="signature verified" />}
        {entry.verified === false && <ShieldAlert className="size-3.5 text-danger" aria-label="signature invalid" />}
        <span className="ml-auto">
          <span className="font-medium text-foreground">{entry.by}</span> · {when(entry).slice(0, 10)}
        </span>
      </div>
      <h4 className="mt-2.5 font-medium leading-snug">{r.title}</h4>
      {!compact && r.body && r.body !== r.title && <p className="mt-1.5 text-sm text-muted-foreground">{r.body}</p>}
      {!compact && (r.trigger || fields.length > 0 || r.evidence.length > 0) && (
        <dl className="mt-3 grid gap-1 text-xs">
          {r.trigger && (
            <div className="flex gap-2">
              <dt className="w-24 shrink-0 text-muted-foreground">Relevant when</dt>
              <dd>{r.trigger}</dd>
            </div>
          )}
          {fields.map(([k, v]) => (
            <div key={k} className="flex gap-2">
              <dt className="w-24 shrink-0 capitalize text-muted-foreground">{k}</dt>
              <dd className="font-mono">{typeof v === "string" ? v : JSON.stringify(v)}</dd>
            </div>
          ))}
          {r.evidence.length > 0 && (
            <div className="flex gap-2">
              <dt className="w-24 shrink-0 text-muted-foreground">Evidence</dt>
              <dd className="font-mono break-all">{r.evidence.join(", ")}</dd>
            </div>
          )}
        </dl>
      )}
      <div className="mt-3 flex flex-wrap gap-1.5">
        {r.anchors.map((a) => (
          <span key={a.symbol} className="max-w-full truncate rounded bg-muted px-1.5 py-0.5 font-mono text-[11px]" title={`${a.path}:${a.line}`}>
            {a.repo}:{a.name}
          </span>
        ))}
      </div>
      {!compact && entry.history.length > 0 && (
        <ol className="mt-3 flex flex-col gap-1 border-l pl-3 text-xs text-muted-foreground">
          {entry.history.map((h) => (
            <li key={h.id} className={cn(!h.counted && "italic")}>
              <span className="font-medium text-foreground">{DONE[h.action] ?? h.action}</span> by {h.who} · {h.at.slice(0, 10)}
              {h.reason && <span>: {h.reason}</span>}
              {!h.counted && <span> (waiting for a person)</span>}
            </li>
          ))}
        </ol>
      )}
      {writable && !compact && (
        <div className="mt-4 flex flex-wrap items-center gap-2">
          {asking ? (
            <>
              <Input autoFocus value={reason} onChange={(e) => setReason(e.target.value)} placeholder={PROMPT[asking][1]} className="h-8 max-w-sm text-xs" />
              <Button size="sm" variant="outline" disabled={!reason.trim()} loading={review.isPending} onClick={() => review.mutate({ action: asking, why: reason })}>
                {PROMPT[asking][0]}
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setAsking(null)}>
                Cancel
              </Button>
            </>
          ) : (
            <>
              {!retired && (
                <>
                  <Button size="sm" variant="brand" onClick={() => review.mutate({ action: "confirm" })} loading={review.isPending} title="Still true">
                    <Check /> Confirm
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => setAsking("archive")} title="Was true, no longer applies; kept as history">
                    <Archive /> Archive
                  </Button>
                  <Button size="sm" variant="outline" onClick={() => setAsking("invalidate")} title="Was never right; kept, marked invalid">
                    <Ban /> Invalidate
                  </Button>
                </>
              )}
              {admin && (
                <Button size="sm" variant="ghost" className="ml-auto text-danger hover:text-danger" onClick={() => setDeleting(true)} title="Remove permanently">
                  <Trash2 /> Delete
                </Button>
              )}
            </>
          )}
          {review.data && !review.data.ok && <span className="text-xs text-danger [overflow-wrap:anywhere]">{sentence(review.data.message)}</span>}
        </div>
      )}
      <Dialog open={deleting} onOpenChange={setDeleting}>
        <DialogContent className="p-5">
          <DialogTitle className="text-base font-semibold">Delete this record permanently?</DialogTitle>
          <p className="mt-2 text-sm text-muted-foreground">
            “{r.title}” and its review history are removed from the ledger for everyone. To keep it as history, archive or invalidate it instead.
          </p>
          {erase.data && !erase.data.ok && <p className="mt-2 text-xs text-danger [overflow-wrap:anywhere]">{sentence(erase.data.message)}</p>}
          <div className="mt-4 flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setDeleting(false)}>
              Cancel
            </Button>
            <Button className="bg-danger text-white hover:bg-danger/90" loading={erase.isPending} onClick={() => erase.mutate()}>
              <Trash2 /> Delete
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </Card>
  );
}
