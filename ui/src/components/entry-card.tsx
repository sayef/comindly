import { useMutation, useQueryClient } from "@tanstack/react-query";
import { AgentView } from "@/components/agent-view";
import { Archive, Ban, Check, Copy, ShieldAlert, ShieldCheck, Trash2 } from "lucide-react";
import { useState } from "react";
import { ConfirmButton } from "@/components/confirm";
import { RepoDot } from "@/components/symbol";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useAccess } from "@/lib/me";
import { api, attention, type Entry, isChallenged, type ReviewAction, when } from "@/lib/api";
import { cn, date, sentence } from "@/lib/utils";

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

export const trustLabel = (t: string) => sentence(TRUST[t]?.[0] ?? t);

export function TrustBadge({ trust }: { trust: string }) {
  const [label, hint] = TRUST[trust] ?? [trust, ""];
  const high = trust === "T3" || trust === "T4";
  return (
    <Badge variant={high ? "brand" : trust === "T0" ? "danger" : "outline"} title={hint}>
      {sentence(label)}
    </Badge>
  );
}

// Statuses that are the normal case and need no badge.
const ROUTINE = new Set(["active", "moved"]);

export function statusLabel(e: Entry) {
  if (e.retired === "outdated") return "archived";
  if (e.retired === "wrong") return "invalid";
  return e.status;
}

