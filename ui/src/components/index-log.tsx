import { AlertTriangle, Check, ChevronDown, CircleSlash, Copy, Loader2, Sparkles, X } from "lucide-react";
import { useState } from "react";
import type { IndexEvent, JobStatus } from "@/lib/api";
import { cn } from "@/lib/utils";

const fmt = (n: number) => n.toLocaleString();
const clock = (iso?: string | null) => (iso ? new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }) : "");
const isWarn = (e: IndexEvent) => e.level === "warn" || e.kind === "warning";
const isError = (e: IndexEvent) => e.level === "error" || e.kind === "error";

/** A duration in words: "<1 s", "42 s", "3 min 05 s", "1 h 12 min". */
export function duration(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 1) return "<1 s";
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min ${String(s % 60).padStart(2, "0")} s`;
  return `${Math.floor(s / 3600)} h ${String(Math.floor((s % 3600) / 60)).padStart(2, "0")} min`;
}

/** The step in progress, if any: the latest progress event after the latest step. */
export function currentProgress(events: IndexEvent[]): IndexEvent | null {
  for (let i = events.length - 1; i >= 0; i--) {
    const e = events[i];
    if (e.kind === "progress") return e;
    if (e.kind === "step" || e.kind === "done") return null;
  }
  return null;
}

/** A short status for the header: the current step, with its count when there is one. */
export function shortStatus(events: IndexEvent[]): string {
  const p = currentProgress(events);
  if (p && p.total) return `${p.step ?? p.message} ${fmt(p.done ?? 0)}/${fmt(p.total)}`;
  return [...events].reverse().find((e) => e.kind === "step")?.message ?? "";
}

type Step = {
  title: string;
  at?: string;
  end?: string;
  lines: IndexEvent[];
  progress: IndexEvent | null;
  tokensIn: number;
  tokensOut: number;
  usd: number;
  warnings: number;
  errors: number;
};

type State = "done" | "running" | "warning" | "failed" | "stopped";

/** Events grouped under the step they belong to. */
function steps(events: IndexEvent[], finished: string | null) {
  const out: Step[] = [];
  for (const e of events) {
    if (e.kind === "step" || !out.length) {
      out.push({ title: e.kind === "step" ? e.message : "Starting", at: e.timestamp, lines: [], progress: null, tokensIn: 0, tokensOut: 0, usd: 0, warnings: 0, errors: 0 });
      if (e.kind === "step") continue;
    }
    const s = out[out.length - 1];
    if (e.kind === "progress") {
      s.progress = e;
      s.tokensIn = Math.max(s.tokensIn, e.tokens_in ?? 0);
      s.tokensOut = Math.max(s.tokensOut, e.tokens_out ?? 0);
      s.usd = Math.max(s.usd, e.usd ?? 0);
      continue;
    }
    s.lines.push(e);
    if (isWarn(e)) s.warnings++;
    if (isError(e)) s.errors++;
  }
  out.forEach((s, i) => (s.end = out[i + 1]?.at ?? finished ?? undefined));
  return out;
}

function stateOf(s: Step, last: boolean, status: JobStatus): State {
  if (s.errors) return "failed";
  if (last && status === "running") return "running";
  if (last && status === "failed") return "failed";
  if (last && (status === "cancelled" || status === "interrupted")) return "stopped";
  return s.warnings ? "warning" : "done";
}

function Marker({ state }: { state: State }) {
  const base = "relative z-10 flex size-6 shrink-0 items-center justify-center rounded-full border bg-card [&_svg]:size-3.5";
  if (state === "running")
    return (
      <span className={cn(base, "border-brand/50 text-brand")}>
        <Loader2 className="animate-spin" />
      </span>
    );
  if (state === "failed")
    return (
      <span className={cn(base, "border-danger/40 bg-danger/10 text-danger")}>
        <X />
      </span>
    );
  if (state === "warning")
    return (
      <span className={cn(base, "border-warning/40 bg-warning/10 text-warning")}>
        <AlertTriangle />
      </span>
    );
  if (state === "stopped")
    return (
      <span className={cn(base, "text-muted-foreground")}>
        <CircleSlash />
      </span>
    );
  return (
    <span className={cn(base, "border-brand/40 bg-accent text-accent-foreground")}>
      <Check />
    </span>
  );
}

function Bar({ e }: { e: IndexEvent }) {
  const pct = e.total ? Math.min(100, ((e.done ?? 0) / e.total) * 100) : 0;
  return (
    <div className="mt-2.5">
      <div className="flex items-baseline justify-between gap-2 text-xs">
        <span className="text-muted-foreground">{e.step && e.step !== e.message ? e.message : "Progress"}</span>
        <span className="nums font-mono text-muted-foreground">
          {fmt(e.done ?? 0)} / {fmt(e.total ?? 0)}
        </span>
      </div>
      <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-muted">
        <div className="h-full rounded-full bg-brand transition-all duration-500" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}

function Lines({ lines }: { lines: IndexEvent[] }) {
  const [all, setAll] = useState(false);
  const shown = all ? lines : lines.slice(0, 8);
  return (
    <ul className="mt-2 flex flex-col gap-1 text-[13px]">
      {shown.map((e, i) => (
        <li
          key={i}
          className={cn(
            "flex items-start gap-2 rounded-md px-2 py-1",
            isError(e) ? "bg-danger/8 text-danger" : isWarn(e) ? "bg-warning/8 text-warning" : e.kind === "done" ? "text-foreground" : "text-muted-foreground",
          )}
        >
          {isError(e) ? (
            <X className="mt-0.5 size-3.5 shrink-0" />
          ) : isWarn(e) ? (
            <AlertTriangle className="mt-0.5 size-3.5 shrink-0" />
          ) : e.kind === "done" ? (
            <Check className="mt-0.5 size-3.5 shrink-0 text-brand" />
          ) : (
            <span className="mt-[7px] size-1 shrink-0 rounded-full bg-current opacity-60" />
          )}
          <span className="min-w-0 flex-1 [overflow-wrap:anywhere]">{e.message}</span>
          <span className="shrink-0 font-mono text-[10.5px] opacity-70">{clock(e.timestamp)}</span>
        </li>
      ))}
      {lines.length > 8 && (
        <li>
          <button className="px-2 text-xs text-muted-foreground hover:text-foreground" onClick={() => setAll((a) => !a)}>
            {all ? "Show fewer" : `Show ${lines.length - 8} more`}
          </button>
        </li>
      )}
    </ul>
  );
}

function StepRow({ step, state, last }: { step: Step; state: State; last: boolean }) {
  const [open, setOpen] = useState<boolean | null>(null);
  const expanded = open ?? (state === "running" || state === "failed" || state === "warning");
  const hasMore = step.lines.length > 0;
  const took = step.at && step.end ? duration(new Date(step.end).getTime() - new Date(step.at).getTime()) : state === "running" && step.at ? duration(Date.now() - new Date(step.at).getTime()) : "";
  return (
    <li className="relative flex gap-3 pb-4 last:pb-0">
      {!last && <span className="absolute top-6 bottom-0 left-3 w-px bg-border" />}
      <Marker state={state} />
      <div className="min-w-0 flex-1 pt-0.5">
        <button
          className={cn("flex w-full items-center gap-2 text-left", !hasMore && "cursor-default")}
          onClick={() => hasMore && setOpen(!expanded)}
          aria-expanded={hasMore ? expanded : undefined}
        >
          <span className={cn("text-sm font-medium", state === "stopped" && "text-muted-foreground")}>{step.title}</span>
          {step.warnings > 0 && <span className="rounded bg-warning/12 px-1.5 font-mono text-[10.5px] text-warning">{step.warnings} warning{step.warnings > 1 && "s"}</span>}
          {step.errors > 0 && <span className="rounded bg-danger/12 px-1.5 font-mono text-[10.5px] text-danger">{step.errors} error{step.errors > 1 && "s"}</span>}
          {hasMore && !step.warnings && !step.errors && <span className="font-mono text-[10.5px] text-muted-foreground">{step.lines.length}</span>}
          {hasMore && <ChevronDown className={cn("size-3.5 text-muted-foreground transition-transform", expanded && "rotate-180")} />}
          <span className="ml-auto flex shrink-0 items-center gap-3 font-mono text-[11px] text-muted-foreground">
            {step.tokensIn > 0 && (
              <span className="hidden items-center gap-1 sm:flex" title={`${fmt(step.tokensIn)} tokens in · ${fmt(step.tokensOut)} out`}>
                <Sparkles className="size-3" />
                {step.usd < 0.01 ? "<$0.01" : `$${step.usd.toFixed(2)}`}
              </span>
            )}
            <span className="nums">{took}</span>
            <span className="hidden md:inline">{clock(step.at)}</span>
          </span>
        </button>
        {state === "running" && step.progress && <Bar e={step.progress} />}
        {expanded && hasMore && <Lines lines={step.lines} />}
      </div>
    </li>
  );
}

function Stat({ label, value, tone }: { label: string; value: string; tone?: "warning" | "danger" }) {
  return (
    <div className="flex flex-col gap-1 px-4 py-3">
      <span className="eyebrow">{label}</span>
      <span className={cn("nums text-[15px] font-semibold tracking-tight", tone === "warning" && "text-warning", tone === "danger" && "text-danger")}>{value}</span>
    </div>
  );
}

/** The indexing log as a timeline: one row per step with its time, warnings and errors, a live bar for the step in progress. */
export function IndexLog({
  events,
  running,
  status,
  started,
  finished,
  summary = true,
}: {
  events: IndexEvent[];
  running: boolean;
  status?: JobStatus;
  started?: string | null;
  finished?: string | null;
  summary?: boolean;
}) {
  const [plain, setPlain] = useState(false);
  const state: JobStatus = status ?? (running ? "running" : "succeeded");
  const list = steps(events, finished ?? null);
  if (events.length === 0) return null;
  const warnings = list.reduce((n, s) => n + s.warnings, 0);
  const errors = list.reduce((n, s) => n + s.errors, 0);
  const usd = list.reduce((n, s) => n + s.usd, 0);
  const tokens = list.reduce((n, s) => n + s.tokensIn + s.tokensOut, 0);
  const from = started ?? events[0]?.timestamp;
  const to = finished ?? (running ? new Date().toISOString() : events[events.length - 1]?.timestamp);
  const text = events
    .filter((e) => e.kind !== "progress")
    .map((e) => `${e.timestamp ?? ""} ${e.level.toUpperCase().padEnd(5)} ${e.kind === "step" ? "▸ " : "  "}${e.message}`)
    .join("\n");

  return (
    <div className="flex flex-col gap-4">
      {summary && (
        <div className="grid grid-cols-2 divide-x divide-y rounded-lg border sm:grid-cols-4 sm:divide-y-0">
          <Stat label="Duration" value={from && to ? duration(new Date(to).getTime() - new Date(from).getTime()) : "—"} />
          <Stat label="Steps" value={String(list.length)} />
          <Stat label="Issues" value={errors ? `${errors} error${errors > 1 ? "s" : ""}` : warnings ? `${warnings} warning${warnings > 1 ? "s" : ""}` : "None"} tone={errors ? "danger" : warnings ? "warning" : undefined} />
          <Stat label="LLM" value={tokens ? `${usd < 0.01 ? "<$0.01" : `$${usd.toFixed(2)}`} · ${fmt(tokens)} tok` : "Not used"} />
        </div>
      )}

      {plain ? (
        <pre className="max-h-[32rem] overflow-auto rounded-lg border bg-muted/40 p-3 font-mono text-[11.5px] leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere]">{text}</pre>
      ) : (
        <ol className="flex flex-col">
          {list.map((s, i) => (
            <StepRow key={i} step={s} last={i === list.length - 1} state={stateOf(s, i === list.length - 1, state)} />
          ))}
        </ol>
      )}

      <div className="flex items-center gap-3 text-xs text-muted-foreground">
        <button className="hover:text-foreground" onClick={() => setPlain((p) => !p)}>
          {plain ? "Timeline" : "Plain log"}
        </button>
        {plain && (
          <button className="flex items-center gap-1 hover:text-foreground" onClick={() => navigator.clipboard?.writeText(text)}>
            <Copy className="size-3" /> Copy
          </button>
        )}
      </div>
    </div>
  );
}
