import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, X } from "lucide-react";
import { useEffect, useState } from "react";
import { RepoDot } from "@/components/symbol";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { api, type Node } from "@/lib/api";
import { cn, sentence } from "@/lib/utils";

const KINDS = [
  ["constraint", "Constraint", "A rule the code must keep"],
  ["decision", "Decision", "A choice between alternatives, and why"],
  ["attempt", "Failed attempt", "Something tried that did not work (needs evidence)"],
  ["gotcha", "Gotcha", "A trap that is easy to fall into"],
  ["intent", "Intent", "Why this code exists"],
  ["question", "Question", "Something nobody could answer yet"],
] as const;
type Kind = (typeof KINDS)[number][0];

const area = "min-h-20 w-full rounded-md border border-input bg-card px-3 py-2 text-sm transition-[border-color,box-shadow] duration-100 hover:border-muted-foreground/50 focus-visible:border-ring focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring max-sm:text-base";
const label = "flex flex-col gap-1.5 text-sm font-semibold";

/** Pick the symbols a record is about, by name. */
function SymbolPicker({ value, onChange }: { value: Node[]; onChange: (v: Node[]) => void }) {
  const [q, setQ] = useState("");
  const [debounced, setDebounced] = useState("");
  useEffect(() => {
    const t = setTimeout(() => setDebounced(q.trim()), 200);
    return () => clearTimeout(t);
  }, [q]);
  const found = useQuery({ queryKey: ["find", debounced], queryFn: () => api.find(debounced), enabled: debounced.length > 1 });
  const chosen = new Set(value.map((n) => n.id));
  return (
    <div className="flex flex-col gap-1.5">
      {value.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {value.map((n) => (
            <span key={n.id} className="flex items-center gap-1 rounded-sm bg-muted px-2 py-0.5 text-xs">
              <RepoDot repo={n.repo} /> {n.name}
              <button type="button" aria-label={`Remove ${n.name}`} onClick={() => onChange(value.filter((x) => x.id !== n.id))}>
                <X className="size-3" />
              </button>
            </span>
          ))}
        </div>
      )}
      <Input
        value={q}
        onChange={(e) => setQ(e.target.value)}
        onKeyDown={(e) => {
          if (e.key !== "Enter") return;
          e.preventDefault();
          const first = found.data?.find((n) => !chosen.has(n.id));
          if (first && debounced.length > 1) (onChange([...value, first]), setQ(""));
        }}
        aria-label="Search a symbol this record is about"
        autoComplete="off"
        spellCheck={false}
        placeholder="Search a function, class or file…"
      />
      {debounced.length > 1 && (found.data?.length ?? 0) > 0 && (
        <div className="max-h-40 overflow-y-auto overscroll-contain rounded-md bg-muted/50">
          {found.data!
            .filter((n) => !chosen.has(n.id))
            .slice(0, 8)
            .map((n) => (
              <button
                type="button"
                key={n.id}
                className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-muted"
                onClick={() => (onChange([...value, n]), setQ(""))}
              >
                <RepoDot repo={n.repo} />
                <span className="font-mono font-semibold">{n.name}</span>
                <span className="truncate font-mono text-muted-foreground">{n.location}</span>
              </button>
            ))}
        </div>
      )}
    </div>
  );
}