export function EntryCard({
  entry,
  writable,
  compact = false,
  heading: H = "h3",
}: {
  entry: Entry;
  writable?: boolean;
  compact?: boolean;
  heading?: "h2" | "h3";
}) {
  const r = entry.record;
  const qc = useQueryClient();
  const { admin } = useAccess();
  const [asking, setAsking] = useState<"archive" | "invalidate" | null>(null);
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
  const erase = useMutation({ mutationFn: () => api.erase(r.id), onSuccess: refresh });
  const retired = entry.status === "retracted" || entry.status === "superseded";
  const challenged = isChallenged(entry);
  const due = attention(entry) < 3;
  const fields = Object.entries(r.fields).filter(([k]) => k !== "at");
  const status = statusLabel(entry);
  const ghostFirst = !asking && !(due && !retired);
  const anchors = entry.vouched?.length ? entry.vouched : r.anchors;
  const alike = !retired ? entry.alike : undefined;
  const about = anchors.length > 0 && (
    <div className="flex items-baseline gap-2 max-sm:flex-col max-sm:gap-1">
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
            {/* The path gives way from its start; the file name and line stay. */}
            <span className="flex min-w-0 font-mono text-muted-foreground">
              <span className="truncate [direction:rtl]">{a.path}</span>
              <span className="shrink-0">:{a.line}</span>
            </span>
          </span>
        ))}
        <AgentView icon asks={anchors.map((a) => ({ label: a.name, tool: "why", args: { focus: a.symbol } }))} />
      </dd>
    </div>
  );

  return (
    <Card className={cn("p-4", retired && "opacity-70")}>
      <div className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
        <Badge variant="outline">{sentence(r.kind)}</Badge>
        {!ROUTINE.has(status) && <Badge variant={STATUS[entry.status] ?? "outline"}>{sentence(status)}</Badge>}
        {entry.trust !== "T3" && !entry.outside?.length && <TrustBadge trust={entry.trust} />}
        {entry.outside?.length ? (
          <Badge variant="outline" title="Written about another team's code; it counts less until one of that team's members confirms it">
            From {entry.outside.join(", ")}
          </Badge>
        ) : null}
        {alike && (
          <Badge variant="warning" title="Reads like an earlier record on the same code; a person keeps both or archives this one">
            Possible repeat
          </Badge>
        )}
        {challenged && (
          <Badge variant="warning" title="An agent proposed retiring this; a person decides">
            Challenged
          </Badge>
        )}
        {entry.verified === true && <ShieldCheck className="size-3.5 text-brand" aria-label="signature verified" />}
        {entry.verified === false && <ShieldAlert className="size-3.5 text-danger" aria-label="signature invalid" />}
        <span className="ml-auto">
          by <span className="font-semibold text-foreground">{entry.by}</span>, {date(when(entry))}
        </span>
      </div>
      <H className="mt-2.5 leading-snug font-semibold">{r.title}</H>
      {!compact && r.body && r.body !== r.title && <p className="mt-1.5 text-sm text-muted-foreground">{r.body}</p>}
      {!compact && (r.trigger || fields.length > 0 || r.evidence.length > 0 || anchors.length > 0 || alike) && (
        <dl className="mt-3 grid grid-cols-[minmax(0,1fr)] gap-1 text-xs">
          {alike && (
            <div className="flex gap-2">
              <dt className="w-24 shrink-0 text-muted-foreground">Reads like</dt>
              <dd className="min-w-0 [overflow-wrap:anywhere]">
                {alike.title} <span className="font-mono text-muted-foreground">{alike.id.slice(0, 8)}</span>
              </dd>
            </div>
          )}
          {r.trigger && (
            <div className="flex gap-2">
              <dt className="w-24 shrink-0 text-muted-foreground">Relevant when</dt>
              <dd>{r.trigger}</dd>
            </div>
          )}
          {fields.map(([k, v]) => (
            <div key={k} className="flex gap-2">
              <dt className="w-24 shrink-0 first-letter:uppercase text-muted-foreground">{k.replace(/_/g, " ")}</dt>
              <dd>{typeof v === "string" ? v : JSON.stringify(v)}</dd>
            </div>
          ))}
          {r.evidence.length > 0 && (
            <div className="flex gap-2">
              <dt className="w-24 shrink-0 text-muted-foreground">Evidence</dt>
              <dd className="font-mono break-all">{r.evidence.join(", ")}</dd>
            </div>
          )}
          {about}
        </dl>
      )}
      {compact && about && <dl className="mt-3 text-xs">{about}</dl>}
      {!compact &&
        r.snippets?.map((sn, i) => (
          <figure key={i} className="mt-3 min-w-0 text-xs">
            <figcaption className="mb-1 text-muted-foreground">
              <span className="font-mono">{sn.path}</span>
              {sn.commit && (
                <>
                  {" "}at <span className="font-mono">{sn.commit.slice(0, 8)}</span>
                </>
              )}
            </figcaption>
            <pre className="overflow-x-auto rounded-md bg-muted p-3 font-mono text-xs leading-relaxed">{sn.code}</pre>
          </figure>
        ))}
      {!compact && entry.history.length > 0 && (
        <ol className="mt-3 flex flex-col gap-1 border-l-2 pl-3 text-xs text-muted-foreground">
          {entry.history.map((h) => (
            <li key={h.id} className={cn(!h.counted && "italic")}>
              <span className="font-semibold text-foreground">{DONE[h.action] ?? h.action}</span> by {h.who}, {date(h.at)}
              {h.reason && <span>: {h.reason}</span>}
              {!h.counted && <span> (waiting for a person)</span>}
            </li>
          ))}
        </ol>
      )}
      {writable && !compact && (
        <div className={cn("mt-4 flex flex-wrap items-center gap-1.5", ghostFirst && "-ml-3", admin && !asking && "-mr-3")}>
          {asking ? (
            <>
              <Input
                autoFocus
                value={reason}
                onChange={(e) => setReason(e.target.value)}
                placeholder={PROMPT[asking][1]}
                aria-label={PROMPT[asking][1]}
                className="h-8 max-w-sm"
              />
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
                  {due && (
                    <Button size="sm" variant="outline" onClick={() => review.mutate({ action: "confirm" })} loading={review.isPending} title={alike ? "Still true, and says something the earlier record does not" : "Still true"}>
                      <Check /> {alike ? "Keep both" : "Confirm"}
                    </Button>
                  )}
                  {alike && (
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => review.mutate({ action: "archive", why: `Same as ${alike.id.slice(0, 8)}: ${alike.title}` })}
                      loading={review.isPending}
                      title="Archive this one; the earlier record stands"
                    >
                      <Copy /> Same as earlier
                    </Button>
                  )}
                  <Button size="sm" variant="ghost" onClick={() => setAsking("archive")} title="Was true, no longer applies; kept as history">
                    <Archive /> Archive
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => setAsking("invalidate")} title="Was never right; kept, marked invalid">
                    <Ban /> Invalidate
                  </Button>
                </>
              )}
              {admin && (
                <span className="ml-auto">
                  <ConfirmButton
                    size="sm"
                    variant="ghost"
                    className="hover:text-danger"
                    title="Remove permanently, with its review history"
                    question="Delete for everyone?"
                    confirmLabel="Delete"
                    loading={erase.isPending}
                    onConfirm={() => erase.mutate()}
                  >
                    <Trash2 /> Delete
                  </ConfirmButton>
                </span>
              )}
            </>
          )}
          {review.data && !review.data.ok && <span className="text-xs text-danger [overflow-wrap:anywhere]">{sentence(review.data.message)}</span>}
          {erase.data && !erase.data.ok && <span className="text-xs text-danger [overflow-wrap:anywhere]">{sentence(erase.data.message)}</span>}
        </div>
      )}
    </Card>
  );
}
