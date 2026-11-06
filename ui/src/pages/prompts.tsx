import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { PageHeader } from "@/components/page-header";
import { ConfirmButton } from "@/components/confirm";
import { Notice } from "@/components/notice";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { TeamSection, useUnsavedWarning } from "@/components/team-settings";
import { api, type PromptAction, type PromptTask, type PromptTaskName, type PromptTried } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Section, SettingsTabs } from "./settings";

const textarea =
  "min-h-48 w-full rounded-md border border-input bg-card px-3 py-2 font-mono text-[13px] font-normal leading-relaxed outline-none transition-[border-color,box-shadow] duration-100 hover:border-muted-foreground/50 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/20 max-sm:text-base";
const pre = "whitespace-pre-wrap rounded-md bg-muted/50 px-3 py-2 font-mono text-[13px] font-normal leading-relaxed [overflow-wrap:anywhere]";

const modelKey = { summary: "llm_model_summary", guide: "llm_model_guide", flow: "llm_model_flow" } as const;

const about: Record<PromptTaskName, { title: string; writes: string }> = {
  summary: {
    title: "Summaries",
    writes: "A sentence and search questions for each of the most used functions and types.",
  },
  guide: {
    title: "Style guides",
    writes: "Each section of a repository's style guide, from evidence measured in its code.",
  },
  flow: {
    title: "Walkthroughs",
    writes: "A short walkthrough of each flow, from its entry point and call trace.",
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
  const team = useQuery({ queryKey: ["team"], queryFn: api.team });
  const writable = me.data?.mode !== "server" || me.data?.role === "admin";
  const model = (t: PromptTaskName) => team.data?.settings[modelKey[t]] || team.data?.settings.llm_model || team.data?.defaults.llm_model;

  return (
    <div className="flex max-w-4xl flex-col gap-5">
      <PageHeader title="Models &amp; prompts">
        Which models indexing and search use, and what indexing asks the LLM. A saved prompt is a draft until someone activates it; deactivating goes back to the built-in prompt. A
        change, to a model or a prompt, applies to what the next index writes, partial or full; nothing already written is redone for it.
      </PageHeader>
      <SettingsTabs />
      <TeamSection part="models" />
      {prompts.error && <Notice ok={false} message={prompts.error.message} />}
      {!prompts.data
        ? !prompts.error && <Skeleton className="h-96 rounded-lg" />
        : prompts.data.tasks.map((t) => <TaskCard key={t.task} t={t} model={model(t.task)} version={prompts.data.version} writable={writable} />)}
    </div>
  );
}

function TaskCard({ t, model, version, writable }: { t: PromptTask; model?: string; version: number | null; writable: boolean }) {
  const change = useChange(version);
  const [editing, setEditing] = useState(false);
  const [open, setOpen] = useState<number | null>(null);
  const a = about[t.task];
  const inUse = t.active === null ? "the built-in prompt" : `version ${t.active}`;
  const activeVersion = t.versions.find((v) => v.version === t.active);
  const current = activeVersion?.text ?? t.builtin;
  const busy = (x: PromptAction["action"], n?: number) =>
    change.isPending && change.variables?.action === x && (n === undefined || ("n" in change.variables && change.variables.n === n));
  const versions = [...t.versions].sort((x, y) => y.version - x.version);

  return (
    <Section title={a.title} description={`${a.writes} In use: ${inUse}${model ? `, on ${model}` : ""}.`}>
      {t.builtin_changed !== null && (
        <Notice
          ok={false}
          message={`This release changed the built-in prompt since version ${t.builtin_changed} was written. Compare them, and save a new version or deactivate to use the new built-in one.`}
        />
      )}
      <ul className="flex flex-col divide-y divide-border rounded-md border border-border">
        <Row
          label="Built-in"
          detail="Comes with comindly"
          active={t.active === null}
          open={open === 0}
          onToggle={() => setOpen(open === 0 ? null : 0)}
          text={withSections(t.builtin, t.sections, {})}
        />
        {versions.map((v) => (
          <Row
            key={v.version}
            label={`Version ${v.version}`}
            note={v.note}
            detail={`${v.author}, ${new Date(v.created_at).toLocaleString()}`}
            active={t.active === v.version}
            open={open === v.version}
            onToggle={() => setOpen(open === v.version ? null : v.version)}
            text={withSections(v.text, t.sections, v.sections ?? {})}
            actions={
              writable &&
              (t.active === v.version ? (
                <ConfirmButton
                  size="sm"
                  variant="ghost"
                  danger={false}
                  question="Back to the built-in prompt for what the next index writes?"
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
                    question="Use this version for what the next index writes?"
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
          <Editor t={t} start={current} startSections={activeVersion?.sections ?? {}} version={version} onDone={() => setEditing(false)} />
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

/** A style guide prompt as text, with each section's guidance where a version changed it. */
function withSections(text: string, builtin: { title: string; guidance: string }[], changed: Record<string, string>) {
  const lines = builtin.filter((b) => changed[b.title]).map((b) => `\n\n${b.title}: ${changed[b.title]}`);
  return text + lines.join("");
}

function Editor({ t, start, startSections, version, onDone }: { t: PromptTask; start: string; startSections: Record<string, string>; version: number | null; onDone: () => void }) {
  const [text, setText] = useState(start);
  const [sections, setSections] = useState<Record<string, string>>(() => Object.fromEntries(t.sections.map((b) => [b.title, startSections[b.title] ?? b.guidance])));
  const [note, setNote] = useState("");
  const [sample, setSample] = useState("");
  const [section, setSection] = useState(t.sections[0]?.title ?? "");
  const save = useChange(version);
  const changedSections = Object.fromEntries(Object.entries(sections).filter(([k, v]) => v.trim() !== (t.sections.find((b) => b.title === k)?.guidance ?? "")));
  const tryIt = useMutation({
    mutationFn: () => api.tryPrompt({ task: t.task, text, sections: changedSections, sample: sample.trim() || undefined, section: t.task === "guide" ? section : undefined }),
    meta: { inline: true, quiet: true },
  });
  const dirty = text.trim() !== start.trim() || JSON.stringify(changedSections) !== JSON.stringify(startSections);
  useUnsavedWarning(dirty);
  const placeholder = {
    summary: "A symbol id; empty for the most used one",
    guide: "A repository; empty for the first in the list",
    flow: "An entry point's id; empty for the furthest-reaching flow",
  }[t.task];

  return (
    <div className="flex flex-col gap-3 rounded-md border border-border p-3">
      <label className="flex flex-col gap-1.5 text-sm font-semibold">
        Instructions
        <textarea className={textarea} value={text} onChange={(e) => setText(e.target.value)} autoFocus />
      </label>
      {t.sections.length > 0 && (
        <details>
          <summary className="cursor-pointer text-sm font-semibold">What each section covers</summary>
          <span className="mt-1 block text-xs text-muted-foreground">Sent with that section's evidence. Leave one as it is to keep the built-in guidance.</span>
          <div className="mt-2 flex flex-col gap-3">
            {t.sections.map((b) => (
              <label key={b.title} className="flex flex-col gap-1.5 text-sm font-semibold">
                {b.title}
                <textarea className={cn(textarea, "min-h-20")} value={sections[b.title] ?? ""} onChange={(e) => setSections((x) => ({ ...x, [b.title]: e.target.value }))} />
              </label>
            ))}
          </div>
        </details>
      )}
      <div className="flex flex-col gap-1.5">
        <span className="text-sm font-semibold">Always added</span>
        <span className="text-xs text-muted-foreground">The reply format and its JSON schema, which comindly reads back; they cannot be changed.</span>
        <div className={cn(pre, "max-h-64 overflow-auto text-muted-foreground")}>{t.format}</div>
      </div>
      <label className="flex flex-col gap-1.5 text-sm font-semibold">
        Note
        <Input value={note} onChange={(e) => setNote(e.target.value)} placeholder="What changed, for whoever activates it" />
      </label>
      <div className="flex flex-wrap items-end gap-2">
        <label className="flex min-w-0 flex-1 flex-col gap-1.5 text-sm font-semibold">
          Try it on
          <Input value={sample} onChange={(e) => setSample(e.target.value)} placeholder={placeholder} />
        </label>
        {t.task === "guide" && (
          <label className="flex min-w-0 flex-col gap-1.5 text-sm font-semibold">
            Section
            <select className="h-9 rounded-md border border-input bg-card px-2 text-sm font-normal" value={section} onChange={(e) => setSection(e.target.value)}>
              {t.sections.map((b) => (
                <option key={b.title}>{b.title}</option>
              ))}
            </select>
          </label>
        )}
        <Button variant="outline" disabled={!text.trim()} loading={tryIt.isPending} onClick={() => tryIt.mutate()}>
          Try
        </Button>
      </div>
      {tryIt.error && <Notice ok={false} message={tryIt.error.message} />}
      {tryIt.data && <Tried r={tryIt.data} />}
      {save.error && <Notice ok={false} message={save.error.message} />}
      <div className="flex flex-wrap gap-2">
        <Button
          variant="brand"
          disabled={!dirty}
          loading={save.isPending}
          onClick={() => save.mutate({ action: "save", task: t.task, text, sections: changedSections, note: note.trim() || undefined }, { onSuccess: onDone })}
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