/** Write a ledger record as the person using the UI. */
export function RememberDialog({ open, onOpenChange, about = [] }: { open: boolean; onOpenChange: (o: boolean) => void; about?: Node[] }) {
  const qc = useQueryClient();
  const [kind, setKind] = useState<Kind>("constraint");
  const [on, setOn] = useState<Node[]>(about);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [trigger, setTrigger] = useState("");
  const [evidence, setEvidence] = useState("");
  const [extra, setExtra] = useState("");
  useEffect(() => {
    if (open) setOn(about);
  }, [open]); // eslint-disable-line react-hooks/exhaustive-deps
  const save = useMutation({
    meta: { inline: true },
    mutationFn: () =>
      api.remember({
        kind,
        on: on.map((n) => n.id),
        title,
        body,
        trigger,
        evidence: evidence.split(/[\n,]/).map((e) => e.trim()),
        fields: kind === "decision" ? { alternatives: extra } : kind === "attempt" ? { outcome: extra } : {},
      }),
    onSuccess: () => {
      for (const k of ["ledger", "overview", "node"]) qc.invalidateQueries({ queryKey: [k] });
      setTitle("");
      setBody("");
      setTrigger("");
      setEvidence("");
      setExtra("");
      onOpenChange(false);
    },
  });
  const needsEvidence = kind === "attempt";
  const ready = title.trim() && on.length > 0 && (!needsEvidence || evidence.trim());
  const written = [title, body, trigger, evidence, extra].some((v) => v.trim());
  const [leaving, setLeaving] = useState(false);
  useEffect(() => {
    if (!open) setLeaving(false);
  }, [open]);
  const discard = () => {
    for (const set of [setTitle, setBody, setTrigger, setEvidence, setExtra]) set("");
    onOpenChange(false);
  };
  const close = () => (written ? setLeaving(true) : onOpenChange(false));
  const hold = (e: Event) => {
    if (!written) return;
    e.preventDefault();
    setLeaving(true);
  };

  return (
    <Dialog open={open} onOpenChange={(o) => (o ? onOpenChange(o) : close())}>
      <DialogContent className="top-[8%] max-h-[84vh] overflow-y-auto p-5" onEscapeKeyDown={hold} onInteractOutside={hold}>
        <DialogTitle className="text-[17px] font-semibold">Add to the ledger</DialogTitle>
        <p className="mt-1 text-sm text-muted-foreground">
          A short fact a future engineer or agent should know before changing this code. It carries your name (or a pseudonymous id) and is shown to agents.
        </p>
        <form
          className="mt-4 flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            if (ready && !save.isPending) save.mutate();
          }}
        >
          <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-3">
            {KINDS.map(([k, name, hint]) => (
              <button
                type="button"
                key={k}
                title={hint}
                onClick={() => setKind(k)}
                aria-pressed={kind === k}
                className={cn(
                  "rounded-md px-2.5 py-2 text-left text-xs transition-colors duration-100",
                  kind === k ? "bg-accent text-accent-foreground outline-2 outline-brand" : "bg-muted hover:bg-[color-mix(in_srgb,var(--color-muted)_85%,var(--color-foreground))]",
                )}
              >
                <div className="text-sm font-semibold">{name}</div>
                <div className="mt-0.5 line-clamp-2 text-xs text-muted-foreground">{hint}</div>
              </button>
            ))}
          </div>
          <div className={label}>
            About
            <SymbolPicker value={on} onChange={setOn} />
          </div>
          <label className={label}>
            Title
            <Input value={title} maxLength={160} onChange={(e) => setTitle(e.target.value)} placeholder="One neutral, factual line…" />
          </label>
          <label className={label}>
            Details <span className="font-normal text-muted-foreground">optional</span>
            <textarea className={area} value={body} onChange={(e) => setBody(e.target.value)} />
          </label>
          {kind === "decision" && (
            <label className={label}>
              Alternatives considered
              <Input value={extra} onChange={(e) => setExtra(e.target.value)} placeholder="What was rejected, and why…" />
            </label>
          )}
          {kind === "attempt" && (
            <label className={label}>
              Outcome
              <Input value={extra} onChange={(e) => setExtra(e.target.value)} placeholder="Reverted, failed in staging, too slow…" />
            </label>
          )}
          <label className={label}>
            Relevant when <span className="font-normal text-muted-foreground">optional</span>
            <Input value={trigger} onChange={(e) => setTrigger(e.target.value)} placeholder="e.g. adding caching to auth paths…" />
          </label>
          <label className={label}>
            Evidence {!needsEvidence && <span className="font-normal text-muted-foreground">optional</span>}
            <Input
              autoComplete="off"
              spellCheck={false}
              value={evidence}
              onChange={(e) => setEvidence(e.target.value)}
              placeholder="commit:abc123, mr:!412, test:tests/test_x.py::test_y"
            />
          </label>
          {save.error && <p role="alert" className="text-xs text-danger [overflow-wrap:anywhere]">{sentence(save.error.message)}</p>}
          {leaving ? (
            <div role="group" aria-label="Discard this record?" className="flex flex-wrap items-center justify-end gap-2">
              <span className="text-sm text-muted-foreground">Discard what you wrote?</span>
              <Button type="button" variant="ghost" onClick={() => setLeaving(false)} autoFocus>
                Keep writing
              </Button>
              <Button type="button" variant="outline" className="text-danger hover:text-danger" onClick={discard}>
                Discard
              </Button>
            </div>
          ) : (
            <div className="flex justify-end gap-2">
              <Button type="button" variant="ghost" onClick={close}>
                Cancel
              </Button>
              <Button type="submit" variant="brand" disabled={!ready} loading={save.isPending}>
                <Plus /> Add
              </Button>
            </div>
          )}
        </form>
      </DialogContent>
    </Dialog>
  );
}
