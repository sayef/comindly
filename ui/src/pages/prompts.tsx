import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { PageHeader } from "@/components/page-header";
import { ConfirmButton } from "@/components/confirm";
import { Notice } from "@/components/notice";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { useUnsavedWarning } from "@/components/team-settings";
import { api, type PromptAction, type PromptTask, type PromptTaskName, type PromptTried } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Section, SettingsTabs } from "./settings";

const textarea =
  "min-h-48 w-full rounded-md border border-input bg-card px-3 py-2 font-mono text-[13px] font-normal leading-relaxed outline-none transition-[border-color,box-shadow] duration-100 hover:border-muted-foreground/50 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/20 max-sm:text-base";
const pre = "whitespace-pre-wrap rounded-md bg-muted/50 px-3 py-2 font-mono text-[13px] font-normal leading-relaxed [overflow-wrap:anywhere]";

const about: Record<PromptTaskName, { title: string; writes: string; redo: string }> = {
  summary: {
    title: "Summaries",
    writes: "A sentence and search questions for each of the most used functions and types.",
    redo: "every summary",
  },
  guide: {
    title: "Style guides",
    writes: "Each section of a repository's style guide, from evidence measured in its code.",
    redo: "every style guide",
  },
  flow: {
    title: "Walkthroughs",
    writes: "A short walkthrough of each flow, from its entry point and call trace.",
    redo: "every walkthrough",
  },
};

function useChange(version: number | null) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (a: PromptAction) => api.changePrompt(a, version),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["prompts"] }),
  });
}

export function PromptsPage() {
  const prompts = useQuery({ queryKey: ["prompts"], queryFn: api.prompts });
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  const writable = me.data?.mode !== "server" || me.data?.role === "admin";

  return (
    <div className="flex max-w-4xl flex-col gap-5">
      <PageHeader title="Prompts">
        What indexing asks the LLM. A saved version is a draft until someone activates it; deactivating goes back to the built-in prompt. Either takes effect from the next index.
      </PageHeader>
      <SettingsTabs />
      {prompts.error && <Notice ok={false} message={prompts.error.message} />}
      {!prompts.data
        ? !prompts.error && <Skeleton className="h-96 rounded-lg" />
        : prompts.data.tasks.map((t) => <TaskCard key={t.task} t={t} version={prompts.data.version} writable={writable} />)}
    </div>
  );
}

function TaskCard({ t, version, writable }: { t: PromptTask; version: number | null; writable: boolean }) {
  const change = useChange(version);
  const [editing, setEditing] = useState(false);
  const [open, setOpen] = useState<number | null>(null);
  const a = about[t.task];
  const inUse = t.active === null ? "the built-in prompt" : `version ${t.active}`;
  const current = t.versions.find((v) => v.version === t.active)?.text ?? t.builtin;
  const busy = (x: PromptAction["action"], n?: number) =>
    change.isPending && change.variables?.action === x && (n === undefined || ("n" in change.variables && change.variables.n === n));
  const versions = [...t.versions].sort((x, y) => y.version - x.version);

  return (
    <Section title={a.title} description={`${a.writes} In use: ${inUse}.`}>
      <ul className="flex flex-col divide-y divide-border rounded-md border border-border">
        <Row label="Built-in" detail="Comes with comindly" active={t.active === null} open={open === 0} onToggle={() => setOpen(open === 0 ? null : 0)} text={t.builtin} />
        {versions.map((v) => (
          <Row
            key={v.version}
            label={`Version ${v.version}`}
            note={v.note}
            detail={`${v.author}, ${new Date(v.created_at).toLocaleString()}`}
            active={t.active === v.version}
            open={open === v.version}
            onToggle={() => setOpen(open === v.version ? null : v.version)}
            text={v.text}
            actions={
              writable &&
              (t.active === v.version ? (
                <ConfirmButton
                  size="sm"
                  variant="ghost"
                  danger={false}
                  question={`Back to the built-in prompt? The next index writes ${a.redo} again.`}
                  confirmLabel="Deactivate"
                  loading={busy("deactivate")}
                  onConfirm={() => change.mutate({ action: "deactivate", task: t.task })}
                >
                  Deactivate
                </ConfirmButton>
              ) : (
                <>
                  <ConfirmButton
                    size="sm"
                    variant="outline"
                    danger={false}
                    question={`The next index writes ${a.redo} again with this version.`}
                    confirmLabel="Activate"
                    loading={busy("activate", v.version)}
                    onConfirm={() =>
                      change.mutate({
                        action: "activate",
                        task: t.task,
                        n: v.version,
                      })
                    }
                  >
                    Activate
                  </ConfirmButton>
                  <ConfirmButton
                    size="sm"
                    variant="ghost"
                    question="Delete this version?"
                    confirmLabel="Delete"
                    loading={busy("delete", v.version)}
                    onConfirm={() =>
                      change.mutate({
                        action: "delete",
                        task: t.task,
                        n: v.version,
                      })
                    }
                  >
                    Delete
                  </ConfirmButton>
                </>
              ))
            }
          />
        ))}
      </ul>
      {change.error && <Notice ok={false} message={change.error.message} />}
      {writable &&
        (editing ? (
          <Editor t={t} start={current} version={version} onDone={() => setEditing(false)} />
        ) : (
          <div>
            <Button size="sm" variant="outline" onClick={() => setEditing(true)}>
              New version
            </Button>
          </div>
        ))}
    </Section>
  );
}

function Row(props: { label: string; note?: string; detail: string; active: boolean; open: boolean; onToggle: () => void; text: string; actions?: React.ReactNode }) {
  return (
    <li className="flex flex-col gap-2 px-3 py-2.5">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <button className="flex min-w-0 flex-1 flex-col items-start text-left" onClick={props.onToggle} aria-expanded={props.open}>
          <span className="flex items-center gap-2 text-sm font-semibold">
            {props.label}
            {props.active && <Badge variant="brand">In use</Badge>}
          </span>
          <span className="text-xs text-muted-foreground [overflow-wrap:anywhere]">
            {props.note ? `${props.note}. ` : ""}
            {props.detail}
          </span>
        </button>
        {props.actions && <div className="flex flex-wrap items-center gap-1.5">{props.actions}</div>}
      </div>
      {props.open && <div className={pre}>{props.text}</div>}
    </li>
  );
}

function Editor({ t, start, version, onDone }: { t: PromptTask; start: string; version: number | null; onDone: () => void }) {
  const [text, setText] = useState(start);
  const [note, setNote] = useState("");
  const [sample, setSample] = useState("");
  const save = useChange(version);
  const tryIt = useMutation({
    mutationFn: () => api.tryPrompt(t.task, text, sample.trim() || undefined),
    meta: { inline: true, quiet: true },
  });
  const dirty = text.trim() !== start.trim();
  useUnsavedWarning(dirty);
  const tryable = t.task !== "guide";

  return (
    <div className="flex flex-col gap-3 rounded-md border border-border p-3">
      <label className="flex flex-col gap-1.5 text-sm font-semibold">
        Instructions
        <textarea className={textarea} value={text} onChange={(e) => setText(e.target.value)} autoFocus />
      </label>
      <div className="flex flex-col gap-1.5">
        <span className="text-sm font-semibold">Always added</span>
        <span className="text-xs text-muted-foreground">The reply format comindly reads back; it cannot be changed.</span>
        <div className={cn(pre, "text-muted-foreground")}>{t.format}</div>
      </div>
      <label className="flex flex-col gap-1.5 text-sm font-semibold">
        Note
        <Input value={note} onChange={(e) => setNote(e.target.value)} placeholder="What changed, for whoever activates it" />
      </label>
      {tryable && (
        <div className="flex flex-wrap items-end gap-2">
          <label className="flex min-w-0 flex-1 flex-col gap-1.5 text-sm font-semibold">
            Try it on
            <Input
              value={sample}
              onChange={(e) => setSample(e.target.value)}
              placeholder={t.task === "summary" ? "A symbol id; empty for the most used one" : "An entry point's id; empty for the furthest-reaching flow"}
            />
          </label>
          <Button variant="outline" disabled={!text.trim()} loading={tryIt.isPending} onClick={() => tryIt.mutate()}>
            Try
          </Button>
        </div>
      )}
      {tryIt.error && <Notice ok={false} message={tryIt.error.message} />}
      {tryIt.data && <Tried r={tryIt.data} />}
      {save.error && <Notice ok={false} message={save.error.message} />}
      <div className="flex flex-wrap gap-2">
        <Button
          variant="brand"
          disabled={!dirty}
          loading={save.isPending}
          onClick={() =>
            save.mutate(
              {
                action: "save",
                task: t.task,
                text,
                note: note.trim() || undefined,
              },
              { onSuccess: onDone },
            )
          }
        >
          Save as draft
        </Button>
        <Button variant="ghost" onClick={onDone}>
          Cancel
        </Button>
      </div>
    </div>
  );
}

function Tried({ r }: { r: PromptTried }) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-xs text-muted-foreground">Tried on {r.sample}; not saved.</span>
      <div className="grid gap-3 md:grid-cols-2">
        <div className="flex min-w-0 flex-col gap-1.5">
          <span className="text-sm font-semibold">With this draft</span>
          <div className={pre}>{r.output}</div>
        </div>
        <div className="flex min-w-0 flex-col gap-1.5">
          <span className="text-sm font-semibold">In the index now</span>
          <div className={cn(pre, !r.current && "text-muted-foreground")}>{r.current ?? "Nothing written for it yet."}</div>
        </div>
      </div>
    </div>
  );
}
